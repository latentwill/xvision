# Karpathy Autoresearcher — Design

> **Status:** Draft for user review · 2026-05-09 (revised same day to decouple from ERC-8004)
> **Author:** xvision hackathon team
> **Companion specs:** [Marketplace Plugin](./2026-05-09-marketplace-plugin-design.md) · [Eval Engine](./2026-05-08-eval-engine-design.md) · [Strategy Creation Engine](./2026-05-08-strategy-creation-engine-design.md) · [Smart Contract Surface](./2026-05-08-smart-contract-surface-design.md)
> **Reference:** github.com/karpathy/autoresearch (March 2026)
> **Hackathon deadline:** 2026-06-15 (5 weeks)

---

## 1. Purpose, scope, and personas

The Karpathy Autoresearcher is xvision's evening cycle: an LLM-driven loop that proposes mutations to existing strategy variants, paper-tests them on day-of trades plus a held-out window, and commits surviving children into a content-addressed lineage. The loop runs entirely off-chain. Whether any of its outputs ever reach Mantle is a separate, optional concern handled by the [Marketplace Plugin](./2026-05-09-marketplace-plugin-design.md).

### 1.1 Personas

xvision serves two user personas. The autoresearcher core is built for **Persona A**; the marketplace plugin extends it for **Persona B**.

| | **Persona A — trader/researcher** | **Persona B — marketplace participant** |
|---|---|---|
| Goal | Better trading strategies; watch evolution | Publish lineages on-chain; reputation/provenance |
| Cares about chain? | No | Yes |
| Cares about NFTs? | No | Yes |
| Wants the dashboard? | Yes — live cycle, tree, ladder | Yes + a marketplace tab |
| Install | `xvn autoresearch start` (default build includes marketplace) | Same install; opts in by connecting a wallet in Settings → Marketplace |
| Wallet required? | No | Yes (one-time setup, in Settings) |

The hackathon submission targets Persona B but the foundation must work cleanly for Persona A — it's the durable artifact, not the hackathon-specific wrapper. **Marketplace is part of xvn, framed as opt-in.** Persona A always sees a Marketplace section in Settings but never has to engage with it; nothing reaches Mantle until they connect a wallet. The cargo feature gate `marketplace` exists for build-flexibility (size-conscious / audit / minimal builds) but the default `cargo build` includes marketplace; user-visible opt-in is the wallet-connect step, not a recompile.

### 1.2 In scope (v1, by 2026-06-15)

- Mutator (LLM proposes prose/param/tool diffs against parent bundle)
- Numeric merge gate (Δ-Sharpe ≥ pre-committed ε on day + held-out windows)
- LLM judge writes a structured finding for accepted children, blind to numeric metrics
- Lineage store (content-addressed mutation log on disk + SQLite)
- Evening cycle orchestrator (cron via the durable scheduler)
- **CycleSeal artifact** — the contract surface between core and any downstream consumer (marketplace plugin, future v2 consumers, external auditors)
- Five novel evals: counterfactual-chain Merkle receipts, null-result canary, inversion-pair eval, mutator-skill ladder, embedding-divergence diversity-decay
- Dashboard: genealogy tree, live evening cycle viewer, mutation diff inspector, lineage timeline, mutator-skill ladder, ladder-with-provenance
- Replay fixture (`xvn autoresearch demo`) for offline reproduction

### 1.3 Out of scope (v1)

- Any ERC-8004 / Mantle / on-chain integration → handled by the [Marketplace Plugin spec](./2026-05-09-marketplace-plugin-design.md)
- Open attestation surface → covered in marketplace spec; v1 is "in-house attesters seeded by xvision," v2 is public participation
- Slot/template-swap mutations (architectural mutations beyond prose/params/tools)
- Adaptive cycle budget (per-lineage allocation tied to recent gains)
- Cross-run memory beyond the strategy ledger
- Multi-asset autoresearch (BTC-only in v1, matches the rest of the hackathon scope)

---

## 2. Locked decisions (from brainstorm 2026-05-09)

| # | Decision |
|---|---|
| 1 | **Hackathon scope:** full loop in submission. Live evening cycle runs nightly through hackathon window. |
| 2 | **Mutation surface:** three-way — `program.md` prose, parameter knobs, MCP tool selection. Slot/template swaps deferred. |
| 3 | **Merge gate:** numeric primary (Δ-Sharpe ≥ ε on day AND held-out). LLM judge writes structured finding for accepted children, **blind to metrics**. |
| 4 | **Cycle budget:** configurable via `config/autoresearch.toml` (mutations-per-parent, parents-per-evening, token caps). Default tuned during week 4. |
| 5 | **Demo shape:** autoresearch-first. Live evening cycle is the headline. `xvn autoresearch demo` replay is the air-gap fallback. |
| 6 | **No chain coupling.** Core emits `CycleSeal` artifacts; the marketplace plugin reads them. The autoresearch module has zero `use` statements pointing at chain code. |
| 7 | **Pre-commitment:** ε, held-out window, parent-selection seed, and cycle config are sealed and operator-signed locally at session start. Anchoring on-chain is optional and handled by the marketplace plugin. |
| 8 | **Hard go/no-go on 2026-05-23:** if eval engine paper-test path is not working end-to-end, fall back to "Mutator + lineage UI, run once" branch (no live evening cycle). The go/no-go is purely about the loop; chain readiness is a separate concern. |
| 9 | **Module location:** new `xvision-engine/src/autoresearch/` parallel to `eval/`. Reuses eval engine's executor, findings extractor, persistence. |
| 10 | **Cheap mutator + expensive judge:** Haiku for proposals (high volume), Sonnet for findings (only on accepted children). Per-cycle token cap with alarm. |
| 11 | **Marketplace is part of default build, opt-in at wallet-connect.** Cargo feature `marketplace` exists for build-flexibility (minimal / audit builds) but default `cargo build` includes it; user-visible opt-in is the Settings → Marketplace wallet-connect step, not a recompile. Core still compiles and runs without the feature. |

---

## 3. Architecture

### 3.1 Module layout

```
xvision-engine/
└── src/
    ├── eval/                    # existing — paper-test executor, scenario fixtures, findings extractor
    ├── autoresearch/            # THIS SPEC — chain-free
    │   ├── mod.rs
    │   ├── mutator.rs           # parent + ledger → candidate bundle + MutationDiff
    │   ├── lineage.rs           # content-addressed mutation log; genealogy queries; Merkle root computation
    │   ├── gate.rs              # deterministic Δ-Sharpe gate; pre-committed ε
    │   ├── judge.rs             # LLM judge writes finding; metrics-blind
    │   ├── cycle.rs             # evening orchestration: select → mutate → eval → gate → commit → seal
    │   ├── parent_policy.rs     # ε-greedy / round-robin / top-K (pluggable)
    │   ├── canary.rs            # null-result canary: sabotaged-parent injection
    │   ├── inversion.rs         # forward + reverse mutation pair eval
    │   ├── diversity.rs         # embedding-divergence diversity-decay metric
    │   ├── seal.rs              # CycleSeal artifact: content-addressed bundle of every cycle output
    │   └── progress.rs          # SSE event emitters for dashboard
    ├── marketplace/             # COMPANION SPEC — only compiled with `--features marketplace`
    │   └── ...                  # see 2026-05-09-marketplace-plugin-design.md
    ├── strategy/                # existing — bundles
    ├── mcp/                     # existing — tool surface
    └── scheduler/               # existing — durable cron, owns the evening cycle trigger
```

**Dependency rule:** `autoresearch/` has zero `use` statements pointing at `marketplace/`. The arrow only goes the other direction. CI enforces this with a feature-flag-off build that must succeed.

### 3.2 Per-cycle data flow

```
parents = parent_policy.pick(N)   # configurable: round-robin | top-K | ε-greedy
canary_parent = canary.inject()   # one sabotaged parent per evening

for parent in parents + [canary_parent]:
    for _ in range(mutations_per_parent):       # configurable, default 3
        candidate, diff = mutator.propose(parent, parent.recent_ledger)
        if not validator.ok(candidate):
            mutator.retry_with_error(...)        # max 2 retries
            continue
        m_child_day      = eval.paper_test(candidate, day_window)
        m_child_holdout  = eval.paper_test(candidate, holdout_window)
        if gate.passes(parent.metrics, m_child_day, m_child_holdout, eps):
            finding = judge.write_finding(parent, candidate, traces=ONLY_TRACES)
            inverse_candidate, _ = inversion.reverse(parent, diff)
            m_inv = eval.paper_test(inverse_candidate, day_window)
            if not inversion.is_signal(m_child_day, m_inv):
                lineage.commit_as_quarantined(candidate, diff, finding, "noise-suspect")
                continue
            lineage.commit(candidate, diff, finding, parent_hash)
        else:
            lineage.commit_ghost(candidate, diff, "gate-rejected")            # genealogy keeps ghost branches

diversity.update()                   # embedding-divergence rate published
mutator_ladder.update()              # mutator-skill metrics for the second ladder
seal.write(CycleSeal { ... })        # content-addressed bundle of everything above
                                     # if marketplace plugin is enabled, it subscribes here
```

The cycle ends with `seal.write()`. Anything beyond that — minting NFTs, anchoring Merkle roots, posting attestations — happens in downstream consumers. Core has done its job.

### 3.3 Reused vs new code

| Component | Source | Notes |
|---|---|---|
| LLM client | `xvision-intern` | already wired for Anthropic + OpenAI-compat. Mutator uses Haiku; judge uses Sonnet. |
| Paper-test executor | `xvision-engine/src/eval/executor.rs` | reused as-is for both day and held-out windows |
| Scenario fixtures | `xvision-engine/src/eval/scenario.rs` | held-out window pinned at session start |
| Finding extractor | `xvision-engine/src/eval/findings.rs` | judge.rs wraps it with metrics-blind input filtering |
| Persistence | SQLite + JSONL | new tables: `mutations`, `lineage_edges`, `findings`, `canary_runs`, `cycle_seals` |
| Scheduler | `scheduler/` (ported from SwarmClaw) | new job: `autoresearch.evening_cycle` |
| MCP | `xvision-mcp` | tool selection mutations validated against the live tool registry |

### 3.4 The CycleSeal artifact (the contract surface)

```rust
struct CycleSeal {
    cycle_id: Ulid,
    sealed_at: DateTime,
    config_hash: ContentHash,                   // autoresearch.toml + ε + holdout window at session start
    session_commitment: ContentHash,            // operator-signed; pre-committed at session start
    parent_seeds: Vec<ContentHash>,             // bundles that were mutated this evening
    mutations: Vec<ContentHash>,                // every mutation diff blob (incl. rejected/ghost)
    paper_tests: Vec<ContentHash>,              // every paper-test trace
    findings: Vec<ContentHash>,                 // every finding written
    canary_outcome: ContentHash,                // sabotaged-parent acceptance/rejection
    lineage_edges_added: Vec<(ContentHash, ContentHash)>,
    diversity_metric: f64,                      // diversity-decay rate this cycle
    operator_signature: Signature,              // operator's long-lived key signs the seal
    merkle_root: B256,                          // root over everything above; this is what marketplace anchors
}
```

The seal lives on disk + SQLite. Anyone with the artifact bundle can independently:
1. Recompute every leaf hash.
2. Recompute the Merkle root.
3. Verify the operator signature.

That's the provability guarantee. Posting the Merkle root to a chain (the marketplace plugin's job) only adds a public timestamp; the *correctness* of the seal is provable from the bytes alone.

---

## 4. Mutation surface and diff schema

```rust
struct MutationDiff {
    prose_diff: Option<UnifiedTextDiff>,        // unified diff over program.md
    param_changes: Vec<ParamChange>,            // (key, old, new) typed against bundle's param schema
    tool_changes: ToolDiff,                     // added: Vec<ToolName>, removed: Vec<ToolName>
    mutator_model: String,                      // e.g., "claude-haiku-4-5"
    mutator_token_cost: u32,
    proposed_at: DateTime,
    parent_hash: ContentHash,
}

struct ParamChange {
    key: String,                                // e.g., "rsi.period"
    old: serde_json::Value,
    new: serde_json::Value,
}

struct ToolDiff {
    added: Vec<ToolName>,
    removed: Vec<ToolName>,
}
```

**Validation invariants (validator rejects + LLM retry):**
1. All keys in `param_changes` exist in the bundle's typed param schema; values fall in declared ranges.
2. All tools in `tool_changes.added` are registered in the live MCP tool registry.
3. `prose_diff` applies cleanly to parent's `program.md` and produces valid markdown.
4. The candidate bundle, after applying all three diff sections, passes the standard bundle validator (`xvision-engine/src/strategy/bundle.rs`).

Hard cap: 2 retries with the validator's error fed back as system context. Beyond that, the candidate is dropped and the rejection is logged (not committed to lineage).

---

## 5. Merge gate

### 5.1 Numeric gate (`gate.rs`)

A child variant merges if and only if:

```
Δ_day      = sharpe(child, day_window)      − sharpe(parent, day_window)      ≥ ε
Δ_holdout  = sharpe(child, holdout_window)  − sharpe(parent, holdout_window)  ≥ ε
```

Both inequalities required. Single-window improvements are rejected. ε is pre-committed at session start (see §8).

### 5.2 LLM judge (`judge.rs`)

The judge runs **only on children that already passed the numeric gate.** It receives:
- Parent and child trace tapes (per-trade decisions, fills, regime tags)
- Parent and child `program.md`
- Mutation diff
- **Not given:** Sharpe, drawdown, profit factor, or any metric. Numeric-blind.

The judge writes a structured `Finding`:

```rust
struct Finding {
    summary: String,                            // 1–2 sentence shape claim
    regime_affinity: Vec<RegimeTag>,            // e.g., [HighVol, Trending]
    failure_modes: Vec<String>,                 // when this variant might break
    confidence: Confidence,                     // Low | Med | High
    judge_model: String,
    judge_token_cost: u32,
    blinded_metrics: bool,                      // assert true at write time
}
```

The blinded-metrics assertion is enforced in code: `judge.rs` strips metrics before constructing the prompt and panics if any leak through.

### 5.3 Inversion-pair check (`inversion.rs`)

For every numeric-gate-passing candidate, generate the inverse mutation (revert prose, reset params to parent's values, undo tool changes) and paper-test on the day window. If the inverse's Sharpe is statistically indistinguishable from the forward child's Sharpe (within bootstrap CI), the lineage is committed but **flagged `quarantined: noise-suspect`** — visible in the dashboard, excluded from the ladder, and excluded from parent-selection in future cycles. This is the cheapest single check that catches gate-passing-on-noise.

---

## 6. Lineage store and genealogy

### 6.1 Content-addressed mutation log

```rust
struct LineageNode {
    bundle_hash: ContentHash,                   // BLAKE3 over the full bundle
    parent_hash: Option<ContentHash>,           // None for seed strategies
    diff: MutationDiff,
    finding: Option<Finding>,                   // None for ghost branches
    status: LineageStatus,                      // Active | Ghost | Quarantined
    born_at: DateTime,
    metrics_at_birth: MetricsSummary,
    cycle_id: Ulid,                             // which evening cycle produced this
}

enum LineageStatus { Active, Ghost, Quarantined }
```

SQLite tables:
- `lineage_nodes` — primary store, one row per bundle_hash
- `lineage_edges` — (parent_hash, child_hash, edge_type) for fast genealogy traversal
- `mutations` — full diff blobs (JSONL on disk, indexed by bundle_hash)
- `findings` — judge outputs
- `canary_runs` — per-evening canary results (sabotaged-parent acceptance/rejection)
- `cycle_seals` — one row per evening cycle, holds the seal's Merkle root and operator signature

### 6.2 Counterfactual-chain Merkle root

For each lineage, `lineage.rs` computes a Merkle root over the chain:

```
parent_hash → child_hash → days_alive → trades_attributed → realized_pnl_attributed
```

Each step is a leaf; the path's Merkle root is what *can be* posted on-chain (by the marketplace plugin). The root computation lives in core; **whether to anchor it is a marketplace decision**. Persona A still gets the genealogy view and the local Merkle root; they just don't see anchored receipts.

---

## 7. Pre-commitment protocol

At session start (one-time, generated by core):

```rust
struct SessionCommitment {
    epsilon: f64,                               // merge-gate threshold
    holdout_window: TimeRange,                  // pinned, never touched by day trading
    parent_policy_seed: u64,                    // for reproducible parent selection
    cycle_config_hash: ContentHash,             // hash of autoresearch.toml at session start
    canary_seed: u64,                           // for reproducible sabotaged-parent generation
    session_id: Ulid,
    operator_signature: Signature,              // long-lived key signs the commitment
}
```

The commitment is sealed locally and operator-signed. The signature establishes **"this commitment existed at this hash before any cycles ran."** Forgery requires operator key compromise.

If the marketplace plugin is enabled, the session commitment hash is anchored to Mantle at session start (one tx, see marketplace spec). Without the plugin, the commitment is purely local — auditable via the artifact bundle but not publicly timestamped.

If ε is loosened mid-hackathon (e.g., merge rate < 1/evening for 3 consecutive evenings), the loosening follows a **pre-committed schedule** — written into `autoresearch.toml` from day one and hashed in `cycle_config_hash`. Schedule example: ε starts at 0.10, drops to 0.07 if no merges for 3 nights, drops to 0.05 if still none. The schedule itself is in the commitment; the values aren't tuned, only triggered.

---

## 8. Novel eval suite (v1)

Five evals beyond the standard Δ-Sharpe gate. All run in core; all produce on-disk artifacts. The marketplace plugin can optionally surface them on-chain (see marketplace spec) but Persona A gets the full suite locally.

| Eval | Failure mode addressed | Cost | Where it lives |
|---|---|---|---|
| **Counterfactual-chain Merkle root** | "lineage's track record is unauditable" | low (one Merkle build per lineage) | `lineage.rs` |
| **Null-result canary** (sabotaged parent injected nightly) | "the gate fits noise" | low (one extra parent per evening) | `canary.rs` |
| **Inversion-pair eval** (forward + reverse mutation) | "the mutation passed the gate by chance" | medium (one extra paper-test per accepted child) | `inversion.rs` |
| **Mutator-skill ladder** (acceptance rate, calibration, regime bias of the LLM mutator itself) | "we're optimizing strategies but never measuring the optimizer" | low (derived metrics over existing tables) | new dashboard view + `mutator_ladder.rs` |
| **Embedding-divergence / diversity-decay metric** (rate at which sibling embedding distance shrinks) | "lineages mode-collapse into one strategy" | low (one embedding API call per committed bundle) | `diversity.rs` |

### 8.1 Null-result canary (`canary.rs`)

Each evening, one synthetic "broken parent" is injected: random params, contradictory `program.md`, conflicting tool set. Generated reproducibly from `canary_seed`. The autoresearcher doesn't know which parent is the canary. The gate's behavior on the canary is published nightly:
- **Mutations rejected** → gate is healthy.
- **Mutations accepted** → gate is fitting noise; alarm raised; that night's results are flagged.

### 8.2 Mutator-skill ladder

Treats the LLM mutator as a model with measurable skill:
- **Acceptance rate** by parent type (TA / onchain / LLM-driven)
- **Calibration** (the mutator can optionally claim expected Δ-Sharpe; we measure realized vs claimed)
- **Regime bias** (what regimes does the mutator improve disproportionately?)
- **Token efficiency** (Sharpe gain per 1k mutator tokens)

Rendered as a second ladder in the dashboard. Recursive reputation: the thing that's optimizing is itself being optimized over.

### 8.3 Embedding-divergence diversity-decay

For every committed bundle, embed `program.md` (one OpenAI / Voyage embedding call). For each lineage, compute mean pairwise distance between siblings at each cycle. Diversity-decay rate = ratio at t to t-1. Published as a single dashboard number plus a sparkline. Falling rate = mode collapse alarm. Rising = healthy exploration.

---

## 9. Dashboard surfaces (autoresearch-first)

Five core views, ordered for the demo. The marketplace plugin adds a sixth tab (see marketplace spec) but the five below render fully without the plugin enabled.

1. **Live evening cycle viewer** (the headline, beat 1)
   - Real-time SSE stream during the evening run. Per-parent, per-mutation, per-paper-test status.
   - Visual: vertical lineage column on the left, mutation timeline scrolling right, ghost branches faded.
   - Token / cost meter visible at top.
   - Designed to read on a projector at 5 meters.
2. **Genealogy tree** (beat 2)
   - D3 force-directed (or radial when lineages > 20).
   - Node size = trade count; color = lineage; edge stroke encodes mutation type (solid prose / dashed param / dotted tool); ghost branches faded.
   - Click → drawer with `program.md`, params, tools, lineage trail, performance sparkline.
3. **Mutation diff inspector**
   - Three-pane: prose diff (markdown red/green), param diff (table), tool diff (chips).
   - LLM finding below.
4. **Mutator-skill ladder** (the second ladder)
   - Acceptance rate, calibration, regime bias, token efficiency.
   - Side by side with the strategy ladder.
5. **Ladder with provenance**
   - Existing strategy ladder, augmented with lineage depth + parent hash + one-line mutation summary.
   - Click row → genealogy tree zoomed to that node.

**SSE event taxonomy:**
`mutation_proposed` · `mutation_evaluating` · `mutation_committed` · `mutation_rejected` · `lineage_forked` · `canary_outcome` · `diversity_updated` · `cycle_sealed`

Marketplace-plugin events (`nft_minted`, `receipt_posted`, `attestation_received`) are emitted from the plugin and rendered in its own dashboard tab.

**Demo replay fallback:**
`xvn autoresearch demo` replays the most recent successful evening cycle from saved fixtures + cached LLM responses. No API keys required. This is the air-gap path — used only if the live demo network or LLM API fails on stage.

---

## 10. Sequencing (5 weeks)

Today is 2026-05-09. Submission is 2026-06-15.

| Wk | Dates | Deliverable | Hard milestone |
|---|---|---|---|
| 1 | 5/09 → 5/16 | Eval engine paper-test path complete. Scenario fixtures pinned. Held-out window pinned. | Paper-test runs end-to-end in CI. |
| 2 | 5/16 → 5/23 | Mutator + lineage store + numeric gate + CycleSeal artifact. Run end-to-end on 2 lineages locally. | **Go/no-go decision.** If not working, fall back to "Mutator + lineage UI, run once" branch. **Note: this milestone is purely about the loop; chain readiness is irrelevant here.** |
| 3 | 5/23 → 5/30 | Cycle orchestrator + judge + canary + inversion-pair + diversity. | Full evening cycle runs end-to-end locally. |
| 4 | 5/30 → 6/06 | Dashboard: all 5 core views. SSE event flow. Mutator-skill ladder. | Dashboard renders live cycle in real time. |
| 5 | 6/06 → 6/13 | **Marketplace plugin (companion spec)** lands. Live evening cycles every night. Tune cycle budget. Submission polish. Replay fixture sealed. | Submission package complete by 6/13 (2-day buffer). |
| 6 | 6/13 → 6/15 | Buffer. Bug fixes. Final demo rehearsal. | Submit. |

Wks 1–4 require zero chain dependencies. The marketplace plugin is integrated only in Wk 5 (see [marketplace spec](./2026-05-09-marketplace-plugin-design.md) §4 for its own sequencing).

**Wk 2 go/no-go criteria (must all be true to continue full-loop path):**
1. Eval engine paper-test runs deterministically on the pinned scenario fixture, two consecutive runs produce identical metrics.
2. Mutator generates a structurally valid candidate from a real parent in < 30 seconds.
3. Numeric gate compares parent vs child correctly on a known-improvement test case (synthetic).
4. SQLite tables for lineage / mutations / findings / cycle_seals exist and are written by the prototype loop.

If any of those four are missing, fall back to "Mutator + lineage UI, run once" (genealogy view rendered from manually-seeded variants; no live evening cycle; demo shape changes from autoresearch-first to genealogy-first).

---

## 11. Failure modes and mitigations

Distilled from the ideonomy adversarial pass; each row is a way the loop could publicly fail. Marketplace-coupled failure modes (chain costs, NFT mint flakiness, attestation flow) live in the [marketplace spec](./2026-05-09-marketplace-plugin-design.md).

| # | Failure | Mitigation |
|---|---|---|
| 1 | Loop overfits, lineages bloom but ladder doesn't move | Pre-committed ε > bootstrap noise floor; held-out window non-negotiable; canary nightly. |
| 2 | Live evening cycle crashes on stage (rate limits, MCP timeouts, sim bugs) | `xvn autoresearch demo` replay fixture; rate-limit-aware retry with backoff; idempotent paper-test runs. |
| 3 | Mutator hallucinates invalid bundles | JSON-mode + bundle validator; 2-retry cap with error feedback; ungrammatical mutations never enter lineage. |
| 4 | All lineages converge (mode collapse) | Diversity-decay metric public; ε-greedy parent policy with explicit exploration term. |
| 5 | "Karpathy" framing reads as hype | Lead pitch with on-chain population evolution; cite Karpathy in references with explicit list of xvision extensions. |
| 6 | LLM judge reverse-engineers the gate | Judge metrics-blinded in code (panic on leak); numeric gate runs first, deterministically. |
| 7 | Eval engine slips | Wk 2 hard go/no-go; fallback branch ready. |
| 8 | Genealogy unreadable at >50 nodes | Cluster by lineage; top-K filter; on-demand expand; demo storyboard ≤ 10 visible. |
| 9 | Judges can't reproduce | `xvn autoresearch demo` replay; no API keys needed. |
| 10 | Token budget runs out | Haiku mutator + Sonnet judge only on accepted; per-cycle cap with alarm. |
| 11 | Gate too tight, nothing merges | Pre-committed loosening schedule (NOT retroactive); pre-baked "honest gate refused noise" messaging if still nothing. |
| 12 | Single point of demo failure | Three-beat demo: live cycle → genealogy → ladder. Each beat tells a partial story alone. The marketplace beat is *additive* to all three; if it breaks, the core demo still stands. |
| 13 | "It's a Karpathy clone with crypto bolted on" | Submission write-up has a one-page extensions section: counterfactual-chain Merkle, mutator-skill ladder, null-result canary, embedding-divergence, persona-split architecture. None are in his repo. |

---

## 12. Open questions (to resolve in the implementation plan)

1. **Embedding model for diversity-decay** — OpenAI `text-embedding-3-small` vs Voyage. Cost vs quality trade-off; decide in Wk 4.
2. **Default cycle budget values** — `mutations_per_parent`, `parents_per_evening`, per-cycle token cap. Tune empirically in Wk 4 against actual evening run wall-clock.
3. **Genealogy layout algorithm at scale** — force-directed vs radial vs Sugiyama for >20 lineages. Benchmark in Wk 4.
4. **Operator key management** — long-lived signing key for SessionCommitment + CycleSeal signatures. Use existing identity infrastructure or generate fresh? Resolve before session start.

---

## 13. References

- `architecture.md` §11 (deferred Karpathy autoresearch direction — un-deferred by this spec)
- `decisions/0010-hackathon-pivot-strategy-loom.md` (the loom + autoresearch framing; cargo-feature-gate idiom)
- `FOLLOWUPS.md` SLF8, SLF9 (program.md as autoresearch unit-of-work; evening-loop wrapper)
- [Marketplace Plugin spec](./2026-05-09-marketplace-plugin-design.md) (companion — handles all chain/8004/marketplace concerns)
- [Eval Engine spec](./2026-05-08-eval-engine-design.md) (paper-test runner, findings extractor, scenario fixtures)
- [Strategy Creation Engine spec](./2026-05-08-strategy-creation-engine-design.md) (bundle schema, slot templates)
- [Smart Contract Surface spec](./2026-05-08-smart-contract-surface-design.md) (ERC-8004 registries on Mantle; consumed by marketplace plugin)
- ERC-8004 EIP — eips.ethereum.org/EIPS/eip-8004 (mainnet live 2026-01-29)
- Karpathy autoresearch — github.com/karpathy/autoresearch (March 2026)
- Mantle Turing Test Hackathon 2026 (Phase 2, "AI Awakening")
