# Karpathy Autoresearcher — Design

> **Status:** Draft for user review · 2026-05-09
> **Author:** xianvec hackathon team
> **Companion specs:** [Eval Engine](./2026-05-08-eval-engine-design.md) · [Strategy Creation Engine](./2026-05-08-strategy-creation-engine-design.md) · [Smart Contract Surface](./2026-05-08-smart-contract-surface-design.md)
> **Reference:** github.com/karpathy/autoresearch (March 2026)
> **Hackathon deadline:** 2026-06-15 (5 weeks)

---

## 1. Purpose and scope

The Karpathy Autoresearcher is xianvec's evening cycle: an LLM-driven loop that proposes mutations to existing strategy variants, paper-tests them on day-of trades plus a held-out window, and commits surviving children into a content-addressed lineage with on-chain receipts on Mantle.

This spec un-defers the loop from architecture.md §11 ("the deferred Karpathy autoresearch direction") and brings it inside the hackathon submission. It is the **autoresearch-first demo path**: the live evening cycle is the headline beat; the on-chain ladder and genealogy tree are supporting context.

**In scope (v1, by 2026-06-15):**
- Mutator (LLM proposes prose/param/tool diffs against parent bundle)
- Numeric merge gate (Δ-Sharpe ≥ pre-committed ε on day + held-out windows)
- LLM judge writes a structured finding for accepted children, blind to numeric metrics
- Lineage store (content-addressed mutation log)
- Evening cycle orchestrator (cron via the durable scheduler)
- ERC-8004 wiring extensions: per-child NFT mint, batched receipts, finding-hash to validation registry
- Open Attestation Surface (event feed for external agents to score and post their own receipts)
- Five novel evals: counterfactual-chain Merkle receipts, null-result canary, inversion-pair eval, mutator-skill ladder, embedding-divergence diversity-decay metric
- Dashboard: genealogy tree, live evening cycle viewer, mutation diff inspector, lineage timeline, mutator-skill ladder, ladder-with-provenance
- Replay fixture (`xvn autoresearch demo`) for offline reproduction

**Out of scope (deferred):**
- Slot/template-swap mutations (architectural mutations beyond prose/params/tools)
- Adaptive cycle budget (per-lineage allocation tied to recent gains)
- TEE / zkML attestation for findings (signed-oracle only in v1)
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
| 6 | **Open Attestation Surface:** structured trace + numeric-gate result + draft finding exposed as JSON event feed. External ERC-8004 agents subscribe and post their own validation receipts. The lineage's reputation becomes an ensemble of attestations. Replaces in-house "twin judges." |
| 7 | **Pre-commitment:** ε, held-out window, parent-selection seed, and cycle config are hashed and posted on-chain at session start. Stops "tuned post-hoc" objections. |
| 8 | **Hard go/no-go on 2026-05-23:** if eval engine paper-test path is not working end-to-end, fall back to "Mutator + lineage UI, run once" branch (no live evening cycle). |
| 9 | **Module location:** new `xianvec-engine/src/autoresearch/` parallel to `eval/`. Reuses eval engine's executor, findings extractor, persistence. |
| 10 | **Cheap mutator + expensive judge:** Haiku for proposals (high volume), Sonnet for findings (only on accepted children). Per-cycle token cap with alarm. |

---

## 3. Architecture

### 3.1 Module layout

```
xianvec-engine/
└── src/
    ├── eval/                    # existing — paper-test executor, scenario fixtures, findings extractor
    ├── autoresearch/            # THIS SPEC
    │   ├── mod.rs
    │   ├── mutator.rs           # parent + ledger → candidate bundle + MutationDiff
    │   ├── lineage.rs           # content-addressed mutation log; genealogy queries
    │   ├── gate.rs              # deterministic Δ-Sharpe gate; pre-committed ε
    │   ├── judge.rs             # LLM judge writes finding; metrics-blind
    │   ├── cycle.rs             # evening orchestration: select → mutate → eval → gate → commit
    │   ├── parent_policy.rs     # ε-greedy / round-robin / top-K (pluggable)
    │   ├── canary.rs            # null-result canary: sabotaged-parent injection
    │   ├── inversion.rs         # forward + reverse mutation pair eval
    │   ├── diversity.rs         # embedding-divergence diversity-decay metric
    │   ├── attestation.rs       # Open Attestation Surface: event feed + receipt ingestion
    │   └── progress.rs          # SSE event emitters for dashboard
    ├── strategy/                # existing — bundles
    ├── identity/                # existing — ERC-8004 NFT minting; extended for parent_agent_id
    ├── mcp/                     # existing — tool surface
    └── scheduler/               # existing — durable cron, owns the evening cycle trigger
```

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
            identity.mint_child_nft(candidate, parent_agent_id=parent.agent_id)
            attestation.emit(candidate, traces, gate_result, finding_draft)  # external agents listen
        else:
            lineage.commit_ghost(candidate, diff, "gate-rejected")            # genealogy keeps ghost branches

identity.batch_post_receipts()       # ONE Mantle tx per cycle
diversity.update()                   # embedding-divergence rate published
mutator_ladder.update()              # mutator-skill metrics for the second ladder
```

### 3.3 Reused vs new code

| Component | Source | Notes |
|---|---|---|
| LLM client | `xianvec-intern` | already wired for Anthropic + OpenAI-compat. Mutator uses Haiku; judge uses Sonnet. |
| Paper-test executor | `xianvec-engine/src/eval/executor.rs` | reused as-is for both day and held-out windows |
| Scenario fixtures | `xianvec-engine/src/eval/scenario.rs` | held-out window pinned at session start |
| Finding extractor | `xianvec-engine/src/eval/findings.rs` | judge.rs wraps it with metrics-blind input filtering |
| Persistence | SQLite + JSONL | new tables: `mutations`, `lineage_edges`, `findings`, `attestations`, `canary_runs` |
| ERC-8004 | `xianvec-identity` | extended: `mint_child_nft(parent_agent_id)`, `batch_post_receipts()` |
| Scheduler | `scheduler/` (ported from SwarmClaw) | new job: `autoresearch.evening_cycle` |
| MCP | `xianvec-mcp` | tool selection mutations validated against the live tool registry |

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
4. The candidate bundle, after applying all three diff sections, passes the standard bundle validator (`xianvec-engine/src/strategy/bundle.rs`).

Hard cap: 2 retries with the validator's error fed back as system context. Beyond that, the candidate is dropped and the rejection is logged (not committed to lineage).

---

## 5. Merge gate

### 5.1 Numeric gate (`gate.rs`)

A child variant merges if and only if:

```
Δ_day      = sharpe(child, day_window)      − sharpe(parent, day_window)      ≥ ε
Δ_holdout  = sharpe(child, holdout_window)  − sharpe(parent, holdout_window)  ≥ ε
```

Both inequalities required. Single-window improvements are rejected. ε is pre-committed at hackathon kickoff and posted on-chain (see §8).

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
    nft_id: Option<U256>,                       // ERC-8004 Identity Registry
    cycle_id: Ulid,                             // which evening cycle produced this
}

enum LineageStatus { Active, Ghost, Quarantined }
```

SQLite tables:
- `lineage_nodes` — primary store, one row per bundle_hash.
- `lineage_edges` — (parent_hash, child_hash, edge_type) for fast genealogy traversal.
- `mutations` — full diff blobs (JSONL on disk, indexed by bundle_hash).
- `findings` — judge outputs.
- `canary_runs` — per-evening canary results (sabotaged-parent acceptance/rejection).

### 6.2 Counterfactual-chain Merkle receipt (novel eval #1)

ERC-8004 reputation receipt for a lineage is **not a scalar.** It is a Merkle path:

```
parent_hash → child_hash → days_alive → trades_attributed → realized_pnl_attributed
```

Each step is a leaf; the path's Merkle root is what's posted to the Reputation Registry. Verifiers can audit any segment of the lineage independently. This is the strongest single differentiator from a Karpathy-clone framing — the on-chain artifact carries the audit trail itself, not just an endorsement summary.

Implementation: `xianvec-identity` extension `post_lineage_merkle_receipt(lineage_id)`.

---

## 7. Open Attestation Surface

### 7.1 Event feed

After every committed child (Active or Quarantined), `attestation.emit()` writes a structured event:

```json
{
  "event": "lineage.candidate_committed",
  "cycle_id": "01H...",
  "bundle_hash": "blake3:...",
  "parent_hash": "blake3:...",
  "diff": { "prose_diff": "...", "param_changes": [...], "tool_changes": {...} },
  "trace_uri": "ipfs://...",
  "gate_result": { "delta_day": 0.18, "delta_holdout": 0.07, "epsilon": 0.05, "passed": true },
  "finding_draft": { "summary": "...", "regime_affinity": [...], "confidence": "Med" },
  "open_for_attestation_until": "2026-06-09T08:00:00Z"
}
```

Distribution:
- **In-process:** SSE stream to the dashboard.
- **Off-process:** posted to a public endpoint (`/autoresearch/feed`), pollable by external agents.
- **On-chain pointer:** the Reputation Registry receipt for the cycle includes the feed batch's content hash, so external agents can prove what they were responding to.

### 7.2 External attestation ingestion

Any address with an ERC-8004 Identity NFT can post a `ValidationReceipt` against a `bundle_hash` within the attestation window:

```rust
struct ExternalAttestation {
    attester_agent_id: U256,                    // their ERC-8004 identity
    bundle_hash: ContentHash,
    verdict: AttestationVerdict,                // Endorse | Question | Reject
    rationale_hash: ContentHash,                // pointer to off-chain rationale
    posted_at: DateTime,
    on_chain_tx: TxHash,
}
```

The dashboard renders external attestations beside the in-house finding. A lineage with multiple independent endorsements gets a `multi-attested` badge. Disagreements are not hidden; they are themselves a finding.

This replaces hard-coded twin judges with an open marketplace of attestations. It is *deeply* on-brand for the marketplace narrative and uses ERC-8004 exactly as designed.

### 7.3 Hackathon demo seeding

For the demo, the xianvec team operates 2–3 internal agents (each with its own ERC-8004 identity) that consume the feed and post their own attestations. The system is open by design; the demo seeds it with internal participation. External participation is the v2 narrative.

---

## 8. Pre-commitment protocol

At session start (one-time, posted on-chain):

```rust
struct AutoresearchSessionCommitment {
    epsilon: f64,                               // merge-gate threshold
    holdout_window: TimeRange,                  // pinned, never touched by day trading
    parent_policy_seed: u64,                    // for reproducible parent selection
    cycle_config_hash: ContentHash,             // hash of autoresearch.toml at session start
    canary_seed: u64,                           // for reproducible sabotaged-parent generation
    hackathon_session_id: Ulid,
}
```

Posted to the Reputation Registry with operator signature. Any post-hoc tuning is provable on chain. This is the "we didn't move the goalposts" artifact.

If ε is loosened mid-hackathon (e.g., merge rate < 1/evening for 3 consecutive evenings), the loosening follows a **pre-committed schedule** — written into `autoresearch.toml` from day one and hashed in `cycle_config_hash`. Schedule example: ε starts at 0.10, drops to 0.07 if no merges for 3 nights, drops to 0.05 if still none. The schedule itself is on-chain; the values aren't tuned, only triggered.

---

## 9. Novel eval suite (v1)

Five evals beyond the standard Δ-Sharpe gate. Each one earns its slot by addressing a specific failure mode of LLM-driven autoresearch:

| Eval | Failure mode addressed | Cost | Where it lives |
|---|---|---|---|
| **Counterfactual-chain Merkle receipt** | "lineage's track record is a single number, unauditable" | low (one Merkle build per lineage) | `lineage.rs` + `identity` extension |
| **Null-result canary** (sabotaged parent injected nightly) | "the gate fits noise" | low (one extra parent per evening) | `canary.rs` |
| **Inversion-pair eval** (forward + reverse mutation) | "the mutation passed the gate by chance" | medium (one extra paper-test per accepted child) | `inversion.rs` |
| **Mutator-skill ladder** (acceptance rate, calibration, regime bias of the LLM mutator itself) | "we're optimizing strategies but never measuring the optimizer" | low (derived metrics over existing tables) | new dashboard view + `mutator_ladder.rs` |
| **Embedding-divergence / diversity-decay metric** (rate at which sibling embedding distance shrinks) | "lineages mode-collapse into one strategy" | low (one embedding API call per committed bundle) | `diversity.rs` |

### 9.1 Null-result canary (`canary.rs`)

Each evening, one synthetic "broken parent" is injected: random params, contradictory `program.md`, conflicting tool set. Generated reproducibly from `canary_seed`. The autoresearcher doesn't know which parent is the canary. The gate's behavior on the canary is published nightly:
- **Mutations rejected** → gate is healthy.
- **Mutations accepted** → gate is fitting noise; alarm raised; that night's results are flagged.

The canary's nightly outcome is itself an on-chain artifact (a `CanaryReceipt` in the Reputation Registry). This is a powerful judging-day talking point: "we publish on-chain proof that our gate rejects garbage."

### 9.2 Mutator-skill ladder

Treats the LLM mutator as a model with measurable skill:
- **Acceptance rate** by parent type (TA / onchain / LLM-driven).
- **Calibration** (the mutator can optionally claim expected Δ-Sharpe; we measure realized vs claimed).
- **Regime bias** (what regimes does the mutator improve disproportionately?).
- **Token efficiency** (Sharpe gain per 1k mutator tokens).

Rendered as a second ladder in the dashboard. Recursive reputation: the thing that's optimizing is itself being optimized over.

### 9.3 Embedding-divergence diversity-decay

For every committed bundle, embed `program.md` (one OpenAI / Voyage embedding call). For each lineage, compute mean pairwise distance between siblings at each cycle. Diversity-decay rate = ratio at t to t-1. Published as a single dashboard number plus a sparkline. Falling rate = mode collapse alarm. Rising = healthy exploration.

---

## 10. Dashboard surfaces (autoresearch-first)

Five views, ordered for the demo:

1. **Live evening cycle viewer** (the headline, beat 1)
   - Real-time SSE stream during the evening run. Per-parent, per-mutation, per-paper-test status.
   - Visual: vertical lineage column on the left, mutation timeline scrolling right, ghost branches faded.
   - Token / cost meter visible at top.
   - Designed to read on a projector at 5 meters.
2. **Genealogy tree** (beat 2)
   - D3 force-directed (or radial when lineages > 20).
   - Node size = trade count; color = lineage; edge stroke encodes mutation type (solid prose / dashed param / dotted tool); ghost branches faded.
   - Click → drawer with `program.md`, params, tools, lineage trail, NFT link, attestations.
3. **Mutation diff inspector**
   - Three-pane: prose diff (markdown red/green), param diff (table), tool diff (chips).
   - LLM finding below; external attestations beside.
4. **Mutator-skill ladder** (the second ladder)
   - Acceptance rate, calibration, regime bias, token efficiency.
   - Side by side with the strategy ladder.
5. **Ladder with provenance**
   - Existing strategy ladder, augmented with lineage depth + parent hash + one-line mutation summary.
   - Click row → genealogy tree zoomed to that node.

**SSE event taxonomy:**
`mutation_proposed` · `mutation_evaluating` · `mutation_committed` · `mutation_rejected` · `lineage_forked` · `nft_minted` · `receipt_posted` · `attestation_received` · `canary_outcome` · `diversity_updated`

**Demo replay fallback:**
`xvn autoresearch demo` replays the most recent successful evening cycle from saved fixtures + cached LLM responses. No API keys required. This is the air-gap path — used only if the live demo network or LLM API fails on stage.

---

## 11. ERC-8004 wiring extensions

`xianvec-identity` gains:
- `mint_child_nft(bundle: &Bundle, parent_agent_id: U256)` — mints with the parent linkage encoded in the manifest CID.
- `batch_post_receipts(cycle_id: Ulid, receipts: Vec<Receipt>)` — single Mantle tx per evening cycle. Uses Mantle's batch endpoint.
- `post_lineage_merkle_receipt(lineage_id, root: B256)` — counterfactual-chain Merkle root.
- `post_canary_receipt(cycle_id, outcome: CanaryOutcome)` — nightly canary result.
- `accept_external_attestation(att: ExternalAttestation)` — verifies attester's ERC-8004 identity and indexes the validation-registry receipt.

Gas estimate (Mantle Sepolia, conservative):
- Per evening cycle: 1 batch tx covering N receipts (N typically 5–20).
- Per session: 1 commitment tx + 1 canary receipt × evenings + 1 batch × evenings.
- Pre-fund operator wallet with 5× estimate before kickoff.

---

## 12. Sequencing (5 weeks)

Today is 2026-05-09. Submission is 2026-06-15. Six weekly milestones with a hard go/no-go on the second.

| Wk | Dates | Deliverable | Hard milestone |
|---|---|---|---|
| 1 | 5/09 → 5/16 | Eval engine paper-test path complete. Scenario fixtures pinned. Held-out window pinned. | Paper-test runs end-to-end in CI. |
| 2 | 5/16 → 5/23 | Mutator + lineage store + numeric gate. Run end-to-end on 2 lineages locally. | **Go/no-go decision.** If not working, fall back to "Mutator + lineage UI, run once" branch. |
| 3 | 5/23 → 5/30 | Cycle orchestrator + judge + canary + inversion-pair + diversity. ERC-8004 wiring extensions. Sepolia smoke. | Full evening cycle runs end-to-end on testnet. |
| 4 | 5/30 → 6/06 | Dashboard: all 5 views. SSE event flow. Mutator-skill ladder. Open Attestation Surface (in-process feed; external endpoint). | Dashboard renders live cycle in real time. |
| 5 | 6/06 → 6/13 | Live evening cycles every night. Tune cycle budget. Pre-commitment posted to Mantle mainnet. Replay fixture sealed. Submission polish. | Submission package complete by 6/13 (2-day buffer). |
| 6 | 6/13 → 6/15 | Buffer. Bug fixes. Final demo rehearsal. | Submit. |

**Wk 2 go/no-go criteria (must all be true to continue full-loop path):**
1. Eval engine paper-test runs deterministically on the pinned scenario fixture, two consecutive runs produce identical metrics.
2. Mutator generates a structurally valid candidate from a real parent in < 30 seconds.
3. Numeric gate compares parent vs child correctly on a known-improvement test case (synthetic).
4. SQLite tables for lineage / mutations / findings exist and are written by the prototype loop.

If any of those four are missing, fall back to "Mutator + lineage UI, run once" (genealogy view rendered from manually-seeded variants; no live evening cycle; demo shape changes from autoresearch-first to genealogy-first).

---

## 13. Failure modes and mitigations

Distilled from the ideonomy adversarial pass; each row is a way the loop could publicly fail.

| # | Failure | Mitigation |
|---|---|---|
| 1 | Loop overfits, lineages bloom but ladder doesn't move | Pre-committed ε > bootstrap noise floor; held-out window non-negotiable; canary nightly. |
| 2 | Live evening cycle crashes on stage (rate limits, MCP timeouts, sim bugs) | `xvn autoresearch demo` replay fixture; rate-limit-aware retry with backoff; idempotent paper-test runs. |
| 3 | Mutator hallucinates invalid bundles | JSON-mode + bundle validator; 2-retry cap with error feedback; ungrammatical mutations never enter lineage. |
| 4 | All lineages converge (mode collapse) | Diversity-decay metric public; ε-greedy parent policy with explicit exploration term. |
| 5 | "Karpathy" framing reads as hype | Lead pitch with on-chain population evolution; cite Karpathy in references with explicit list of xianvec extensions. |
| 6 | On-chain costs balloon | Batch all receipts into 1 tx per cycle; pre-fund 5× estimate; Sepolia smoke before mainnet. |
| 7 | LLM judge reverse-engineers the gate | Judge metrics-blinded in code (panic on leak); numeric gate runs first, deterministically. |
| 8 | Eval engine slips | Wk 2 hard go/no-go; fallback branch ready. |
| 9 | Genealogy unreadable at >50 nodes | Cluster by lineage; top-K filter; on-demand expand; demo storyboard ≤ 10 visible. |
| 10 | Judges can't reproduce | `xvn autoresearch demo` replay; no API keys needed. |
| 11 | Token budget runs out | Haiku mutator + Sonnet judge only on accepted; per-cycle cap with alarm. |
| 12 | Gate too tight, nothing merges | Pre-committed loosening schedule (NOT retroactive); pre-baked "honest gate refused noise" messaging if still nothing. |
| 13 | Single point of demo failure | Three-beat demo: live cycle → genealogy → ladder. Each beat tells a partial story alone. |
| 14 | "It's a Karpathy clone with crypto bolted on" | Submission write-up has a one-page extensions section: counterfactual-chain receipts, Open Attestation Surface, mutator-skill ladder, null-result canary, embedding-divergence. None are in his repo. |

---

## 14. Open questions (to resolve in the implementation plan)

1. **Embedding model for diversity-decay** — OpenAI `text-embedding-3-small` vs Voyage. Cost vs quality trade-off; decide in Wk 4.
2. **Default cycle budget values** — `mutations_per_parent`, `parents_per_evening`, per-cycle token cap. Tune empirically in Wk 4 against actual evening run wall-clock.
3. **Genealogy layout algorithm at scale** — force-directed vs radial vs Sugiyama for >20 lineages. Benchmark in Wk 4.
4. **External attestation window length** — hours-to-days. Affects how quickly external agents can participate. Default 24h, configurable.
5. **Sepolia vs mainnet for hackathon submission** — current Identity work is Sepolia. Plan for mainnet cutover in Wk 5; budget gas accordingly.

---

## 15. References

- `architecture.md` §11 (deferred Karpathy autoresearch direction — un-deferred by this spec)
- `decisions/0010-hackathon-pivot-strategy-loom.md` (the loom + autoresearch framing)
- `FOLLOWUPS.md` SLF8, SLF9 (program.md as autoresearch unit-of-work; evening-loop wrapper)
- `docs/superpowers/specs/2026-05-08-eval-engine-design.md` (paper-test runner, findings extractor, scenario fixtures)
- `docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md` (bundle schema, slot templates)
- `docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md` (ERC-8004 registries on Mantle)
- ERC-8004 EIP — eips.ethereum.org/EIPS/eip-8004 (mainnet live 2026-01-29)
- Karpathy autoresearch — github.com/karpathy/autoresearch (March 2026)
- Mantle Turing Test Hackathon 2026 (Phase 2, "AI Awakening")
