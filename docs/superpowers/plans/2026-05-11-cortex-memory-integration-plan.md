# Cortex memory integration for xvision — implementation plan

Status: plan
Date: 2026-05-11
Author: research session w/ Edward
Related: `docs/superpowers/research/2026-05-11-autoimproving-memory-survey.md`

## Goal

Give xvision agents (intern, trader, risk, executor, autooptimizer,
scheduler) a self-hosted, auto-improving memory layer using
[gambletan/cortex](https://github.com/gambletan/cortex) as the
storage and inference backend.

Cortex is consumed **as an HTTP sidecar** (binary
`cortex-http`, image `ghcr.io/gambletan/cortex/cortex-http:latest`)
running alongside `xvn` in the same `docker-compose` stack. xvision
agents talk to it over `http://127.0.0.1:3315` via a new
Rust crate `xvision-memory` that wraps the REST API.

Deployment assumption: **one operator per install.** Each xvn
deployment has a single human user; cortex's per-user identity model
is reused at a finer grain to scope memory *per StrategyBundle*
(per `agent_id`).

## Why cortex (recap)

From the survey:

- **MIT-licensed pure Rust**, 3.8 MB binary, no runtime deps.
- **156 µs ingest / 568 µs search** on M-series Mac — fast enough
  to call from the trading hot path.
- **Four-tier memory model** (Working / Episodic / Semantic /
  Procedural) matches the lifecycle of trading information:
  active cycle context → completed-cycle traces → distilled
  beliefs → learned routines.
- **Bayesian belief system** — `observe_belief(key, value,
  confidence)` updates posterior confidence via Bayes rule
  rather than overwriting. This is the right model for trading
  beliefs that should converge as evidence accumulates and
  invert when refuted.
- **Native namespace isolation** (shipped in v1.5), the
  primitive we'll use for per-`agent_id` scoping.
- **Native contradiction detection** with confidence scores.
- **Consolidation engine** — episodic → semantic promotion +
  decay + sweep, runnable on a cron via the existing 2c
  scheduler.
- **Local-only by default** — cortex-http binds to 127.0.0.1;
  no data leaves the host.
- **Already ships a Docker image** and an MCP server; minimal
  packaging work for us.

## Architecture

```
┌────────────────────────────────────────────────────────────────┐
│ xvn (Rust workspace)                                           │
│                                                                │
│   intern ─┐                                                    │
│   trader ─┤                                                    │
│   risk   ─┼─►  xvision-memory  ──HTTP──►  cortex-http          │
│  executor─┤      (Rust crate)                                  │
│ autoresch─┤                                                    │
│ scheduler─┘                                                    │
│                                                                │
└────────────────────────────────────────────────────────────────┘
                                              │
                                              ▼
                                  ┌───────────────────────┐
                                  │ cortex-http :3315     │
                                  │ (127.0.0.1 only)      │
                                  │  ┌─────────────────┐  │
                                  │  │ cortex-core     │  │
                                  │  │  ├─ Working     │  │
                                  │  │  ├─ Episodic    │  │
                                  │  │  ├─ Semantic    │  │
                                  │  │  └─ Procedural  │  │
                                  │  │  ├─ Beliefs     │  │
                                  │  │  ├─ Facts       │  │
                                  │  │  └─ Consolidate │  │
                                  │  └─────────────────┘  │
                                  │  ┌─────────────────┐  │
                                  │  │ SQLite          │  │
                                  │  │ ~/.cortex/      │  │
                                  │  │   memory.db     │  │
                                  │  └─────────────────┘  │
                                  └───────────────────────┘
```

## Mapping cortex primitives to xvision concepts

| xvision concept | Cortex primitive | Notes |
|---|---|---|
| `agent_id` (StrategyBundle ULID) | **namespace** | Hard isolation. Each StrategyBundle gets its own namespace so beliefs / facts / episodic memories don't cross-contaminate between strategies. |
| `cycle_id` (briefing→decision→outcome) | metadata field on memory + working-memory session id | Cycle ids are short-lived; we tag the cycle id in the memory body and use cortex's working-memory tier for in-flight context. |
| Pipeline stage (intern / trader / risk / executor) | **channel** | The `channel` field tags origin: `"intern"`, `"trader"`, `"risk"`, `"executor"`, `"autooptimizer"`, `"eod"`, `"scheduled_task"`. Lets us query "what does the risk gate think about strategy X?" without text search. |
| User config + global knowledge | **`shared` namespace** | One reserved global namespace (`"shared"`) for user preferences, market regime beliefs, autooptimizer findings, and anything the operator wants every strategy to see. |
| Realized P&L / outcome | **belief observation + episodic memory** | Two writes per cycle close: (a) `belief_observe` on the relevant strategy-level beliefs with new confidence based on win/loss, (b) `memory_ingest` on the full `(briefing, decision, outcome, pnl)` tuple as an episodic memory. |
| Contradictions (e.g. trader says BUY, risk vetoes) | **fact contradictions API** | Use `/v1/facts/contradictions` to surface conflicts in declared facts; let the consolidation engine resolve. |
| AutoOptimizer findings | **facts + semantic memory in `shared`** | Each finding lands as one structured fact (subject-predicate-object) plus a longer-form episodic memory. |
| EOD reports | **episodic in `shared`**, channel `"eod"` | Stored verbatim for retrieval + auto-summarized via consolidation. |

## The `xvision-memory` crate

New workspace crate that wraps the cortex HTTP API. Keep the surface
small and trait-shaped so a future swap (e.g. to embedded
`cortex-core` or LanceDB) doesn't ripple through callers.

```rust
// xvision-memory/src/lib.rs

pub struct MemoryClient {
    base: String,         // http://127.0.0.1:3315
    http:  reqwest::Client,
}

#[async_trait]
pub trait MemoryStore {
    /// Ingest a memory. Namespace = agent_id or "shared".
    async fn ingest(&self, m: NewMemory) -> Result<MemoryId>;

    /// Multi-signal search (vector + temporal + salience + channel).
    async fn search(&self, q: Query) -> Result<Vec<MemoryHit>>;

    /// Generate token-budgeted LLM context for the next prompt.
    async fn context(&self, c: ContextRequest) -> Result<String>;

    /// Store a structured fact.
    async fn add_fact(&self, f: Fact) -> Result<()>;

    /// Update a Bayesian belief with new evidence.
    async fn observe_belief(&self, b: BeliefObservation) -> Result<Belief>;

    /// List beliefs above a confidence threshold.
    async fn beliefs(&self, ns: &str, threshold: f32) -> Result<Vec<Belief>>;

    /// Run the consolidation cycle (decay + promote + sweep).
    async fn consolidate(&self, ns: &str) -> Result<ConsolidationReport>;

    /// Check for fact contradictions in a namespace.
    async fn contradictions(&self, ns: &str) -> Result<Vec<Contradiction>>;
}

pub struct NewMemory {
    pub namespace: String,           // agent_id or "shared"
    pub text:      String,
    pub channel:   Channel,           // intern, trader, risk, ...
    pub cycle_id:  Option<String>,    // tagged into metadata
    pub salience:  Option<f32>,
    pub embedding: Option<Vec<f32>>,  // optional; cortex computes one if absent
    pub tags:      Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub enum Channel {
    Intern, Trader, Risk, Executor,
    AutoOptimizer, Eod, ScheduledTask,
    UserConfig, MarketRegime,
}
```

Implementation notes:
- HTTP client: `reqwest` with connection pool, 5 s timeout for
  hot-path reads, 30 s for consolidation.
- Serde for request/response shapes — generate them from the
  endpoint table in cortex's README (see below).
- Embeddings: leave `None` and let cortex compute via its
  bundled `all-MiniLM-L6-v2` ONNX model. Revisit if we want
  control of the embedding model.
- Errors: `MemoryError` enum (`Transport`, `NotFound`,
  `ContradictionUnresolved`, etc.) — never let a memory failure
  fail a trading decision; degrade to "no memory" mode.

## Endpoint usage table

| xvision call | Cortex endpoint | Frequency |
|---|---|---|
| `ingest(NewMemory)` | `POST /v1/memories` | every cycle stage, autooptimizer, eod |
| `search(Query)` | `POST /v1/memories/search` | trader pre-decision, risk pre-veto, autooptimizer |
| `context(...)` | `GET /v1/memories/context` | start of each cycle for intern; per-stage budget |
| `add_fact(Fact)` | `POST /v1/facts` | autooptimizer, executor on confirmation |
| `observe_belief(...)` | `POST /v1/beliefs/observe` | on cycle close, risk verdict, EOD |
| `beliefs(ns, thr)` | `GET /v1/beliefs?ns=...&min_confidence=...` | trader pre-decision |
| `consolidate(ns)` | `POST /v1/memories/consolidate` | nightly scheduled task |
| `contradictions(ns)` | `POST /v1/facts/contradictions` | nightly + after autooptimizer batch |
| `export()` | `GET /v1/export` | nightly backup |
| `import(json)` | `POST /v1/import` | restore / migration |

(Endpoint shapes verified against the cortex README on 2026-05-11;
re-verify before merging since cortex is at v1.6 and APIs are still
evolving.)

## Integration points in the xvn pipeline

### Cycle start (intern)
1. `context({namespace: agent_id, max_tokens: 1500, channel:
   Intern})` to pull what we know about this strategy.
2. `context({namespace: "shared", max_tokens: 500, channel:
   MarketRegime})` for general market beliefs.
3. Optional `beliefs(agent_id, threshold: 0.6)` to inject
   high-confidence strategy beliefs into the briefing.

### Trader pre-decision
1. `search({namespace: agent_id, query: "outcomes for setups
   similar to {brief_summary}", limit: 5})` — episodic
   recall for similar past cycles.
2. Read returned `outcome` tuples; bias the trader's prompt.

### Risk gate verdict
1. After the gate runs, `ingest` the verdict as channel `Risk`,
   tagged with `cycle_id`.
2. If the verdict was a veto, `observe_belief({namespace:
   agent_id, key: "strategy_overconfident_in_{regime}",
   value: true, confidence: 0.7})`. Bayesian update will
   compound over repeated vetoes.

### Executor on outcome close
1. `ingest` the full `(briefing, decision, outcome, pnl,
   slippage, regime)` tuple as episodic with high salience.
2. `observe_belief` on:
   - `strategy_works_in_{regime}` with confidence derived
     from realized return vs. expected.
   - `setup_pattern_{hash}_is_profitable` for pattern-based
     beliefs.
3. `add_fact` for any newly confirmed structured fact
   (e.g. `(SYM, has_overnight_gap_tendency, true)`).

### AutoOptimizer
1. Each finding → `add_fact` in the `shared` namespace.
2. Each report → `ingest` as channel `AutoOptimizer`.
3. After a batch: call `contradictions("shared")` and surface
   any conflicts to the operator for manual resolution (or to
   the consolidation engine if confidence-weighted resolution
   is safe).

### EOD report
1. Persist the rendered report → `ingest` as channel `Eod` in
   `shared` namespace, salience 0.9.
2. Trigger `consolidate("shared")` to promote recurring
   patterns into semantic facts.

### Nightly scheduled task
1. For each `agent_id` known to the wallet plan: call
   `consolidate(agent_id)` and `contradictions(agent_id)`.
2. Call `consolidate("shared")` once.
3. Snapshot memory via `export()` to
   `~/.cortex/backups/{date}.json` and rotate (keep last 30).

## Deployment

### `docker-compose.yml` additions

```yaml
services:
  cortex:
    image: ghcr.io/gambletan/cortex/cortex-http:latest
    container_name: xvision-cortex
    restart: unless-stopped
    volumes:
      - ./data/cortex:/data
    # Bind only to localhost on the host — never expose outside the box.
    ports:
      - "127.0.0.1:3315:3315"
    command: ["--port", "3315", "--db", "/data/memory.db"]
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://localhost:3315/health"]
      interval: 30s
      timeout: 3s
      retries: 3

  xvn:
    # existing xvn service
    depends_on:
      cortex:
        condition: service_healthy
    environment:
      XVISION_MEMORY_URL: "http://cortex:3315"
```

Single-binary users (no Docker) get a `xvn memory install` helper
that downloads the cortex-http binary, drops it in
`~/.local/bin`, and writes a launchd / systemd unit.

### Config

Add to `xvn` config:

```toml
[memory]
enabled       = true
base_url      = "http://127.0.0.1:3315"
default_ns    = "shared"
timeout_ms    = 5_000
fail_open     = true       # if cortex is down, degrade silently
context_budget_tokens = 1500
nightly_consolidate   = true
nightly_export        = true
```

### Bootstrapping the operator's namespace

On first run, `xvn memory init` does:
1. Health-check cortex.
2. Write canonical user preferences via `POST /v1/preferences`.
3. Seed `shared` namespace with market-regime baseline facts
   if the operator has a seed file.

### Per-strategy bootstrap

When a new `StrategyBundle` is registered, `xvn strategy add`
also calls `xvn memory new-ns {agent_id}` (a thin wrapper that
just makes sure the first ingest uses the right namespace; cortex
creates namespaces lazily on first use).

## Auto-improvement loops

Three loops, all driven by the existing 2c scheduler:

### 1. Outcome ingestion (online, per cycle)

Runs the moment the executor closes a cycle. Code lives in the
executor stage. Does:
- `ingest` the full cycle tuple (episodic).
- `observe_belief` on the strategy- and regime-level beliefs.
- `add_fact` on any newly confirmed structured fact.

### 2. Consolidation (nightly)

Cron: `0 3 * * *` (3 AM local). For each namespace:
- `POST /v1/memories/consolidate` → episodic → semantic
  promotion, decay of stale entries, pattern extraction.
- Capture the report (`{promoted, decayed, merged}`) into
  the xvision DB for observability.

### 3. Contradiction resolution (nightly + ad-hoc)

After each consolidation run:
- `POST /v1/facts/contradictions` per namespace.
- For confidence-resolvable contradictions, apply a Bayesian
  resolver (let the lower-confidence side decay).
- For ties / high-confidence conflicts, write a row to a new
  `memory_disputes` table in the xvision DB and surface in the
  daily 2d dashboard for operator review.

## Schema notes

We don't replicate cortex tables in our SQLite. The xvision DB
gets two thin tables for cross-system observability:

```sql
-- consolidation runs we triggered
CREATE TABLE memory_consolidation_runs (
  id          INTEGER PRIMARY KEY,
  namespace   TEXT NOT NULL,
  ran_at      TIMESTAMP NOT NULL,
  promoted    INTEGER NOT NULL,
  decayed     INTEGER NOT NULL,
  merged      INTEGER NOT NULL
);

-- contradictions surfaced for operator review
CREATE TABLE memory_disputes (
  id          INTEGER PRIMARY KEY,
  namespace   TEXT NOT NULL,
  detected_at TIMESTAMP NOT NULL,
  fact_a      TEXT NOT NULL,
  fact_b      TEXT NOT NULL,
  conf_a      REAL NOT NULL,
  conf_b      REAL NOT NULL,
  resolution  TEXT,           -- 'auto', 'manual', null
  resolved_at TIMESTAMP
);
```

## Phased rollout

### Phase 1 — wire it up (week 1)
- Add cortex to `docker-compose.yml`; ship the
  `ghcr.io/gambletan/cortex/cortex-http:latest` image.
- Create `xvision-memory` crate with `MemoryClient` and
  `MemoryStore` trait.
- Plumb the client into the xvn config layer; add the `[memory]`
  section and a `--no-memory` CLI flag.
- Implement `ingest`, `search`, `context`, `consolidate`,
  `export`. Skip beliefs/facts/contradictions for now.
- Integration test: spin up cortex in CI via testcontainers,
  ingest a cycle, retrieve it.

### Phase 2 — episodic learning (week 2)
- Hook the executor to ingest the cycle tuple on close.
- Hook the trader to pull context + episodic search before
  deciding.
- Hook the intern to pull strategy-level context at cycle start.
- A/B: run one strategy with memory enabled vs. one without
  for a week; measure decision-quality deltas.

### Phase 3 — beliefs and facts (week 3)
- Add `observe_belief` + `add_fact` + `beliefs` to the client.
- Hook the risk gate to observe overconfidence beliefs on veto.
- Hook the executor to observe regime/setup beliefs on close.
- Hook the autooptimizer to write facts.

### Phase 4 — consolidation and contradictions (week 4)
- Wire the nightly scheduler.
- Implement `memory_consolidation_runs` and `memory_disputes`
  tables.
- Add a 2d dashboard panel for memory stats and disputes.

### Phase 5 — backup / restore + portability (week 5+)
- Nightly export to `~/.cortex/backups/`.
- `xvn memory backup` / `xvn memory restore` CLI verbs.
- For the marketplace: when a `StrategyBundle` is shipped
  cross-host, export its namespace and ship the JSON with the
  bundle so the new host inherits learned beliefs.

## Risks and mitigations

**Maturity.** Cortex has 1 GitHub star at time of writing, 1
contributor, and v1.6 was released in the last few weeks. Even
with strong README claims, this is unproven in production.

Mitigations:
- Hide it behind the `MemoryStore` trait from day one — never
  let cortex-specific types leak into pipeline code.
- `fail_open: true` by default — a cortex outage degrades to
  "no memory" rather than blocking trades.
- Pin to a specific cortex-http image tag in production; review
  changelog before bumping.
- Nightly JSON export means we can migrate off cortex without
  losing data — the export is a stable serialization, and we
  can replay it into any future store.

**Bayesian belief calibration.** Observed confidences depend on
how we map win/loss/pnl to `(value, confidence)` arguments. Bad
mapping = miscalibrated beliefs = bad future decisions.

Mitigations:
- Document the mapping function and unit-test it.
- Backtest belief evolution against historical cycles before
  letting beliefs influence live decisions.
- Start with `observe_belief` confidences in a narrow band
  (0.55–0.7) so beliefs converge slowly until we trust the loop.

**Hot-path latency.** 156 µs ingest / 568 µs search is fast, but
those are local-bench numbers without HTTP overhead. Real
sidecar latency will be a few ms per call.

Mitigations:
- Issue context + belief reads in parallel (`tokio::join!`) at
  cycle start.
- Set 5 s client timeout but `fail_open` so a slow cortex
  doesn't stall a cycle.
- If aggregate memory latency exceeds 50 ms per cycle, move to
  embedding cortex-core directly (drop sidecar, link the
  crate). The `MemoryStore` trait makes this a localized
  refactor.

**Belief / fact schema drift.** Cortex's structured fact format
is subject-predicate-object; if we encode trading concepts
sloppily we'll get a graph full of inconsistent predicates.

Mitigations:
- Centralize predicate vocabularies in `xvision-memory::vocab`.
- Code-review every new predicate the same way we code-review
  a new DB column.

**Single-author dependency.** If the cortex maintainer steps
away, we own the fork.

Mitigations:
- Vendor a known-good cortex-http binary as a release asset on
  the xvision GitHub release; pin to it.
- Stay current enough that we can fork cleanly if needed —
  read the source as part of code review, not as a black box.

## Open questions

1. **Namespace creation API.** The README mentions namespace
   isolation but doesn't show an explicit "create namespace"
   endpoint. Need to verify whether namespaces are created
   implicitly on first ingest (likely) or via a dedicated call.
2. **Embedding model swap.** Cortex bundles `all-MiniLM-L6-v2`
   ONNX. Adequate for general text but unclear for trading
   jargon. Benchmark on real cycle text before committing.
3. **Consolidation cost.** What's the per-namespace
   consolidation runtime at e.g. 10 k episodic entries?
   Affects whether nightly is enough or we need hourly for
   active strategies.
4. **Multi-host marketplace.** When `StrategyBundle`s become
   tokenized and shipped between operators, do we ship the
   memory namespace too, or does each operator's memory of the
   strategy live in isolation? Strong arguments both ways —
   defer to the marketplace plan amendment.
5. **HTTP vs embedded.** If Phase 2 A/B shows memory adds real
   value but sidecar latency is a problem, switch to
   `cortex-core` in-process. Should be a one-day refactor
   thanks to the trait, but plan ahead for the build-time
   coupling.

## References

- Cortex repo & README: https://github.com/gambletan/cortex
- Cortex Docker image:
  `ghcr.io/gambletan/cortex/cortex-http:latest`
- Memory survey (this repo):
  `docs/superpowers/research/2026-05-11-autoimproving-memory-survey.md`
- xvision terminology: `CLAUDE.md` and
  `docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md`
- Scheduler (plan 2c): `docs/superpowers/plans/2c-*.md`
- Dashboard (plan 2d): `docs/superpowers/plans/2d-*.md`
