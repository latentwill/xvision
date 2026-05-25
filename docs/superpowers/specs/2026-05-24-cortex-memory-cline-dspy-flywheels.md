# Cortex memory + ClineSDK + DSRs - self-improvement flywheel implementation plan

Date: 2026-05-24
Status: implementation plan and acceptance contract

## Purpose

This document specifies how the existing xvision Cortex memory layer
(`xvision-memory` crate, V2D shipped) composes with the ClineSDK runtime
(Stages 0-4, in flight) and the DSRs / `dspy-rs` optimizer foundation
(specified in the companion chat-rail-DSPy plan) to produce closed-loop
self-improvement flywheels for trading agents.

It is a sibling to `2026-05-24-chat-rail-and-strategy-agents-evaluation.md`
and must not contradict it. Where that document specifies operator surfaces
and DSRs adoption, this one specifies the memory substrate underneath, the
autoresearcher distillation pass on top, and the leakage-safe closure between
them.

It is also a follow-on to `2026-05-21-cortex-memory-integration-plan.md`
and `2026-05-09-karpathy-autoresearcher-design.md`. Those documents define
the data model and the distillation loop respectively. This document defines
how those two surfaces connect to the new ClineSDK runtime and the DSRs
optimizer, and adds the proof gates the earlier specs lack.

## Attribution and credit

This plan adopts [gambletan/cortex](https://github.com/gambletan/cortex)
(MIT-licensed, pure Rust) as the upstream memory engine. Credit to the
gambletan maintainer is non-negotiable across the surfaces that touch
memory:

- **Repository root**: a `CREDITS.md` lists gambletan/cortex with the
  upstream URL, version pin, license, and the role the project plays
  (persistent memory engine). Created in the same PR as this plan.
- **Cargo.toml**: when `cortex-core` is added as a git dependency, the
  workspace `Cargo.toml` includes a `# Persistent memory engine —
  https://github.com/gambletan/cortex (MIT)` comment above the entry,
  and the `crates/xvision-memory/Cargo.toml` adapter declares it as a
  required dependency.
- **LICENSE inclusion**: `LICENSES/gambletan-cortex.txt` copies the
  upstream `LICENSE` file verbatim. Release artifacts (Docker image,
  binary archives) include this file in the standard
  `usr/share/doc/xvision/` path or equivalent.
- **README**: the xvision README's Architecture section names
  gambletan/cortex as the memory subsystem and links to the upstream
  repo.
- **Memory-touching docs**: every spec or plan that discusses memory
  (this doc, the V2D plan, the leakage notes, the autoresearcher
  design) carries an inline attribution at first mention.
- **Release notes**: the release that lands cortex-core adoption
  names the dependency, the version, and the upstream maintainer.
- **No false-authorship claims**: xvision-memory's `Cargo.toml`
  description states "trading-safety adapter over gambletan/cortex,"
  not "memory engine."
- **Star and link back**: signal to the upstream maintainer that
  adoption is happening; offer to upstream improvements where
  reasonable.

This list is the floor, not the ceiling. If gambletan ever asks for
specific attribution language, we honor it.

## Terminology lock

Diverging from these names should require a written rationale.

| Concept | Use this name | Notes |
|---|---|---|
| The reference design xvision-memory ports | gambletan/cortex (https://github.com/gambletan/cortex) | 4-tier memory engine (Working/Episodic/Semantic/Procedural), Bayesian beliefs, people graph, MCP server. MIT, pure Rust. Not currently a crate dependency. |
| The xvision in-house memory crate | `xvision-memory` | Clean-room Rust implementation (~509 SLOC across `types.rs`/`store.rs`/`embedder.rs`/`lib.rs`) that mirrors gambletan/cortex's Episodic/Semantic model, collapsed to 2 tiers, with F+L+T safety added. Zero `cortex_*` deps in `Cargo.toml`. |
| Per-cycle decision record | `Observation` | `tier='observation'` in `memory_items`. Maps to cortex's Episodic. Write-only from auto-recorder. Carries `(run_id, scenario_id, cycle_idx)` plus the contributing market-data window (`source_window_start`, `source_window_end`) before Phase 3 begins. |
| Distilled semantic insight | `Pattern` | `tier='pattern'` in `memory_items`. Maps to cortex's Semantic. Recall-only at decision time. Carries `training_window_end` (nullable only for operator-attested manual seeds). Staging is represented by `promotion_state`, not by inventing a new tier. |
| Deferred sidecar integration | `cortex-http` sidecar | gambletan/cortex's HTTP crate (axum, local-only). 2026-05-21 V2D Decision 1 explicitly defers this to v2. Until then, xvision-memory is self-contained. |
| The recall-time prompt wrapper | "case-law framing" | "A prior decision noted: ... Consider whether this situation matches the present cycle." |
| The three-layer leakage protection | "F+L+T" | F = structural (tier split), L = rhetorical (case-law framing), T = temporal (training_window_end gate). |
| The distillation pass that promotes Observations -> Patterns | "autoresearcher" | Per `2026-05-09-karpathy-autoresearcher-design.md`. |
| One full closed loop | "flywheel cycle" | capture -> observe -> score -> distill -> optimize -> mint -> promote -> recall -> demote. |

## Existing work to preserve

Do not rediscover these. Fold them into implementation.

- `crates/xvision-memory/src/types.rs` - `Tier { Observation, Pattern }`,
  `MemoryMode { Off, Global, AgentScoped }`, `Namespace::for_mode`. These
  shapes are shipped and load-bearing.
- `crates/xvision-memory/src/store.rs` - `upsert_observation`,
  `upsert_pattern`, `query`, `forget`. Tier-shape invariants enforced at
  write time. Vector math (cosine over f32 blob) in pure Rust.
- `crates/xvision-engine/src/agent/memory_recorder.rs` - `recall()` and
  `record()` integration into `execute_slot`. `RecallResult` enum carries
  `decision_id` for trace provenance.
- `crates/xvision-engine/migrations/029_agent_slot_memory_mode.sql` -
  `agent_slots.memory_mode TEXT NOT NULL DEFAULT 'off'`. Pre-029 rows read
  as `Off`.
- `docs/superpowers/notes/2026-05-21-v2d-memory-cortex-tiers-and-leakage.md` -
  the F+L+T leakage contract. This is hard physics, not negotiable.
- `docs/superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md` -
  the mutation loop, Finding/gate model, demote-on-failure semantics.
- `docs/superpowers/research/2026-05-11-autoimproving-memory-survey.md` -
  the research foundation for outcome-conditioned memory.
- `docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md`
  and stages 0-4 plans - the runtime that emits trajectory frames the
  recorder consumes.
- `docs/superpowers/specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md` -
  the operator surface (chat rail) and DSRs offline optimizer. Memory
  surfaces hang off the same chat-rail rows and the same `xvn` verb tree.

## Locked technical decisions

1. **Direct crate-dependency path (Path A) is the chosen integration.**
   gambletan/cortex's `cortex-core` crate is added as a git dependency
   pinned to a specific tag or commit proven by the Phase 1.4 dependency
   spike. Do not assume the plan-date candidate is still current; as of
   2026-05-25 upstream already has tags newer than v1.3.0. The exact
   pin is selected only after compiling against the API surface and
   documenting the hook/schema contract. The existing clean-room
   `xvision-memory` (~509 SLOC) is rewritten as a thin trading-safety
   adapter (~150 SLOC target) that wraps `cortex_core::Cortex` and
   enforces F+L+T through cortex plugin hooks if Phase 1.4 proves those
   hooks are sufficient. If hooks cannot enforce F+L+T, the adapter must
   enforce the boundary itself or the plan falls back to the sidecar /
   wrapper path; direct engine access to cortex remains forbidden either
   way.
   Reasons: the clean-room port discarded ~15 capabilities cortex
   ships (Bayesian beliefs, multi-signal retrieval, HNSW, continuous
   consolidation, MCP surface, token-budgeted context, contradiction
   detection, conversation compression, plugin hooks, event bus,
   negation detection, multi-hop retrieval, people graph, etc.) that
   directly serve the flywheel's needs. Path B (cortex-http sidecar)
   is reserved for a future operational separation if HTTP isolation
   becomes worth the latency. Path C (continued in-tree porting) is
   abandoned. We do NOT vendor or depend on `sopaco/cortex-mem` or
   `prem-research/cortex`; those share the name only.
2. Tier semantics are fixed: `Observation` is write-only by the auto-
   recorder, `Pattern` is recall-only at decision time. No code path
   reads Observations into prompts. No code path writes Patterns from
   the recorder. The store enforces this at the function boundary.
3. F+L+T is non-negotiable. Adding a new recall path requires a written
   amendment to `2026-05-21-v2d-memory-cortex-tiers-and-leakage.md`.
4. Patterns produced by autoresearcher must carry `training_window_end`
   equal to the latest `source_window_end` across contributing
   Observations. Operator-seeded Patterns may set
   `training_window_end = NULL` only when a first-class
   `operator_attestations` row exists and the Pattern stores its
   `attestation_id`. A CLI flag or dashboard checkbox alone is not
   sufficient evidence.
5. The ClineSDK runtime never writes to `memory_items` directly. The Rust
   `MemoryRecorder` is the only writer. The sidecar's role is to emit
   trajectory frames; recording is Rust-side post-dispatch.
6. The DSRs optimizer reads Observations and Patterns as inputs but never
   writes to them. Optimizer output is written to `AgentSlot.system_prompt`
   (or a child agent) and to `agent_slot_optimizations`. Memory writes
   from optimization happen only via autoresearcher Pattern promotion,
   never directly from the optimizer.
7. Demote-on-failure is soft-delete (`forgotten_at` set), with a grace
   window (default 14 days) before janitor hard-deletes. Hard-deletes
   require explicit operator confirmation in the CLI.
8. Backtest scenarios always pass `current_scenario_start` to `recall()`.
   Live and paper modes pass `None` (the operator has accepted the risk).
   No code path may construct a `recall()` call against a non-`None`
   scenario start without an explicit unit test asserting temporal
   filtering applied.
9. Memory-mode upgrades on an existing agent require a new child agent
   mint, not an in-place edit, when the change crosses `Off -> on` or
   `AgentScoped -> Global`. The lineage chain records the transition.
10. Autoresearcher writes never run inside a live decision cycle. Like
    DSRs, autoresearcher is offline-only. The `xvn autoresearch run`
    verb is the only entry point.
11. Raw cortex APIs are private to `xvision-memory`. No engine,
    dashboard, CLI, MCP, optimizer, or autoresearcher code imports
    `cortex_core::*` directly. CI fails on direct imports outside the
    adapter crate.
12. Memory-mode upgrades are enforced server-side, not only in UI copy.
    Any edit surface that would cross `Off -> on` or
    `AgentScoped -> Global` must mint a child agent or return a typed
    validation error pointing to the mint flow.

## Non-negotiable gates

Every implementation task closes with evidence. A task is not done because
it compiles or because the agent says it is done.

Per task:

- Code diff scoped to the task.
- Unit or integration test proving the behavior.
- CLI transcript when the behavior is exposed by `xvn`.
- API transcript when dashboard or MCP surfaces depend on it.
- Browser screenshot or Playwright trace for user-visible dashboard changes.
- Before/after JSON for persisted schemas or exported artifacts.
- Migration proof for any schema change: fresh DB, migrated existing DB,
  and rollback or compatibility statement.
- Leakage probe: for any code touching `recall()`, `query()`, or the
  Observation/Pattern boundary, a regression test asserting the F+L+T
  invariants on a scenario whose `training_window_end` overlaps the
  query window.

Memory-specific additions:

- Determinism probe for `recall()`: same query embedding, same namespace,
  same scenario start -> bit-identical top-k order. Required on every PR
  that touches store.rs or memory_recorder.rs.
- Provenance probe for `record()`: every Observation row written during a
  test must have non-NULL `(run_id, scenario_id, cycle_idx,
  source_window_end)`. Asserted by a fixture check, not a comment.
- Forgotten probe: a forgotten row must not appear in `query()` results
  during the grace window. A separate test must confirm it DOES appear in
  `list_forgotten()` admin view.
- Prompt identity probe: every recall-injected decision stores the final
  prompt hash and, under FullDebug retention, the exact prompt blob after
  memory injection. The dashboard/trace dock must render that same blob or
  hash, not reconstruct a close-enough prompt client-side.

## Anti-shortcut rules

Agents implementing this plan must not:

- Write Observations into the recall path "just for completeness." The
  tier split is a safety boundary, not an optimization.
- Skip the temporal filter in tests because "the scenario doesn't matter
  here." Backtests without scenario_start are leakage by construction.
- Compute `training_window_end` from any source other than the latest bar
  timestamp across contributing Observations. Not the run timestamp, not
  the wall clock, not the operator's mood.
- Promote a Pattern from a single Observation. Distillation requires a
  multi-Observation cohort and a numeric outcome gate; one-hot promotion
  is by definition unable to discriminate signal from noise.
- Use the LLM judge's Finding as the sole gate. The numeric metric gate
  must independently pass before promotion. The Finding is qualitative
  context, not the verdict.
- Optimize a slot using Observations from the same agent ID as the
  optimization target without a holdout split. The default split is
  70/15/15 train/dev/holdout; deviations require justification in the
  evidence ledger.
- Recall Patterns into a prompt while the dashboard is showing the
  operator a different prompt. The prompt the agent sees and the prompt
  the operator sees must be identical at the byte level.
- Run the autoresearcher inside a live trading process. The verb is
  offline; the writer is offline; the audit trail is signed offline.
- Treat `MemoryMode::Off` as "memory not configured." It is "operator
  opted out." Surfacing this distinction matters for evidence.
- Hide LLM extraction costs. Every Pattern promotion records the LLM
  cost (input tokens, output tokens, model, latency) in the lineage row.
- Add a new memory tier for staging or beliefs before updating the F+L+T
  contract. Staged Patterns use `promotion_state`; beliefs/procedural
  memories in Phase 6 require explicit adapter mappings and query gates.
- Expose raw cortex MCP write tools directly to the chat rail. MCP writes
  must go through xvision-safe wrappers that preserve F+L+T, operator
  policy, and audit logs.

## Evidence ledger

Create and maintain:

```text
docs/superpowers/evidence/2026-05-24-cortex-flywheels/
  README.md
  recall/
    deterministic-topk.json
    temporal-filter-probe.txt
    case-law-rendered.md
  record/
    provenance-fixture.json
    observation-rows.json
  autoresearcher/
    distill-run-baseline.json
    distill-run-candidate.json
    finding-blind-to-metric.txt
    pattern-promotion-row.json
    pattern-demotion-row.json
  flywheel/
    cycle-end-to-end-trace.json
    holdout-comparison.md
    leakage-regression-suite.txt
  surface/
    cli-transcripts.txt
    dashboard-screenshots/
    mcp-traces.jsonl
  final/
    surface-matrix.md
    risk-decisions.md
    flywheel-velocity-report.md
```

The final PR links the ledger and summarizes which evidence is automated,
which is manual, and which live-provider proof is intentionally opt-in.

## Surface matrix

Each user-visible capability appears in every applicable surface. If a
surface is not applicable, record why in the evidence ledger.

| Surface | Memory layer | Autoresearcher | DSRs x Memory | Flywheel observability |
|---|---|---|---|---|
| Dashboard UI | Extend existing `/agents/memory`, per-agent Memory tab, and eval-run MemoryPanel with recall preview, exact prompt link, forget UI, lineage view | Autoresearch run history, Pattern provenance, mutation diff, judge Finding view | Optimizer demo-source selector (live vs frozen Observations), holdout split control | Flywheel velocity chart, Pattern lifecycle ribbon, leakage-regression dashboard |
| CLI | Existing verbs remain canonical: `xvn memory ls`, `show`, `add-pattern`, `rm`, `forget`, `undo-forget`. Add aliases only with compatibility tests. No `observe` write verb unless it is explicitly marked fixture-only and cannot hit production DBs. | `xvn autoresearch run`, `xvn autoresearch inspect`, `xvn autoresearch promote`, `xvn autoresearch demote` | `xvn optimize --use-memory-demos`, `--demo-source`, `--holdout-split` | `xvn flywheel status`, `xvn flywheel velocity` |
| Dashboard API | Extend existing `/api/memory` list/get/create/delete/forget/undo-forget routes; add recall-preview and namespace listing through adapter methods, not raw table SQL | autoresearch CRUD, finding endpoints, promotion/demotion endpoints | optimizer demo-source endpoints | flywheel telemetry endpoints |
| MCP | `memory.recall`, `memory.list`, `memory.context` read-only by default; `memory.forget` and all cortex write tools are xvision-wrapped writes requiring server-side policy + audit | `autoresearch.run`, `autoresearch.inspect`, `autoresearch.findings` | none (offline only, no MCP) | `flywheel.status`, `flywheel.velocity` |
| Scripts | leakage-regression script, recall determinism script | distillation smoke script, pattern-lineage export | memory-demo audit script | flywheel velocity export |
| Docs | operator memory overview, MANUAL.md memory section | autoresearcher operator guide, judge prompt provenance docs | optimizer x memory docs | flywheel operator guide |
| Skills | `.claude/skills/xvision/memory-ops` | `.claude/skills/xvision/autoresearch-ops` | extended optimizer skill | `.claude/skills/xvision/flywheel-ops` |
| Tests | recall/record unit tests, leakage probes, migration tests | autoresearcher integration tests, finding-blind tests, promotion gate tests | optimizer-with-memory integration tests, holdout tests | end-to-end flywheel test |
| Observability | `memory_recall`, `memory_write`, `memory_forget` events | `autoresearch_started`, `autoresearch_finished`, `pattern_promoted`, `pattern_demoted` events | optimization events tagged with demo source | flywheel cycle ID threaded through every event |

## The flywheel - canonical loop

Numbered steps. Each step is a defined boundary with an event and an
evidence record.

1. **Capture (ClineSDK runtime, Stage 2 trajectory store).** Each decision
   cycle through the live or eval path produces a trajectory frame. The
   frame includes the briefing input, the model call(s), the tool calls,
   the decision output, and the realized fill. The trajectory store is
   the system of record for the cycle.
2. **Observe (MemoryRecorder.record, post-dispatch).** After the slot
   returns `EndTurn`, the recorder embeds the decision text and writes
   one `tier='observation'` row with full provenance `(run_id,
   scenario_id, cycle_idx, source_window_start, source_window_end)`.
   The latest bar timestamp in the briefing is stored as
   `source_window_end` and becomes the contributing-data window marker
   for any future Pattern derived from this Observation. This is a
   distinct Observation field; Observations must not carry
   `training_window_end`.
3. **Score (xvision-eval, Stage 3 replay).** When the run finishes, the
   eval crate computes per-cycle returns, per-arm Sharpe / max-DD /
   profit factor. Each cycle's outcome is attached to its Observation
   via `cycle_id`. This binding is the link that makes memory outcome-
   conditioned.
4. **Distill (autoresearcher, V3 offline verb).** `xvn autoresearch run`
   reads Observations for a target slot or capability, clusters by
   regime / asset / outcome, asks an LLM to propose Patterns ("when in
   high-vol regime with negative funding, smaller size, tighter stop"),
   and writes candidate Patterns to a staging buffer (not yet promoted).
5. **Optimize (DSRs, offline verb).** `xvn optimize --use-memory-demos`
   takes the candidate Pattern's contributing Observations as the
   demo pool, runs MIPROv2 or GEPA against a `SharpeMetric` or
   `JudgeMetric`, and produces a compiled instruction + demo set.
   Optimizer output is written to a new child agent's `system_prompt`
   plus the `agent_slot_optimizations` lineage row.
6. **Mint (lineage, marketplace).** The compiled child agent gets a new
   `agent_id` ULID with `parent_agent_id` set. The candidate Patterns
   are linked to the optimization row via a new `pattern_optimizations`
   join. Marketplace minting (post-ERC-8004 work) reads the lineage
   chain as part of the attested artifact.
7. **Gate (paper-test, blind Finding).** The child agent runs against a
   holdout corpus disjoint from training. Numeric metric gate must pass
   first. LLM judge writes a Finding blind to the numeric outcome,
   strictly for qualitative context. Both must be recorded in the
   evidence ledger.
8. **Promote (Pattern -> recall path).** Patterns that survive the gate
   become recall-active. `promotion_state` flips from `staged` to
   `active` while `tier` remains `pattern`,
   `training_window_end` is set to the latest contributing
   `source_window_end`. Patterns inherit the operator-confirmed safety
   attestation chain.
9. **Recall (ClineSDK runtime, live or eval).** On the next decision
   cycle of any agent in the matching namespace (Global or AgentScoped),
   `MemoryRecorder.recall()` queries Patterns filtered by
   `training_window_end < current_scenario_start`, wraps each hit in
   case-law framing, prepends as a `<prior_observations>` block to the
   system_prompt before the sidecar dispatches.
10. **Demote (failure detection).** Patterns associated with cycles whose
    realized outcome falls below a per-Pattern threshold over a rolling
    window are soft-deleted (`forgotten_at` set). Janitor honors the
    grace window before hard-delete. The demote-on-failure rule is
    parameterized per Pattern, not global.

Every numbered step emits an event tagged with the same `flywheel_cycle_id`
so the trace dock can render an end-to-end ribbon: capture -> observe ->
... -> demote.

## Phase 0 - Inventory and proof harness

Preflight. Must not implement product features. Prevents hidden gaps.

Tasks:

- Audit current `MemoryRecorder` wiring in `execute_slot` and confirm both
  `recall()` and `record()` are reached on the live ClineSDK runtime path
  (Stage 1), the eval replay path (Stage 3), and the new chat-rail path
  (companion plan, phase 1).
- Audit migration set: `0001_init.sql` in `xvision-memory/migrations` plus
  `029_agent_slot_memory_mode.sql` in `xvision-engine/migrations`. Confirm
  fresh-DB and migrated-DB rollout both work.
- Inventory already-shipped memory surfaces before adding new ones:
  `xvn memory ls/show/add-pattern/rm/forget/undo-forget`,
  `/api/memory`, `/agents/memory`, per-agent Memory tab, eval-run
  MemoryPanel, and `MemoryRecallEvent`. Record any planned rename as an
  alias/migration, not as a duplicate surface.
- Build the leakage-regression suite skeleton at
  `crates/xvision-memory/tests/leakage_regression.rs`. Stub each F+L+T
  probe; they will be filled by later phases.
- Add the `flywheel_cycle_id` field to the unified event taxonomy from the
  chat-rail plan. Every memory-related event must carry this ID.
- Add a schema proposal for Observation market-window provenance
  (`source_window_start`, `source_window_end`) and for operator
  attestations. Phase 1 may implement it, but Phase 0 must at least
  prove every downstream gate knows which field it depends on.

Exit criteria: leakage-regression skeleton compiles, all three integration
paths emit `memory_recall` and `memory_write` events with non-empty
`flywheel_cycle_id`, evidence captured in `recall/temporal-filter-probe.txt`
(stub form).

## Phase 1 - Recorder hardening and observability completion

The V2D plan ships through Phase 5 in its own document but stops short of
end-to-end provenance. This phase closes that.

Tasks:

1. **Phase 5 events on the unified taxonomy.** `memory_recall`,
   `memory_write`, `memory_forget` emit on the same SSE stream the
   chat rail and trace dock consume (per companion plan phase 1.1).
   Event shape: `{ kind, namespace, decision_id, flywheel_cycle_id,
   memory_item_ids: [...], scenario_id, scenario_start, tier,
   promotion_state, count, prompt_hash_after_memory }`. No PII or full
   text leakage in the event itself; the trace dock fetches the body on
   demand subject to retention policy. Existing `MemoryRecallEvent`
   deserialization must remain backward-compatible or get an explicit
   event schema version bump.
2. **Decision-trace provenance thread.** `RecallResult.decision_id` is
   threaded through to the decision record so eval-review can correlate
   a poor decision to the Patterns it consulted. Persist either a
   SQLite-compatible TEXT JSON column or a join table
   `decision_recalled_memory(decision_id, memory_item_id, score,
   rank)`. Do not specify PostgreSQL-only `JSONB` in this repo.
3. **Observation market-window provenance.** Extend `MemoryRecorder::record`
   and the storage adapter so Observations carry
   `source_window_start`/`source_window_end`. Backtest/eval must pass
   the scenario-cycle briefing window; live/paper must pass the latest
   market-data timestamp used in the briefing. Autoresearcher may not
   run until this field is populated for its cohort.
4. **Operator attestation table.** Add `operator_attestations` with
   operator id/initials, surface (`cli`, `dashboard`, `chat-rail`),
   normalized warning text hash, timestamp, and optional signature.
   Manual Patterns with `training_window_end = NULL` must reference an
   attestation row.
5. **Forget grace window janitor.** Background task that hard-deletes
   `forgotten_at IS NOT NULL AND forgotten_at < NOW() - grace_days`.
   Grace defaults to 14 days; configurable per namespace. Janitor logs
   every deletion to the audit log.
6. **Undo-forget CLI/API.** Existing `xvn memory undo-forget` remains the
   canonical CLI spelling. Add `unforget` only as an alias if tests prove
   both spellings hit the same API and docs explain the alias. The route
   clears `forgotten_at` only inside grace and records operator
   attestation.
7. **Memory panel in run-detail UI.** Per-cycle list of recalled Patterns
   with similarity scores, case-law framing preview, click-through to
   the Pattern's full lineage, and a link to the exact post-memory
   prompt blob/hash. Lives in the existing eval-run MemoryPanel and the
   chat-rail trace dock per companion plan section A.1 item 1 (unified
   stream).

Exit criteria: every cycle's recalled Patterns visible in the dashboard,
queryable by CLI, exported in the trajectory store, and survive a
soft-delete + undo-forget round trip.

## Phase 1.4 - Cortex dependency spike and contract proof

This phase is a hard gate before Phase 1.5. It proves the upstream
dependency is usable before xvision commits to the adapter rewrite.

Tasks:

1. **Exact pin discovery.** Query upstream tags/commits and choose the
   candidate pin. Record why that pin was selected and whether xvision
   pins upstream directly or a light fork. The plan-date v1.3.0
   assumption is not authoritative.
2. **Compile spike.** Add `cortex-core` on a throwaway branch or
   scratch crate and compile against the exact APIs xvision expects:
   open/init, ingest, retrieve/search, context generation,
   consolidation, beliefs, forget/delete, plugin hooks, external
   embeddings, and MCP embedding/child-process mode if applicable.
3. **Schema dump.** Create a cortex database with representative
   Episodic, Semantic, forgotten, belief, and procedural rows. Save the
   schema and sample rows in the evidence ledger. Identify which
   migration steps can be SQL-only and which require Rust.
4. **Hook proof.** Implement the smallest possible F+L+T hook prototype
   and tests proving an invalid Observation write, invalid Pattern
   write, unsafe retrieval, consolidation bypass, and forget bypass are
   rejected.
5. **Boundary proof.** Add a grep/CI prototype proving no crate outside
   `xvision-memory` imports `cortex_core::*`.

Exit criteria: `evidence/.../cortex-adoption/dependency-spike.md`
contains the selected pin, API table, schema notes, hook proof, and a
go/no-go decision. Phase 1.5 cannot start on assumptions.

## Phase 1.5 - Adopt gambletan/cortex as the memory engine

This phase replaces the clean-room `xvision-memory` implementation with a
thin trading-safety adapter over `cortex_core::Cortex`. It is gating for
Pattern-aware Phase 2 work because the optimizer's demo selection benefits
   from cortex's multi-signal retrieval, and gating for cortex-backed Phase 3
   consolidation because cortex's consolidation engine overlaps part of what
   the autoresearcher does. Phase 2A may still use Observation-only demo
   pools after Phase 1 if the adapter spike is delayed.

The adapter rewrite is small (~150 SLOC target) but the migration of
existing rows + the plugin wiring is delicate. Treat this as a
distinct wave with its own evidence ledger sub-directory:
`evidence/2026-05-24-cortex-flywheels/cortex-adoption/`.

Tasks:

1. **Attribution PR (precedes any code).** Land:
   - `CREDITS.md` at repo root naming gambletan/cortex with upstream
     URL, license (MIT), role, and target version pin.
   - `LICENSES/gambletan-cortex.txt` copying the upstream LICENSE
     verbatim.
   - README architecture section updated with the memory-subsystem
     credit.
   - This doc + V2D plan + leakage notes annotated at first mention.
   Evidence: PR link, before/after of the four files, screenshot of
   the README rendered on github.
2. **Cortex contract audit.** Read every public API in
   the exact pinned `cortex_core` source selected by Phase 1.4.
   Document the API surface we depend
   on: `Cortex::open`, `ingest`, `retrieve`, `get_context`,
   `run_consolidation`, `observe_belief`, `add_fact`,
   `add_preference`, the plugin hook signatures, storage schema,
   external embedding seam, and MCP mode. Pin all of it.
   Decide which APIs we proxy through the adapter vs. expose directly.
   Evidence: `evidence/.../cortex-contract.md` with API table + our
   binding decision per row.
3. **Plugin design: F+L+T as a cortex plugin.** Write the trait
   implementations against cortex's compile-time hook system:
   - **Ingest hook** rejects writes that violate tier-shape invariants
     (Observation requires `(run_id, scenario_id, cycle_idx)`;
     Pattern requires `training_window_end` unless an
     `operator_attestations` row is linked).
   - **Retrieve hook** filters results by `tier='pattern' AND
     (training_window_end IS NULL OR training_window_end <
     scenario_start)`.
   - **Consolidation hook** intercepts cortex's Episodic->Semantic
     promotion. Promotion path emits candidates with
     `promotion_state='staged'` and is rerouted through
     autoresearcher's numeric gate before any candidate becomes active
     (Phase 3 dependency).
   - **Forget hook** preserves the grace-window semantics
     (`forgotten_at` timestamp, janitor sweep).
   Evidence: integration test asserting each hook fires and enforces
   its invariant, plus a negative test per hook proving the wrong
   write is rejected.
4. **Schema migration: `xvision-memory` rows -> cortex.** Write a
   migration plan using Phase 1.4's schema dump. Prefer a Rust
   migration binary if cortex's layout is not a simple table superset;
   do not force SQL-only migration. Run migration in shadow mode first:
   old store read-only, cortex store populated, row counts and checksums
   compared, then adapter switched. Map columns:
   - xvision `tier='observation'` -> cortex Episodic
   - xvision `tier='pattern'` -> cortex Semantic
   - `(run_id, scenario_id, cycle_idx)` -> cortex episode metadata
   - Observation `source_window_start` / `source_window_end` -> cortex
     episode metadata used by autoresearcher
   - Pattern `training_window_end`, `promotion_state`, and
     `attestation_id` -> cortex metadata + F+L+T plugin state
   - `forgotten_at` -> cortex's decay state with explicit forgotten
     flag
   Evidence: migration runs on a populated dev DB; row counts and
   content hashes match before/after; F+L+T probes still green;
   rollback path documented and exercised against a fixture DB.
5. **Adapter crate rewrite.** `xvision-memory` becomes the
   trading-safety adapter, NOT the engine. Public API
   (`MemoryRecorder::recall`, `MemoryRecorder::record`,
   `MemoryStore::*`) preserved at the boundary so engine call sites
   in `crates/xvision-engine/src/agent/execute.rs` don't change.
   Internally, every call delegates to cortex through the plugin
   hooks. Add adapter methods for list/get/delete/admin views so
   dashboard/API code no longer reads `memory_items` through
   `MemoryStore::pool()`. The 4 source files shrink only if the API
   stays readable; SLOC reduction is evidence, not a requirement.
   Embedder trait stays if cortex accepts an external embedding
   function; otherwise the Phase 1.4 spike must document the replacement.
   Evidence: diff stat showing the SLOC reduction, integration
   tests against the new adapter, engine call-site list confirming
   no engine code changed.
6. **Tier renaming reconciliation.** Code's `Tier { Observation,
   Pattern }` and plan's `resource/skill` were already divergent;
   align the docs to the code AND to cortex's `Episodic/Semantic`
   in a single pass. Update the 2026-05-21 V2D plan, the leakage
   notes, and any tool/CLI surface. Evidence: docs diff, MCP/CLI
   help-text screenshots.
7. **Engine call-site stability proof.** Run the full
   `cargo test --workspace` plus the leakage-regression suite plus
   `xvn ab-compare` on a fixture corpus. All green, no behavior
   change at the engine boundary. Evidence: test transcript,
   ab-compare delta showing no metric change beyond noise.
8. **Cortex version-pin guard.** Add a CI check that fails if the
   `cortex-core` git rev in `Cargo.toml` changes without a
   companion `evidence/.../cortex-adoption/version-bump-N.md`
   audit note. Evidence: CI workflow file diff, a sample
   audit note for the selected pin -> next.
9. **Performance baseline.** Benchmark recall latency at 1k, 10k,
   100k items per namespace pre- and post-adoption. cortex's HNSW
   should be substantially faster at 10k+; xvision-memory's linear
   scan was the prior implementation. Evidence: `cargo bench`
   output before/after, written analysis.
10. **Raw import guard.** Add a repo-wide check that rejects
    `use cortex_core` or `cortex_core::` outside `crates/xvision-memory`
    and its tests. Evidence: CI output and a failing fixture/sample.

Exit criteria: engine runs through a full backtest cycle with the
cortex-backed adapter, all leakage probes green, MCP tools available
through xvision-safe wrappers only (Phase 4/6 dependency), Bayesian
belief API callable through the adapter (but no production use yet -
Phase 6), attribution surfaces all in place, and no non-adapter crate
imports cortex directly.

## Phase 2 - DSRs x memory: demo pools and holdout discipline

Wire memory into the DSRs optimizer foundation specified in the companion
plan.

Tasks:

1. **Demo source selection.** `xvn optimize` accepts
   `--demo-source { fresh-recorder | frozen-snapshot | manual-csv }`.
   Default `frozen-snapshot` for reproducibility. `fresh-recorder`
   queries Observations live and is marked as non-reproducible in the
   lineage row.
2. **Holdout discipline.** `--holdout-split 70/15/15` train/dev/holdout
   is mandatory. The optimizer must report metric on dev (during
   optimization) and holdout (final, untouched during search).
   Optimizer is forbidden from seeing holdout examples during compile.
3. **Cohort filtering.** `--cohort regime=Trend,asset=BTC` filters
   Observations to a stratified cohort. The lineage row records the
   cohort query verbatim for reproducibility.
4. **Memory-demo audit script.** `scripts/audit-memory-demos.sh` exports
   the demo set actually used for an optimization, plus the holdout
   set, plus a hash of each. Verifies no overlap. Runs as part of CI on
   every optimization-touching PR.
5. **Pattern-aware optimizer.** When recalled active Patterns exist for the
   target capability, the optimizer's initial instruction may include
   them as a prior. This is opt-in; default off because it conflates
   the optimizer's exploration with the runtime's recall. This task may
   start only after Phase 3 has produced active Patterns; before then,
   Phase 2 is Observation-demo-only.
6. **Lineage join table.** `pattern_optimizations { optimization_id,
   pattern_id, role: 'demo_source' | 'prior' }`. Recorded at optimize
   time. Drives the "which Patterns trained this agent" view in the UI.

Exit criteria: A complete `xvn optimize` invocation produces a child
agent whose `agent_slot_optimizations` row links to specific
contributing Observations and, when Phase 3 is active, contributing
Patterns; holdout metric reported; demo set hash verifiable; no
Observation in the demo/dev set has `source_window_end` or outcome
window overlapping the holdout scenario window.

## Phase 3 - Autoresearcher distillation

Implements `2026-05-09-karpathy-autoresearcher-design.md` as a concrete
verb wired into the rest of the system.

Tasks:

1. **`xvn autoresearch run`.** Reads Observations for the target
   slot/capability over a configurable window. Clusters by regime,
   asset, outcome buckets. For each cluster, calls an LLM (offline
   model, not the live trader's model) to propose 1-N candidate
   Patterns. Writes them as `tier='pattern',
   promotion_state='staged'` with full provenance to source
   Observations. `tier='pattern-staged'` is forbidden because it
   breaks the `Tier { Observation, Pattern }` safety boundary.
2. **Numeric gate.** For each staged Pattern, run a replay over a
   holdout corpus. Compute outcome metrics. Promote only Patterns that
   beat a per-capability null hypothesis (e.g., for Trader: Sharpe
   improvement > 0 with t-stat above operator-set threshold).
3. **Judge Finding (blind).** A second offline LLM call writes a
   qualitative Finding for each staged Pattern. The judge sees the
   Pattern text, the Observation cohort, the cohort outcome
   distribution, but NOT the numeric gate result. Finding is
   informational; it cannot override the numeric gate.
4. **Promotion.** Staged Patterns that pass the gate flip
   `promotion_state='active'`, get `training_window_end` set to latest
   contributing `source_window_end`, and become recall-active.
   Promotion writes a `pattern_promoted` event.
5. **Demotion on failure.** Background task watches Patterns recalled
   in recent live or eval cycles. If a Pattern's recall cohort shows
   negative outcome correlation over a rolling window (configurable,
   default 30 cycles), `forgotten_at` is set and `pattern_demoted`
   fires. Operator can `xvn memory undo-forget` during grace.
6. **Mutator-skill ladder (deferred from V3).** Per-capability optimizer
   metrics on the autoresearcher itself. Tracks: candidates proposed,
   gate pass rate, promotion durability (how long promoted Patterns
   survive before demotion), diversity score. Recorded in
   `autoresearch_runs` table.

Exit criteria: A single `xvn autoresearch run` produces at least one
promoted Pattern visible in the dashboard, recall-active on the next
backtest cycle, with full lineage from contributing Observations through
the LLM judge Finding to the numeric gate result. The same Pattern can be
demoted by a subsequent run if its recall cohort underperforms.

## Phase 4 - Flywheel surface and operator UX

The flywheel is a real product, not a side effect. Operators need to see
velocity, intervene, and trust it.

Tasks:

1. **Flywheel velocity dashboard.** Per-agent chart showing: Observations
   captured per day, Patterns promoted per week, Patterns demoted per
   week, optimized child agents minted per week, average lineage depth.
   Lives at `/agents/:id/flywheel` route. No popups (per project rule).
2. **Pattern lifecycle ribbon.** For each Pattern: timeline of recall
   count, outcome correlation, demote risk score. One-click drill to
   the contributing Observations and the LLM judge Finding.
3. **Leakage-regression dashboard.** Surfaces the result of the
   leakage-regression suite (Phase 0) on every PR. Red bar if any F, L,
   or T probe fails.
4. **Manual seeding form.** Operator can author a Pattern directly
   (training_window_end = NULL with attestation). Form requires the
   operator to type their initials and re-confirm the safety
   implications; server writes an `operator_attestations` row and links
   the Pattern to it. The CLI `add-pattern --training-end` remains
   normal; NULL-window manual seeds require an explicit attestation flag
   plus stored attestation evidence.
5. **Chat-rail integration.** The chat rail's `act` mode (per companion
   plan A.1 item 3) exposes `optimize_slot` and `autoresearch_slot` as
   write-tools. Both are ask-mode by default per the three-state tool
   policy.
6. **Skills.** `.claude/skills/xvision/memory-ops`,
   `xvision/autoresearch-ops`, `xvision/flywheel-ops`. Each carries
   operator runbook, safety checklist, common-failure-mode guide.

Exit criteria: An operator can drive a complete flywheel cycle from the
chat rail with no CLI access required, see the resulting Pattern
recalled in the next backtest, and inspect every step's lineage from one
URL.

## Phase 5 - Closed-loop velocity hardening

The system runs. Now it has to run safely under volume.

Tasks:

1. **Compounding bias guard.** When an agent's optimization uses
   Observations sourced predominantly (>50%) from its own ancestors,
   the optimizer emits a warning and requires operator override. Logged
   to lineage.
2. **Survivor bias counter.** Demoted Patterns are not deleted from the
   audit log (only soft-deleted from recall). Autoresearcher input
   cohorts include a configurable fraction of demoted-Pattern
   Observations to prevent the corpus from drifting toward
   success-only.
3. **Memory poisoning detection.** Anomaly detector watches recall
   patterns: if a single newly-promoted Pattern is recalled in >80%
   of cycles within its first week, the system flags it for manual
   review. Plausibly fine; plausibly a bad universal Pattern.
4. **Leakage drift audit.** Quarterly automated audit: pick 100 random
   `pattern_promoted` events, recompute `training_window_end` from
   source Observations, fail audit if any drift.
5. **Judge calibration drift.** The LLM judge model and prompt are
   pinned per autoresearch run. Recorded in `autoresearch_runs.judge_model_hash`.
   Operator must explicitly migrate to a new judge; Findings from
   different judges are not directly comparable.
6. **Flywheel velocity report.** Weekly export
   `evidence/.../flywheel-velocity-report.md` summarizing cycle counts,
   Pattern durability, demote ratios, gate pass rates, judge agreement
   with numeric gate. Operator reads this to spot regressions.

Exit criteria: System runs unattended for two weeks across paper
trading + scheduled autoresearch + scheduled optimize, with no manual
intervention, leakage probes green, velocity report produced, no
single Pattern recalled in >80% of cycles, demote ratio plausible
(some positive demote rate is healthy; zero means the gate is too
permissive).

## Phase 6 - Adopt remaining cortex features

Phase 1.5 wires cortex-core in as the engine. Phase 6 exposes the
features the clean-room port discarded, one at a time, each with its
own proof gate. Order is driven by leverage; nothing in this phase is
mandatory for Phase 5 exit, but each item unlocks measurable upside.

Each sub-phase carries its own evidence sub-directory:
`evidence/.../cortex-features/<feature>/`.

### 6.1 - Bayesian belief system

Cortex ships first-class Bayesian beliefs (`observe_belief(name,
value, confidence)`) with auto-updating confidence scores and
contradiction detection. Use cases for trading:

- Per-regime beliefs: `belief("trend_persists_after_2_consec_green",
  true, conf)` updates on each subsequent cycle's outcome.
- Per-pattern confidence: every recalled Pattern's prior reliability
  is itself a belief; recall-time score weights similarity by belief
  confidence.
- Risk gate input: a belief about regime stability feeds the risk
  layer's size-throttling logic.

Tasks: expose `Belief` as an adapter-backed memory kind or view, not as
a third value in the existing `Tier` enum unless the F+L+T contract is
amended first. Add `MemoryRecorder::observe_belief()`, surface beliefs
in the chat-rail and trace dock, integrate belief confidence into
recall scoring (Phase 6.2 dependency partial overlap).

Evidence: integration test showing a belief updating on opposing
evidence with correct Bayesian math (compare to scipy reference);
chat-rail screenshot of belief panel; CLI transcript of
`xvn belief observe` and `xvn belief list`.

### 6.2 - Multi-signal retrieval

Cortex's `retrieve` combines similarity + temporal recency + salience +
social + channel. xvision-memory currently uses similarity only.

Mapping for trading:
- **Similarity** - cosine of query embedding vs Pattern embedding (kept)
- **Temporal** - recency weighting with configurable decay; relevant
  for regime-shift detection
- **Salience** - importance score driven by outcome correlation
  (Patterns whose recalls correlated with good outcomes outrank
  Patterns with weaker correlation)
- **Social analog** - per-asset weighting (Patterns about BTC outrank
  ETH Patterns when querying BTC context)
- **Channel analog** - regime weighting (Patterns developed in Trend
  regime outrank Range Patterns when current regime is Trend)

Tasks: implement the salience updater (writes outcome correlation back
to cortex on each cycle close), wire the asset and regime hints into
the retrieve call, A/B test multi-signal vs similarity-only on a
holdout corpus.

Evidence: A/B holdout report showing multi-signal recall produces
better-correlated Patterns (measure: realized Sharpe of cycles using
multi-signal vs similarity-only recall, same agent, same corpus).

### 6.3 - HNSW vector index

Cortex v1.1 shipped HNSW (50k search: 91µs). Linear cosine over f32
blobs in xvision-memory was fine at <10k items per namespace; HNSW
unlocks 100k+ without recall becoming a hot-path bottleneck. Less
urgent if Pattern counts stay small.

Tasks: enable HNSW in the cortex adapter (likely a config flag, not
new code), benchmark recall latency at 1k / 10k / 50k / 100k items
per namespace, monitor index rebuild cost.

Evidence: `cargo bench` output at each scale, written latency
analysis, decision on whether to default-on HNSW or leave it
opt-in per namespace.

### 6.4 - Continuous consolidation cycle

Cortex's consolidation engine runs as a background cycle that does
Episodic->Semantic promotion, decay, pattern extraction. xvision's
autoresearcher is an offline verb. These overlap; the right
relationship is:

- Cortex's consolidation handles the cheap stuff: stale-decay,
  obvious-duplicate-merge, frequency-based promotion candidates.
- The autoresearcher (Phase 3) handles the expensive stuff: LLM
  proposal of new Patterns, numeric gate, judge Finding.
- Cortex's consolidation flags candidates; autoresearcher decides.

Tasks: register the autoresearcher's gate as cortex's promotion
hook (Phase 1.5 task 3 already establishes the seam); decide the
background cadence (default: hourly during paper trading, on
demand during backtest); surface consolidation activity in the
flywheel velocity dashboard.

Evidence: log of consolidation cycle output, verification that
no promotion bypasses the autoresearcher gate, dashboard
screenshot of cycle activity.

### 6.5 - Token-budgeted context injection

Cortex's `get_context(max_tokens, channel, person)` returns
LLM-ready context within a token budget. xvision-memory currently
prepends raw recalls to the system_prompt. Under prompt-cache
constraints (Anthropic's 4-block cache), unbounded prepends break
cache hits.

Tasks: replace the current `render_recalled_patterns()` wrapper
with a call to `cortex.get_context()` with a per-slot
`memory_context_budget` field (default: 1500 tokens). Maintain
the case-law framing (L in F+L+T) — verify cortex's output
already wraps or add it.

Evidence: prompt-cache hit-rate before/after on a representative
backtest, byte-count distribution of recalled context per cycle.

### 6.6 - MCP tools surface

Cortex ships MCP tools (memory_ingest, memory_search,
memory_context, belief_observe, fact_add, preference_set, etc.).
They must not drop directly into the chat rail's tool list. xvision
registers a safe wrapper set whose write tools enforce F+L+T,
operator policy, audit logging, and namespace restrictions server-side.
Read-only tools may be auto-enabled after policy review; write tools
default to ask-mode and may be disabled entirely in live trading.

Tasks: expose cortex's MCP server as a child process of the xvision
dashboard or embedded adapter only behind xvision wrappers; map each
raw cortex tool to one of: blocked, read-only wrapper, write wrapper,
or deferred. Wire the wrapper tools into the chat-rail tool registry
with per-tool policies (`enabled`, `auto_approve`) defaulting to
ask-mode for writes; document each exposed wrapper and each intentionally
blocked raw tool in the operator skill.

Evidence: MCP tool transcripts showing each exposed wrapper callable
from the chat rail, screenshots of the tool policy panel listing
wrapper/blocked/deferred status, negative tests proving raw write tools
cannot bypass xvision policy, and an end-to-end demo where the operator
asks the chat rail "what do you remember about my preferences" and gets
a useful answer from cortex through a read wrapper.

### 6.7 - Contradiction detection

Cortex flags fact conflicts automatically. In trading: "Pattern X
says size up on high vol, Pattern Y says size down on high vol"
should surface as a contradiction before it bites you live.

Tasks: enable cortex's `contradiction_check` on Pattern promotion
(an additional gate alongside the numeric metric and the LLM
judge); add an operator-facing contradictions panel in the
flywheel dashboard.

Evidence: synthetic contradiction test (manually plant two
conflicting Patterns; verify the second promotion is gated),
screenshot of contradictions panel.

### 6.8 - Procedural memory tier for compiled prompts

Cortex's Procedural tier captures "learned routines." Trading
analog: DSRs-optimized agent prompts are closer to procedural
than semantic. Currently the optimizer writes back to
`AgentSlot.system_prompt`. Treating compiled prompts as
procedural memory makes the optimization->recall loop explicit
and gives them their own lifecycle (decay, contradiction,
versioning).

Tasks: define a procedural mapping in the adapter without overloading
the existing Observation/Pattern recall tier. Update the optimizer
write-back path to also record a procedural memory item linking to the
new `AgentSlot.system_prompt` value, surface "compiled prompts" as a
distinct view in the agent detail UI, and prove procedural items are not
returned by Pattern recall unless an explicit future amendment says so.

Evidence: procedural memory item visible after an `xvn optimize`
run, lineage from contributing Observations -> Patterns ->
Procedural compiled prompt.

### Phase 6 ordering

```
6.1 Beliefs   ---> 6.2 Multi-signal retrieval ---> 6.4 Consolidation
6.3 HNSW (parallel, any time)
6.5 Token-budgeted context (parallel with 6.1, 6.2)
6.6 MCP surface (after Phase 4 chat-rail integration)
6.7 Contradictions (after 6.1, 6.4)
6.8 Procedural prompts (after Phase 2 optimizer foundation)
```

## Risk register

Catalog of failure modes the design must resist.

1. **Goodhart on Sharpe.** Optimizing on backtest Sharpe is not the
   same as optimizing on real future returns. Mitigation: mandatory
   holdout (Phase 2 task 2), demote-on-live-failure (Phase 3 task 5),
   compounding-bias guard (Phase 5 task 1).
2. **Pattern monoculture.** Autoresearcher converges on Patterns that
   are easy to extract rather than Patterns that are useful. Mitigation:
   diversity metric in the mutator-skill ladder (Phase 3 task 6),
   memory-poisoning detection (Phase 5 task 3).
3. **Look-ahead leakage via creative scenario naming.** Operator names
   a backtest scenario "20240801_flash_crash" but actually runs it on
   2020 data; T filter trusts the scenario_start. Mitigation: scenario
   metadata is signed; mismatch between scenario name and data range
   surfaces as a CI failure. Out of scope for this plan; tracked in
   eval crate hardening.
4. **Cost runaway.** LLM judge and embedder costs scale with cycle
   count. Mitigation: per-run cost budgets recorded; janitor warns
   when daily spend exceeds threshold; embedder cache (already in
   xvision-intern via foyer-style LRU once V2D Phase 5 lands).
5. **Stale Pattern drift.** A Pattern that worked in 2024 regime
   doesn't work in 2026 regime. Demote-on-failure handles the
   obvious cases. Mitigation: Pattern decay metric in the velocity
   report; operator can manually set a max-age policy per namespace.
6. **Operator attestation theater.** Operator clicks through manual
   seeding without reading the safety implications. Mitigation:
   typed-initials requirement on the form (Phase 4 task 4), audit log
   of every attestation, periodic review surface in the operator
   skill.
7. **Cline sidecar mismatch.** Trajectory store schema changes
   between sidecar versions; old Observations no longer parse.
   Mitigation: trajectory format is versioned (per Stage 2 plan),
   memory recorder records `cline_sdk_version` on each Observation,
   recall path filters incompatible versions cleanly.
8. **DSRs breaking changes.** Crate is beta. Mitigation: pin version,
   persist optimizer output as plain `(instruction, demo_json)` not
   dspy serialization, run `xvn optimize` from a separate
   `xvision-optimizer` image (per companion plan B.8) so the runtime
   image doesn't pull dspy-rs.
9. **Recall determinism regression.** A future change to similarity
   ranking (e.g., adding HNSW) silently changes recall ordering and
   thus prompts and thus outcomes. Mitigation: determinism probe in
   the leakage-regression suite (Phase 0), pinned recall ordering
   tests on every PR touching store.rs.
10. **Tier confusion in new code.** A future contributor adds a third
    tier or reads Observations into a prompt "just for context."
    Mitigation: tier-shape invariants enforced at the store boundary,
    not at call sites; PR template includes a "did you touch the
    memory tier?" checkbox; F+L+T document is the authoritative
    reference.
11. **Cortex upstream drift / dependency mismatch.** The upstream tag
    set can move quickly and the plan-date candidate may be stale by
    phase entry. As of the review pass on 2026-05-25, tags newer than
    v1.3.0 exist. If upstream changes API or schema, xvision either
    pins off it, forks, or migrates. Mitigation pyramid (lowest cost
    first):
    - **Tight git pin** (Phase 1.5 task 8). Tag-pin or commit-rev
      pin; never `branch = "main"`. Version bumps require an
      explicit audit note. This is mandatory.
    - **Light fork from day one**: create `latentwill/cortex-fork`
      private mirror, pin xvision against the fork's tag rather
      than upstream. Lets us cherry-pick fixes from upstream when
      they come, ship our own when they don't, without changing
      xvision's `Cargo.toml` shape. Low overhead.
    - **Vendored copy** as escalation: if upstream goes cold for
      6+ months, vendor the relevant subset (cortex-core, the
      MCP tools we use) into `crates/vendored/cortex-*`. Update
      attribution accordingly; preserve LICENSE; explicit note
      in CREDITS.md that the vendored tree may diverge.
    - **Upstream contribution**: when we fix something in our
      fork, open a PR upstream. Reduces fork drift even if
      upstream merges are slow.
    The default plan is tight pin selected by Phase 1.4 + light fork if
    the spike shows maintainer/API risk. Vendoring is a last resort.
12. **Cortex API breaking changes between pins.** When we bump
    the pinned version, public API shape may move. Mitigation:
    the `xvision-memory` adapter is the single point of contact;
    no engine code calls `cortex_core::*` directly. API breaks
    are caught by the adapter's unit tests, never by an engine
    test failing in a confusing way. Phase 1.5 task 7 enforces
    this discipline.
13. **License-attribution drift.** Future contributors add cortex
    features without updating LICENSE/CREDITS/README. Mitigation:
    add a CI check that every Cargo.toml change touching
    `cortex-core` must come with a CREDITS.md timestamp bump or
    explicit "no attribution change needed" PR-description tag.
14. **Raw SQL adapter bypass.** Current `/api/memory` code reads the
    xvision table directly for list/get/delete. After cortex adoption,
    that bypass would either break or silently skip plugin policy.
    Mitigation: Phase 1.5 replaces raw SQL with adapter list/get/admin
    methods before switching storage.
15. **Prompt display drift.** The operator may see a reconstructed
    prompt while the model received a different byte sequence after
    memory injection. Mitigation: persist final prompt hash/blob after
    memory injection and make the dashboard render that artifact.
16. **MCP policy bypass.** Raw cortex MCP tools can write facts,
    preferences, or memories without xvision provenance. Mitigation:
    expose only xvision-wrapped tools; raw write tools are blocked by
    default and covered by negative tests.

## Phase ordering and dependencies

```
Phase 0 (preflight)
   ---> Phase 1 (recorder hardening)
   ---> Phase 1.4 (cortex dependency spike) ---> Phase 1.5 (adopt cortex-core; ATTRIBUTION FIRST)
        \-> Phase 2A (DSRs x Observation demo pools)
   ---> Phase 3 (autoresearcher)
   ---> Phase 2B (Pattern-aware optimizer)
   ---> Phase 4 (surface + UX)
   ---> Phase 5 (velocity hardening)
   ---> Phase 6 (cortex feature exposure; itemized sub-phases)
```

Phase 2A may proceed after Phase 1 because Observation demo pools do
not require active Patterns or cortex. Phase 1.4 and 1.5 remain the
cortex adoption path. Phase 3 can proceed after Phase 1 and should use
the active adapter available at the time; cortex-backed consolidation
waits for Phase 1.5. Phase 2B depends on Phase 3 active Patterns.
Phase 5 is ongoing once entered; the "exit criteria" represents the
gate for declaring the wave shipped, not the end of work. Phase 6
sub-phases are independently schedulable; 6.3 (HNSW) and 6.5 (token
budget) can run at any time after 1.5 if Phase 1.4 proved the relevant
APIs.

The attribution PR (Phase 1.5 task 1) is the earliest landable item
and SHOULD ship before any cortex-core code lands. This both honors
MIT obligations early and signals intent to the upstream maintainer.

## Cross-references

- Companion: `2026-05-24-chat-rail-and-strategy-agents-evaluation.md`
  (chat rail, DSRs foundation, strategy-agent improvements).
- Runtime: `2026-05-24-cline-runtime-unification-design.md` and stages
  0-4 plans.
- Memory: `2026-05-21-cortex-memory-integration-plan.md`,
  `2026-05-21-v2d-memory-cortex-tiers-and-leakage.md`,
  `2026-05-11-autoimproving-memory-survey.md`.
- Reference design: https://github.com/gambletan/cortex (4-tier memory,
  Bayesian beliefs, people graph, multi-signal retrieval, MCP server,
  HNSW vector index, consolidation engine, 25 MCP tools, plugin hooks).
  Mirrored but not vendored.
- Autoresearcher: `2026-05-09-karpathy-autoresearcher-design.md`.
- Capability model: `2026-05-22-capability-first-agent-model-and-graph-composition.md`.
- Optimizer thread: `2026-05-21-optimizer-and-capability-framing-handoff.md`,
  `team/intake/archive/2026-05-21-dspy-dsrs-optimizer-adoption.md`.

## Open questions

These are real decisions deferred to phase entry. Each must be answered
in the evidence ledger before the relevant phase begins.

1. Default holdout split. 70/15/15 is the proposed default; operator may
   want stricter (60/20/20) for sparse cohorts. Decide at Phase 2 entry.
2. LLM judge model for autoresearcher. Pinned per run, but a default is
   required. Anthropic Haiku-3.5 or 4 are the candidates. Decide at
   Phase 3 entry.
3. Demote-on-failure rolling window default. 30 cycles is the proposed
   default; could be 7 days or 90 days depending on cycle cadence.
   Decide at Phase 3 entry.
4. Marketplace lineage view depth. Should the marketplace expose full
   ancestor chains or N-level summaries? Decide at Phase 4 entry,
   coordinating with the marketplace minting plan.
5. Cost-budget defaults per namespace. Decide at Phase 5 entry.
6. Mutator-skill ladder visibility to operators vs. internal-only.
   Defaulting to operator-visible (operator should know how good
   their autoresearcher is at being an autoresearcher), but the metric
   is meta enough to confuse. Decide at Phase 5 entry.
7. Phase 6 feature ordering inside the broad ladder. Phase 6 already
   ranks 8 features by leverage, but the specific entry point can
   shift based on what the chat rail and DSRs work expose first.
   Decide at Phase 5 exit.
8. Light-fork vs. tight-pin from day one. Phase 1.5 task 8 specifies
   a tight git pin. The Risk Register (item 11) recommends an
   additional `latentwill/cortex-fork` private mirror as the
   default pin target. Decide at Phase 1.5 entry: pin upstream
   directly, or pin our fork. Light-fork is the recommended
   default given the 2-month upstream silence.
9. Pin policy for cortex tag bumps. Quarterly review, on-demand,
   or never (lock to the selected Phase 1.4 pin)? Recommended:
   on-demand with an
   audit gate, never automatic. Decide at Phase 1.5 entry.
10. Cortex API and tag selection. Phase 1.4 must choose the exact pin,
    prove the Rust API, and decide whether plugin hooks are sufficient.
    If hooks do not exist or cannot enforce F+L+T, either switch to the
    sidecar/wrapper path or keep the in-tree store until the dependency
    can be safely wrapped.
11. Prompt provenance storage. Decide whether final post-memory prompts
    live in the trajectory store, observability blob store, or both.
    The decision must precede Phase 1 exit.
12. Observation market-window source. Decide which engine field supplies
    `source_window_start/end` for each runtime path: backtest, paper,
    live, chat-rail replay, and future multi-asset cycles.

## Final acceptance criteria

The work ships through milestone gates. Do not hold early PRs hostage to
the two-week soak, and do not declare the whole flywheel operational until
all gates have passed.

### Gate A - Cortex adoption shipped

1. **Attribution is live.** `CREDITS.md` exists at repo root with
   gambletan/cortex named, `LICENSES/gambletan-cortex.txt` carries
   the upstream LICENSE verbatim, README architecture section links
   to the upstream repo, this doc and the V2D plan and the leakage
   notes carry inline attribution at first mention.
2. **Dependency proof is live.** Phase 1.4 selected the exact cortex pin,
   compiled the API, dumped schema, proved hooks or selected a fallback,
   and recorded the go/no-go decision.
3. **Cortex is the engine or the fallback is explicit.** If Phase 1.4
   proved the direct adapter safe, `xvision-memory` is an adapter over
   `cortex_core::Cortex`; otherwise this gate records the fallback path
   and blocks any claim that cortex is the engine. In both cases, engine
   call sites remain behind the `xvision-memory` API and F+L+T tests are
   green.
4. **Existing rows migrated.** The dev DB pre-migration and
   post-migration carry the same Observation/Pattern row counts and
   content hashes; no data loss; rollback path documented and exercised.
5. **Raw bypasses are closed.** `/api/memory`, CLI, dashboard, MCP,
   optimizer, and autoresearcher access memory through adapter methods;
   CI blocks `cortex_core::*` imports outside `xvision-memory`.

### Gate B - Offline flywheel works

6. **A new agent created today**, given a memory mode and run on a
   paper corpus, captures Observations on every cycle with
   `(run_id, scenario_id, cycle_idx, source_window_end)`.
7. After enough cycles, `xvn autoresearch run` proposes Patterns,
   gates them, stores staged candidates with `promotion_state='staged'`,
   and promotes at least one by flipping `promotion_state='active'`.
   Cortex's consolidation engine may surface candidates but does not
   bypass the autoresearcher gate.
8. The promoted Pattern is recalled in the next paper cycle, visible in
   the trace dock with full lineage and final prompt hash/blob. If Phase
   6.5 has landed, the prepended context is token-budgeted.
9. `xvn optimize --use-memory-demos` produces a child agent that beats
   its parent on a holdout corpus, with the lineage row linking demo
   Observations and any seeding Patterns. If Phase 6.8 has landed, the
   compiled prompt is also recorded as a procedural memory item.
10. The child agent runs in paper; its outcomes feed back as new
    Observations; the flywheel velocity report shows the cycle closing.
11. **Leakage-regression suite is green** against the active adapter.
    Every F+L+T probe passes after the migration.

### Gate C - Operator surfaces complete

12. Existing CLI, dashboard, API, and chat-rail surfaces expose the same
    capabilities without duplicate naming drift. `xvn memory
    ls/show/add-pattern/rm/forget/undo-forget` remains canonical unless
    aliases are documented and tested.
13. An operator with no CLI access can drive the full loop from the
    chat rail. If Phase 6.6 has landed, only xvision-wrapped cortex MCP
    tools are surfaced through the chat-rail tool registry with
    per-tool policies and negative tests for raw write bypass.
14. Manual NULL-window Pattern seeding requires a persisted
    `operator_attestations` row from CLI, dashboard, or chat rail.

### Gate D - Soak and fork readiness

15. The system survives two weeks unattended without leakage drift,
    monoculture, or cost runaway.
16. **Fork-readiness proof.** A documented procedure exists for
    forking and pinning to the fork (Risk Register item 11). It has
    been exercised at least once on a non-functional bump (e.g., a
    doc-only or build-config commit) to prove the path works before we
    need it under pressure.
When all gates hold, the flywheel is operational, the cortex
adoption is complete, the attribution is honored, and the wave
closes.
