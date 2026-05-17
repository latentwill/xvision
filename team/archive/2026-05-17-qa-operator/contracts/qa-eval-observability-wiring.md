---
track: qa-eval-observability-wiring
lane: integration
wave: qa-operator-2026-05-17
worktree: .worktrees/qa-eval-observability-wiring
branch: task/qa-eval-observability-wiring
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/observability.rs
  - crates/xvision-engine/src/agent/mod.rs
  - crates/xvision-engine/src/agent/pipeline.rs
  - crates/xvision-engine/src/api/mod.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/api/eval.rs
  - crates/xvision-engine/tests/eval_observability.rs
  - crates/xvision-engine/tests/agent_slot.rs
  - crates/xvision-engine/tests/agent_execute_slot_cap.rs
  - crates/xvision-engine/tests/pipeline_inline.rs
  - crates/xvision-engine/tests/role_normalization.rs
  - crates/xvision-dashboard/src/state.rs
  - crates/xvision-engine/Cargo.toml
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-observability/src/bus.rs
  - crates/xvision-observability/src/sqlite.rs
  - xvision-agentd/**
parallel_safe: false
parallel_conflicts:
  - "qa-eval-trace-fidelity: also edits SpanInspector for model-id display. Coordinate file regions; this track is engine-side only, that track is UI."
verification:
  - cargo build -p xvision-engine
  - cargo test -p xvision-engine agent::execute
  - cargo test -p xvision-engine --test eval_observability
  - cargo test -p xvision-engine
  - cargo test -p xvision-observability
acceptance:
  - "`execute_slot` emits a `SpanStarted` event on the observability bus before each `dispatch.complete()` and a matching `SpanFinished` after — with `status: Error` + `error_json` when the dispatch returns `Err`, `status: Ok` otherwise."
  - "Each successful model call also emits a `ModelCallFinished` carrying provider, model, input/output tokens, and a synthetic prompt hash. Failed calls emit no ModelCallFinished (the SpanFinished with status=Error is sufficient)."
  - "The eval executor passes an `Option<Arc<xvision_observability::RunEventBus>>` through to the pipeline → `execute_slot`. When `None`, emission is a no-op so existing tests keep working unchanged."
  - "An eval run that fails inside an LLM call writes spans to the observability tables under the eval `run_id`, so a future `/api/agent-runs/<eval_run_id>` returns them. (Run-registration in `agent_runs` table is part of this track.)"
  - "Regression test: `tests/eval_observability.rs` exercises a fake-failing `LlmDispatch` and asserts the recorded span has `status='error'` + `error_json` containing the dispatch error message."
  - "Existing eval test suites (`agent::execute::tests`, `pipeline_inline`, `eval::executor::*`) keep passing — no caller signature change beyond an additive `Option` field."
---

# Scope

Closes the producer-side gap identified during `qa-trace-error-surfacing`
(see `team/status/qa-trace-error-surfacing.md`):

> The operator's failing run was an EVAL run, not an agent run. Eval
> runs do not currently flow through `xvision_observability::RunEventBus`
> — `eval/executor/{backtest,paper}.rs` uses a separate
> `xvision_engine::api::chart::RunEventBus` for live chart events. So
> eval errors don't surface in `/agent-runs` traces regardless of UI
> fidelity.

This track wires the observability bus into the eval execution path so
LLM call spans + model-call detail land in the `spans` / `model_calls`
tables, and `SpanInspector` (already error-aware via PR #238) renders
them on the standard agent-run surface.

## Two-half delivery

1. **Engine instrumentation (this track):**
   - New `agent/observability.rs` helper that wraps an
     `Option<Arc<xvision_observability::RunEventBus>>` and exposes
     `emit_span_started` / `emit_span_finished_ok` /
     `emit_span_finished_error` / `emit_model_call_finished`.
   - `SlotInput` gains an additive `obs: Option<Arc<ObsEmitter>>` field.
     Existing callers default to `None`; only the eval executors plumb
     it through.
   - `execute_slot` emits around `dispatch.complete()`. Order:
     `SpanStarted(ModelCall)` → `dispatch.complete()` →
     `SpanFinished(Ok)` + `ModelCallFinished` on success, OR
     `SpanFinished(Error)` with `error_json` on `Err`.
   - `eval/executor/backtest.rs` + `paper.rs` accept the bus via a
     new builder method (`with_observability_bus`) and pass it down
     to the pipeline.
   - `api/eval.rs::run_inner` registers the eval `run_id` in the
     observability `agent_runs` table BEFORE the executor runs, so
     SpanStarted has a valid foreign key.

2. **Run-registry helper (this track):**
   - New `crates/xvision-observability/src/run_registry.rs` with a
     `register_run(pool, run_id, objective, …) -> Result<()>` that
     INSERTs into `agent_runs` and is idempotent on `run_id` conflict.
     Used by the eval executor entry point.

# Out of scope

- Unifying `/eval-runs/:id` and `/agent-runs/:id`. This track makes
  eval spans reachable at `/agent-runs/<eval_run_id>`; the eval-runs
  detail page continues to use its own chart event bus.
- ToolCallStarted/Finished/Failed emission for engine-side tool calls
  invoked by `execute_slot` (`tool_call::invoke`). The current Phase B
  IPC emission covers sidecar tool calls; engine-side tool wrapping
  can be a follow-up if needed.
- Live SSE for eval runs through the observability stream. The bus
  events will be persisted; SSE subscribers join the existing
  observability subscriber path. No new SSE wiring.
- OTEL bridge integration for eval runs. The OTEL feature gate
  remains opt-in; eval observability emits through the standard
  recorder path.
- Modifying migrations. The `spans` + `model_calls` + `agent_runs`
  tables landed by migration 018 already accept these rows.
- Modifying `xvision-observability/src/{bus.rs, sqlite.rs}`. Both are
  conflict-zone single-writers from prior tracks; this track
  consumes their public API only.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-eval-observability-wiring \
  -b task/qa-eval-observability-wiring origin/main
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
git -C .worktrees/qa-eval-observability-wiring status
```

# Notes

Implementation hints:

- `xvision-observability::RunEventBus::emit` is the public publish
  surface; the bus internally fans out to subscribers + recorder.
- The recorder writes `SpanFinished{status: Error, error_json}` to
  `UPDATE spans SET status='error', error_json=? WHERE id=?` — exactly
  what `SpanInspector` from PR #238 surfaces.
- `agent_runs` table requires `run_id`, `objective`, `started_at`,
  `status`, `retention_mode`. The eval flow can supply
  `objective = format!("eval:{scenario_id}")` (or similar) and
  `retention_mode = "hash_only"` for now.
- Keep `obs: Option<...>` on `SlotInput` — most callers (legacy
  pipeline, integration tests) pass `None` and stay silent. Only the
  eval executors opt in.
- `tracing::warn!` after the dispatch error so the existing
  observability path on the test/CLI side still sees something even
  when `obs` is `None`.

# Filed

2026-05-17 by Claude as conductor, following the investigation note
in `team/status/qa-trace-error-surfacing.md`. Operator approved
during the post-#238 review.
