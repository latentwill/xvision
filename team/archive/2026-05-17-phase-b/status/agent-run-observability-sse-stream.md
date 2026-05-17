---
track: agent-run-observability-sse-stream
status: pr-ready (backend only — frontend wiring is a follow-up)
worktree: .worktrees/agent-run-observability-sse-stream
branch: task/agent-run-observability-sse-stream
base: task/agent-run-observability-export-cli
---

# Status

**Backend complete and tested.** Frontend wiring is a small follow-up
that must wait for PR #227 (`agent-run-observability-ui`) to merge —
the SSE consumer needs to compose with the mock→real `openAgentRunStream`
upgrade #227 introduces, and this worktree is based on #226 which
predates that surface.

## Shipped

- `crates/xvision-observability/src/bus_subscriber.rs` — `BroadcastSubscriber`
  implementing `AgentRunRecorder`, fans events into per-run
  `tokio::sync::broadcast::Sender` channels (capacity 256, lagged
  consumers see `RecvError::Lagged`). Lazy sender creation so a UI
  client can connect before the producer emits.
- `crates/xvision-dashboard/src/sse/mod.rs` — `agent_run_sse(snapshot, rx)`
  builds an axum `Sse<...>` response. Wire format: `snapshot` event
  first with the full `xvn.agent_run.v1` payload, then one event per
  `RunEvent` variant (snake_case name + JSON data), `lagged` marker on
  `RecvError::Lagged(n)`, KeepAlive `: keep-alive` comment every 15 s.
  Terminates on `RunFinished` / `RunInterrupted` / `Closed`.
- `crates/xvision-dashboard/src/state.rs` + `server.rs` — broadcast
  subscriber wired into `AppState` alongside the canonical
  `SqliteRecorder` on a shared `RunEventBus`.
- `crates/xvision-dashboard/src/routes/agent_runs.rs` — `GET
  /api/agent-runs/:id/stream` handler. 404 for unknown run_id; same
  auth gating as the existing `GET /api/agent-runs/:id`.
- `crates/xvision-dashboard/tests/agent_runs_stream.rs` — 2 axum-test
  integration tests: snapshot + live event round-trip, 404 for unknown
  run.

## Deferred to follow-up (post-#227 merge)

- `frontend/web/src/api/agent-runs.ts` — upgrade `openAgentRunStream`
  real branch to map SSE event names to typed events and route through
  to the trace dock store. Wait for #227 to land first so the mock
  branch + env-flag surface lives upstream.
- `frontend/web/src/stores/trace-dock.ts` — add streaming reducers
  (`appendDelta`, `markSpanActive`, `recordLag`).
- `frontend/web/src/features/agent-runs/SpanInspector.tsx` — "Streaming
  response..." indicator with delta-len counter on selected active
  model.call spans.
- `frontend/web/src/features/agent-runs/RunStatusStrip.tsx` — live in-
  flight span display when a stream is open.
- Frontend tests for the above.

These can ship as a single follow-up PR (~150 lines) once #227 is on
main. The wire protocol and snapshot shape are stable, so the consumer
work has no remaining backend dependency.

## Verification (backend, all green)

- `cargo test -p xvision-observability` (existing tests + new
  `bus_subscriber` units)
- `cargo test -p xvision-dashboard --test agent_runs_stream` (2 tests)
