---
track: agent-graph-capability-dispatch
lane: foundation
wave: agent-graph-2026-05-22
worktree: .worktrees/agent-graph-capability-dispatch
branch: task/agent-graph-capability-dispatch
base: origin/main
status: deferred
depends_on:
  - agent-graph-capability-schema  # PR #527 — merge first
blocks:
  - agent-graph-filter-capability
  - agent-graph-unified-recorder
  - agent-graph-template-capabilities
stacking: declared:agent-graph-capability-schema
allowed_paths:
  - crates/xvision-engine/src/agent/pipeline.rs
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/dispatch_capability.rs
  - crates/xvision-engine/src/agent/edge_predicate.rs
  - crates/xvision-engine/src/agent/mod.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-engine/src/strategies/validate.rs
  - crates/xvision-engine/tests/agent_graph_dispatch.rs
  - crates/xvision-engine/tests/agent_graph_edge_predicate_eval.rs
  - crates/xvision-engine/tests/agent_graph_router_dispatch.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/agents/model.rs  # Phase A owns; do not extend
  - crates/xvision-engine/src/agents/capability.rs  # Phase A owns
  - crates/xvision-engine/src/agents/store.rs  # Phase A owns
  - crates/xvision-engine/src/strategies/agent_ref.rs  # Phase A owns
  - crates/xvision-observability/**  # Phase D owns recorder unification
  - frontend/web/**  # Phase F owns UI
interfaces_used:
  - xvision_engine::agents::Capability + AgentSlot::capabilities (from Phase A)
  - xvision_engine::strategies::agent_ref::AgentRef::activates (from Phase A)
  - xvision_engine::strategies::agent_ref::PipelineEdge::condition + EdgePredicate (from Phase A)
  - xvision_engine::agent::execute::execute_slot (existing — receives capability-typed dispatch input)
  - xvision_engine::agent::llm::LlmDispatch (existing)
  - xvision_observability::ObsEmitter (existing — emits decision spans + tool_calls + events from Phase D's predecessor F43)
parallel_safe: false
parallel_conflicts:
  - memory-aware-eval-findings  # touches pipeline.rs if it lands first
  - indicator-tool-wiring  # already in flight (#521) — must merge first
verification:
  - cargo fmt --check
  - cargo clippy --workspace --tests -- -D warnings
  - cargo test -p xvision-engine --test agent_graph_dispatch
  - cargo test -p xvision-engine --test agent_graph_edge_predicate_eval
  - cargo test -p xvision-engine --test agent_graph_router_dispatch
  - cargo test -p xvision-engine
  - cargo build --workspace
acceptance:
  - New `dispatch_capability(slot, capability, briefing, prev_outputs, edges) -> Result<AgentOutput>` function at `crates/xvision-engine/src/agent/dispatch_capability.rs` — single seam that routes by `Capability` to a typed handler per kind. Replaces the role-string-gated branches scattered across `pipeline.rs`.
  - `AgentOutput` typed sum at the same module: `enum AgentOutput { Trader(TraderDecision), Filter(FilterSignal), Critic(Critique), Intern(InternObservation), Router(RouteSelection) }`. Trader is unchanged; the others are stub-shaped structs in Phase B (real semantics land per-kind in Phases C-E).
  - `run_agent_pipeline` in `pipeline.rs` rewritten to iterate strategy.agents, resolve `activates` to a concrete `Capability` (falling back to the slot's first capability when `activates` is `None` per spec Decision 1), then call `dispatch_capability`. The role-string switch goes away.
  - `run_agent_pipeline` evaluates `PipelineEdge.condition` predicates after each agent emits its output. If the predicate is `Some` and matches against the prior agent's `AgentOutput`, the edge fires; if no edge matches and `pipeline.kind == Sequential` (or `Single`), default fall-through to the next AgentRef in strategy order (spec Decision 6 — DAG-strict, no cycles).
  - `EdgePredicate` evaluator at `crates/xvision-engine/src/agent/edge_predicate.rs` — covers all 8 closed variants from Phase A (`Eq/Neq/Gte/Lte/In/All/Any/Not`). Predicates compare against fields in the upstream `AgentOutput` (specifically `FilterSignal.payload` keys for v1). Unknown signal_field → predicate evaluates false (don't fire), no panic.
  - `eval/executor/paper.rs` + `eval/executor/backtest.rs` lifted onto the same `dispatch_capability` seam. The existing role-string code path becomes a thin shim that routes through the new seam (delete it in a future cleanup, not in this PR — keep this PR focused).
  - `strategies/validate.rs` updated to reject strategies whose `PipelineEdge.condition` references a `signal_field` that no upstream Filter could plausibly produce. v1 heuristic: walk the strategy's `agents`, check each upstream agent's `capabilities` for `Capability::Filter`; if no Filter precedes the edge, the predicate is invalid. Edge with `None` condition is always valid.
  - Pre-spec strategies (legacy `trader_slot` / `regime_slot` / `intern_slot` populated; `agents.len() == 0`) continue to work via the legacy dispatch path (spec Decision 8). The new path activates when `agents.len() > 0`.
  - Router capability shipped in v1 (spec Decision 2). `Router` returns `RouteSelection { target_agent_ref_index: usize }`; the pipeline jumps to that AgentRef next, bounded by DAG-strictness (`target_agent_ref_index > current_index`). Spec phase decomposition put Router in Phase B per operator decision.
  - Tests:
    - `agent_graph_dispatch.rs` — round-trip a `kind: Sequential` strategy with 3 capability-typed agents (Trader+Filter+Critic); assert each dispatched via the right handler.
    - `agent_graph_edge_predicate_eval.rs` — 8 variants × at least 2 cases each (match + no-match); legacy `None` condition; unknown signal_field returns false.
    - `agent_graph_router_dispatch.rs` — Router emits `RouteSelection` pointing forward; pipeline jumps as instructed; backward target rejected at validate time.
  - Pre-launch breaking change: no fallback for malformed capability schema. A strategy with an unknown `Capability` variant fails to deserialize via `deny_unknown_fields` (Phase A established this).
---

# Scope

Phase B of `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md` (merged via PR #518). Builds the **dispatcher** that consumes Phase A's schema.

This is the largest of the 5 phases — it replaces the existing role-string-switch in `pipeline.rs` with a capability-typed dispatch seam, lifts both eval executors onto the same seam, and implements the edge predicate evaluator. Trader behavior is byte-identical (Trader is just one of the five capabilities); Filter / Critic / Intern get stub handlers that Phases C-E flesh out. Router ships fully in v1 per operator Decision 2.

After this PR merges, Phase B is the seam every Phase C-F contract bolts onto.

# What lands in this PR

1. **`crates/xvision-engine/src/agent/dispatch_capability.rs`** (NEW) — single entry point `dispatch_capability(slot, capability, briefing, prev_outputs, edges) -> Result<AgentOutput>`. Internal match on `Capability` dispatches to per-kind handlers:
   - `Capability::Trader` → unchanged LLM call → `TraderDecision` (extracted from existing `pipeline.rs` Trader branch)
   - `Capability::Filter` → stub returning `FilterSignal { name: "stub", payload: serde_json::Value::Null, granularity: FilterGranularity::Bar, ts: scenario_now }`. Phase C wires the real Filter LLM call.
   - `Capability::Critic` → stub returning empty `Critique { severity: Info, text: "stub critique" }`. Phase D wires real Critic semantics.
   - `Capability::Intern` → stub returning empty `InternObservation { text: "stub intern" }`. Phase D wires real Intern semantics.
   - `Capability::Router` → real implementation in Phase B: LLM call → JSON `{ "target_agent_ref_index": usize }` → `RouteSelection`. Bounded validation (target > current index, target < agents.len()).
2. **`crates/xvision-engine/src/agent/edge_predicate.rs`** (NEW) — pure evaluator over `(predicate, upstream_output)`. Reads `FilterSignal.payload` (`serde_json::Value`) by `signal_field` key. Unknown field = predicate fails (no panic, no error — just doesn't fire).
3. **`crates/xvision-engine/src/agent/pipeline.rs`** — gut the role-string switch; replace with capability dispatch loop. Resolve `AgentRef.activates` (Phase A field) to a concrete capability per Decision 1 (when `None`, pick the slot's first capability from `capabilities: BTreeSet<Capability>` in iteration order; that's `Trader` for legacy rows since the Phase A default is `{Trader}`). Evaluate edge predicates after each agent emits; on match, route to `RouteSelection.target_agent_ref_index`; otherwise fall through to next AgentRef in strategy order.
4. **`crates/xvision-engine/src/eval/executor/{paper,backtest}.rs`** — lift onto the same seam. The existing role-string code path (`call_trader_slot`, `call_regime_slot`, `call_intern_slot`) becomes a thin shim that builds the legacy `Strategy { trader_slot, regime_slot, intern_slot }` shape into the unified seam input. Pre-spec strategies (legacy fields populated, `agents.len() == 0`) continue to work via the shim.
5. **`crates/xvision-engine/src/strategies/validate.rs`** — predicate validation. If a `PipelineEdge.condition` references a `signal_field` that no upstream Filter (per `agents[0..edge.from].capabilities` containing `Filter`) could produce, the strategy fails `validate_strategy`. Heuristic only — Phase C may add stricter type-checking once Filter signal schemas are real.
6. **`crates/xvision-engine/src/agent/mod.rs`** — `pub mod dispatch_capability;` + `pub mod edge_predicate;`.
7. **Three new test files** covering dispatch, predicate eval, and Router.

# What this PR explicitly does NOT do

- Real Filter LLM dispatch (Phase C)
- Real Critic / Intern semantics (Phase D)
- Unified `Recorder` trait that replaces the per-surface emit paths (Phase D)
- Starter-template `capabilities` retrofit (Phase E)
- UI for capability editor (Phase F)
- Removing the legacy `trader_slot` / `regime_slot` / `intern_slot` fields from `Strategy` (post-v1 cleanup)
- Per-Filter granularity runtime cache (Phase C)
- F-11(f) recorder unification (Phase D)

# Migration

No engine migration. Phase A's `migrate_agent_slot_capabilities` (migration 033) already handles the storage side. This PR is dispatch-runtime only.

# Coordination + sequencing

- **Stacking:** declared on `agent-graph-capability-schema` (PR #527). Phase A must merge before this PR opens.
- **Concurrent peers in `pipeline.rs`:** `memory-aware-eval-findings` (deferred behind `memory-provenance` PR #523) may touch `pipeline.rs` for finding emit. Disjoint regions; coordinate on rebase.
- **Concurrent peers in `eval/executor/{paper,backtest}.rs`:** None today (#516 schema-missing-field merged). If a new contract claims this file, sequential preferred (this PR is large).
- **`memory-provenance-in-decisions-trace` (#523, open):** touches `agent/execute.rs:232` (the recall emit). This PR rewrites the surrounding region in `execute.rs`. Either #523 lands first (preferred — smaller diff) and this PR rebases the recall emit, or vice versa and #523 rebases.
- **`indicator-tool-wiring` (#521, open):** touches `agent/pipeline.rs` (allowed_tools resolution). Must merge before this PR (small change; minimal rebase risk). Mark this PR as `depends_on: indicator-tool-wiring` at dispatch time if #521 still open.

# Hard rules

- Do NOT extend `AgentSlot` (Phase A's territory).
- Do NOT extend `Capability` enum (Phase A locked it).
- Do NOT touch migrations or `xvision-observability/**`.
- Do NOT touch frontend.
- A/B cache pairing: every `dispatch_capability` call must preserve the `(cycle_id, scenario_id)` cache key shape. Cite the test that confirms this (probably in `agent_graph_dispatch.rs` — pin a fixture trader path and assert the same `cycle_id` flows through).
- No try/catch silencing. No `#[allow(...)]` to mute new clippy lints.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
# Wait until PR #527 (Phase A) has merged; this contract is stacked on it.
git worktree add .worktrees/agent-graph-capability-dispatch -b task/agent-graph-capability-dispatch origin/main
```

Set `CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"` before cargo invocations.

# Notes

This contract is **deferred** until Phase A (PR #527) merges. The conductor flips status to `ready` and announces dispatch at that point.

The 4 Phase B-E phase contracts together close out the agent-graph wave. Phase F (UI) is a separate spec authored under `docs/superpowers/specs/`.
