---
track: agent-graph-filter-capability
lane: foundation
wave: agent-graph-2026-05-22
worktree: .worktrees/agent-graph-filter-capability
branch: task/agent-graph-filter-capability
base: origin/main
status: deferred
depends_on:
  - agent-graph-capability-schema    # PR #527 — Phase A
  - agent-graph-capability-dispatch  # PR #534 contract / future PR — Phase B
blocks:
  - agent-graph-template-capabilities  # Phase E references signal field naming
stacking: declared:agent-graph-capability-dispatch
allowed_paths:
  - crates/xvision-engine/src/agent/dispatch_capability.rs
  - crates/xvision-engine/src/agent/filter_dispatch.rs
  - crates/xvision-engine/src/agent/signal_cache.rs
  - crates/xvision-engine/src/agent/pipeline.rs
  - crates/xvision-engine/src/agents/capability.rs   # FilterSignal payload helpers only — NOT enum variants (those are Phase A)
  - crates/xvision-engine/src/strategies/validate.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-filters/src/runtime.rs
  - crates/xvision-filters/src/lib.rs
  - crates/xvision-engine/tests/agent_graph_filter_dispatch.rs
  - crates/xvision-engine/tests/agent_graph_filter_granularity.rs
  - crates/xvision-engine/tests/agent_graph_filter_multi_signal.rs
  - crates/xvision-engine/tests/agent_graph_filter_dsl_bridge.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**           # no new migration
  - crates/xvision-engine/src/agents/model.rs     # Phase A owns
  - crates/xvision-engine/src/strategies/agent_ref.rs  # Phase A owns
  - crates/xvision-observability/**               # Phase D owns recorder unification
  - frontend/web/**                               # Phase F owns UI
interfaces_used:
  - xvision_engine::agents::Capability::Filter (Phase A)
  - xvision_engine::strategies::agent_ref::AgentRef { activates, granularity } (Phase A)
  - xvision_engine::strategies::agent_ref::PipelineEdge { condition } (Phase A)
  - xvision_engine::agent::dispatch_capability::{dispatch_capability, AgentOutput, FilterSignal, FilterGranularity} (Phase B)
  - xvision_engine::agent::edge_predicate::evaluate_predicate (Phase B)
  - xvision_engine::agent::llm::LlmDispatch (existing)
  - xvision_filters::runtime::{RuntimeFilter, ActivationDecision} (existing DSL filters substrate)
parallel_safe: false
parallel_conflicts:
  - agent-graph-capability-dispatch   # touches pipeline.rs + dispatch_capability.rs — Phase B must merge first
  - agent-graph-unified-recorder      # Phase D touches dispatch_capability.rs to emit per-capability spans
verification:
  - cargo fmt --check
  - cargo clippy --workspace --tests -- -D warnings
  - cargo test -p xvision-engine --test agent_graph_filter_dispatch
  - cargo test -p xvision-engine --test agent_graph_filter_granularity
  - cargo test -p xvision-engine --test agent_graph_filter_multi_signal
  - cargo test -p xvision-engine --test agent_graph_filter_dsl_bridge
  - cargo test -p xvision-engine
  - cargo test -p xvision-filters
  - cargo build --workspace
acceptance:
  - **LLM Filter dispatcher** at `crates/xvision-engine/src/agent/filter_dispatch.rs`. Given an `AgentSlot` with `Capability::Filter` and a briefing, wraps the slot's `system_prompt` with an output-schema constraint that forces the model to return `FilterSignal`-shaped JSON `{ name, payload, granularity }`. Parse + validate via `serde_json` + `deny_unknown_fields`. Malformed output → record a `FilterParseError` event and propagate `None` to the signal map (downstream edges evaluate against missing signal — predicate returns false, edges do not fire).
  - **Filter dispatcher wired into Phase B's `dispatch_capability`**. The `AgentOutput::Filter(FilterSignal)` branch in `dispatch_capability` now calls `filter_dispatch::run_llm_filter(slot, briefing) -> Result<FilterSignal>` instead of the Phase B stub. Trader / Critic / Intern / Router branches unchanged.
  - **`FilterGranularity` runtime** at `crates/xvision-engine/src/agent/signal_cache.rs`. Per-eval-run in-memory cache keyed by `(strategy_id, agent_ref_role)` → `(FilterSignal, last_evaluated_ts)`. On each cycle:
    - `Bar`: re-evaluate every new bar (no cache lookup).
    - `Minute`: re-evaluate when `now.truncate(60s) > cached.ts.truncate(60s)`; otherwise re-fire cached signal into downstream briefings.
    - `Decision`: re-evaluate only when a downstream Trader is about to be invoked (driven by edge graph topology — the dispatcher walks the graph forward, identifies which Filters feed any reachable Trader, and re-evaluates those before invoking the Trader). Other Filters re-fire their cached signal.
  - **Signal cache lifetime: in-memory, single eval-run only** (operator Q5 resolution 2026-05-22). No SQLite persistence. Cache is owned by the executor for the duration of the run; dropped on completion. Live trading scenarios will rebuild the cache from cycle 1.
  - **Granularity fallback**: if `AgentRef.granularity == Minute` and the scenario's bar period is `> 1 minute`, the runtime emits a `granularity_fallback` event (via `ObsEmitter::event`) carrying `{ requested: "minute", effective: "bar", reason: "bar_period_exceeds_granularity" }` and degrades to `Bar`. No silent behavior change — the trace shows the demotion.
  - **Multi-Filter cardinality**: configurable via `[engine] multi_fire_bar_threshold_minutes` in `xvn.toml` (default 30, operator Q3 resolution 2026-05-22).
    - Bars with `period_minutes < threshold` (short bars, e.g. 5m / 15m): all Filter signals from the cycle are coalesced into a single downstream Trader briefing. The Trader sees `filter_signals: { regime_filter: …, vol_filter: … }` in one invocation. **Default behavior.**
    - Bars with `period_minutes >= threshold` (long bars, e.g. 30m / 1h / 1d): the Trader is invoked once per emitting Filter, with `filter_signals` containing the producing signal only. Order: Filter-ref order in `Strategy.agents`. The cycle's recorded `TraderDecision` is the **last** invocation's output (matches Phase B's "last AgentRef of `activates: Trader` wins" rule).
    - Threshold is read from the scenario's `BacktestConfig` (or live config) at executor startup; passed into `dispatch_capability` as a config field. Not per-strategy.
  - **DSL filter bridge** at `crates/xvision-filters/src/runtime.rs`. `RuntimeFilter::evaluate(ctx) -> ActivationDecision` is wrapped by a thin adapter `dsl_to_filter_signal(filter_id, decision) -> FilterSignal` exported from `xvision-filters`. The adapter sets:
    - `name = filter_id`
    - `payload = { "active": <bool>, "reason": <string?> }`  ← stable schema for edge predicates
    - `granularity = FilterGranularity::Bar` (DSL filters are always bar-cadence today)
    A strategy that declares a DSL-backed AgentRef (slot whose `provider == "dsl"` or similar marker; existing field) routes through the bridge instead of `filter_dispatch::run_llm_filter`. Edge predicates work identically on both — they operate on the `FilterSignal.payload` JSON regardless of producer.
  - **`strategies/validate.rs` extended**: `PipelineEdge.condition` predicate referencing a `signal_field` that no upstream Filter declares in its `system_prompt`'s output-schema is a validation warning (not error — the schema can change between runs). Existing "no upstream Filter" check from Phase B remains an error.
  - **Tests**:
    - `agent_graph_filter_dispatch.rs` — a strategy with one LLM Filter slot + one Trader; assert the Filter produces a `FilterSignal`, the Trader sees it under `filter_signals[<role>]`, and an edge predicate `Eq` on the payload gates Trader invocation correctly.
    - `agent_graph_filter_granularity.rs` — three sub-tests:
      - `Bar` granularity re-evaluates every cycle.
      - `Minute` granularity caches within the same minute, re-evaluates on next minute.
      - `Decision` granularity re-evaluates only when a Trader is reachable downstream.
      - A `Minute`-granularity Filter on a 5-minute-bar scenario emits `granularity_fallback` and degrades to Bar.
    - `agent_graph_filter_multi_signal.rs` — two-Filter cycle, with sub-tests for both threshold regimes:
      - 5m bar (below default 30m threshold): both signals coalesce; Trader runs once with `filter_signals.len() == 2`.
      - 1h bar (above default 30m threshold): Trader runs twice; second invocation's TraderDecision is the recorded one; both are emitted on the trace.
      - Same fixture with `multi_fire_bar_threshold_minutes = 0` in config: forces multi-fire on the 5m bar too. Confirms the knob.
    - `agent_graph_filter_dsl_bridge.rs` — a DSL-backed Filter + an LLM Trader; assert the bridge emits a `FilterSignal { payload: { active: bool } }` and that edge predicates `Eq` on `payload.active` gate the trader correctly. Confirms LLM/DSL parity.
  - **Pre-launch breaking change**: legacy strategies with `filter_slot` populated (pre-2026-05-12) continue to route through the legacy dispatcher (Phase B Decision 8 fallback). The new Filter capability path activates only when `agents.len() > 0` AND at least one AgentRef has `activates: Capability::Filter`.
  - No new migration. Cache state is in-memory only. Config field `multi_fire_bar_threshold_minutes` lands on `xvision_engine::config::EngineConfig` (existing struct) with `default = 30`.

---

# Scope

Phase C of `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md`. Phase A (PR #527) defines the `Capability::Filter` enum variant and the `FilterSignal` struct shape; Phase B (PR-deferred) builds the dispatch seam with a stub for Filter; **Phase C makes Filter real**:

1. The LLM Filter dispatcher actually invokes the model with a schema-constrained prompt.
2. `FilterGranularity` runtime semantics ship (Bar / Minute / Decision + granularity_fallback).
3. The in-memory signal cache implements the spec's "re-fire cached signal into downstream briefings" behavior for Minute + Decision granularities.
4. The DSL filter bridge so existing `xvision-filters` DSL strategies route through the same edge-predicate substrate as LLM Filters.
5. Multi-Filter cardinality per operator Q3 resolution: threshold-gated coalesce-vs-multi-fire.

# Out of scope

- Filter-conditional Critic invocation. Critic in v1 is observation-only (spec Decision 4; operator confirmed).
- Edge-predicate authoring UI. Phase F deferred indefinitely.
- Persistent signal cache. Operator Q5 resolution: in-memory only.
- Filter-to-Filter signal chaining. Spec lock: Filters do not consume each other's output ("Per-capability I/O contracts — Filter — Consumes" section in spec).
- Phase D recorder symmetry. Phase D owns `crates/xvision-observability/**`.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
# Wait for Phase A (#527) AND Phase B (TBD PR) to merge into origin/main.
git worktree add .worktrees/agent-graph-filter-capability \
  -b task/agent-graph-filter-capability origin/main
```

Set per-worktree target dir to avoid colliding with other parallel agents:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-filter-capability"
```

# Iterative verification loop

```bash
cargo test -p xvision-engine --test agent_graph_filter_dispatch       2>&1 | tee /tmp/filter-dispatch.log
cargo test -p xvision-engine --test agent_graph_filter_granularity    2>&1 | tee /tmp/filter-granularity.log
cargo test -p xvision-engine --test agent_graph_filter_multi_signal   2>&1 | tee /tmp/filter-multi-signal.log
cargo test -p xvision-engine --test agent_graph_filter_dsl_bridge     2>&1 | tee /tmp/filter-dsl-bridge.log
cargo test -p xvision-engine
cargo test -p xvision-filters
cargo build --workspace
cargo clippy --workspace --tests -- -D warnings
```

# Notes

- The signal cache lives on the executor, not on the strategy or the dispatcher. Each call to `eval/executor::run()` constructs a fresh `SignalCache` and passes it into the per-cycle dispatch loop. The cache is dropped when the run completes.
- The granularity_fallback event is the single point where the dispatcher silently degrades behavior; that's why it MUST emit an observable event. Tests assert the event is recorded.
- Multi-Filter cardinality is one knob, one default. The operator resolution favors short-bar coalesce as the default because short bars trade frequently and re-running the Trader on each Filter signal would dominate token budget. Long bars (intraday H/D) trade rarely; running the Trader per Filter is acceptable. The knob exists for operators who want strict per-Filter invocation on short bars (model-bakeoff scenarios may want it).
- The DSL bridge keeps `xvision-filters` autonomous — that crate still exposes `RuntimeFilter` as before; Phase C adds one helper function `dsl_to_filter_signal()` and `xvision-engine` calls it from the dispatcher when the slot is DSL-backed.
- Edge predicate evaluation is Phase B's responsibility (it ships in Phase B). Phase C only adds the signal-field validation warning in `validate.rs`.
