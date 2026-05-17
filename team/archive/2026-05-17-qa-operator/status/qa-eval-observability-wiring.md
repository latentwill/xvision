---
track: qa-eval-observability-wiring
status: pr-open
last_update: 2026-05-17
worker: Claude (xvision conductor session)
pr: 242
commits:
  - 90c29e1 — qa: wire engine eval path through observability bus
---

## Outcome

PR #242 open. Branch pushed. Closes the producer-side gap noted in
`team/status/qa-trace-error-surfacing.md`.

## What shipped

- New `crates/xvision-engine/src/agent/observability.rs` — `ObsEmitter`
  wrapper over `xvision_observability::RunEventBus`.
- `SlotInput.obs` additive Option; `execute_slot` brackets every
  `dispatch.complete()` with SpanStarted / SpanFinished{Ok|Error}
  + ModelCallFinished. Error path wraps `anyhow::Error.to_string()`
  in the recorder's `{message:...}` shape so PR #238's
  `SpanInspector.parseErrorJson` extracts it verbatim.
- `PipelineInputs.obs` threads through `run_pipeline` / `run_agent_pipeline`.
- `BacktestExecutor::with_observability` / `PaperExecutor::with_observability`
  builders.
- `ApiContext.with_obs_event_bus` builder; `AppState::api_context`
  injects the dashboard's singleton bus.
- `api/eval.rs::run_inner` (both entry points) builds an `ObsEmitter`
  bound to `run.id`, emits `RunStarted` so the recorder registers the
  run, threads the emitter into executor builders, emits
  `RunFinished{Completed|Failed}` on terminal.

## Verification

| Command | Result |
|---|---|
| `cargo build -p xvision-engine` | clean |
| `cargo build -p xvision-dashboard` | clean |
| `cargo test -p xvision-engine --test eval_observability` | 2/2 pass |
| `cargo test -p xvision-engine --lib agent::` | 14/14 pass |
| `cargo test -p xvision-engine --test pipeline_inline --test agent_slot --test agent_execute_slot_cap --test role_normalization` | 22/22 pass |
| `cargo test -p xvision-observability` | pass (incl. doc-tests) |

## Path drift handled

- Added `crates/xvision-engine/Cargo.toml` to allowed_paths so the
  `xvision-observability` workspace dep could be wired in.
- Added the test files (`agent_slot.rs`, `agent_execute_slot_cap.rs`,
  `pipeline_inline.rs`, `role_normalization.rs`) to allowed_paths
  — each needed a one-line `obs: None` field addition on every
  `SlotInput { ... }` / `PipelineInputs { ... }` literal.

## Out of scope (per contract)

- Unifying `/eval-runs/:id` and `/agent-runs/:id`. Eval spans are
  now reachable at `/agent-runs/<eval_run_id>`, but the eval-runs
  detail page still uses its own chart event bus.
- Engine-side tool-call wrapping (`tool_call::invoke` in
  `execute_slot`). Phase B IPC emission covers sidecar tool calls;
  engine-side tool wrapping is a future follow-up.
- Live SSE for eval runs through the observability stream — events
  are persisted, SSE subscribers piggyback on the existing
  observability subscriber path. No new SSE wiring.
- OTEL bridge integration. OTEL stays opt-in via cargo feature `otel`.
- Migrations or `xvision-observability/src/{bus.rs, sqlite.rs}`
  changes. Both are conflict-zone single-writers; this track
  consumes their public API only.

## Operator-visible outcome

Once #238 and #242 are both merged, an eval LLM call that fails
inside `dispatch.complete()` will:

1. Persist a `spans` row with `status='error'` and `error_json`
   carrying `{"message":"[unclassified] error decoding response body: ..."}`.
2. Surface at `/api/agent-runs/<eval_run_id>` with the eval run id
   as the trace identifier.
3. Render in the trace dock's `SpanInspector` with a red `ERROR`
   badge in the header strip and the full error message as a
   pull-quote at the top of the body.

That's the operator's original "errors need to be in trace so we can
debug" complaint, end-to-end.
