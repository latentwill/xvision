---
track: agent-run-observability-sse-stream
lane: leaf
wave: agent-run-observability-followups
worktree: .worktrees/agent-run-observability-sse-stream
branch: task/agent-run-observability-sse-stream
base: task/agent-run-observability-export-cli
status: ready
depends_on:
  - agent-run-observability-ipc-emission
  - agent-run-observability-export-cli
  - agent-run-observability-ui
  - agent-run-observability-ipc-emission-v2
blocks: []
stacking: declared:agent-run-observability-export-cli
allowed_paths:
  - crates/xvision-dashboard/src/routes/agent_runs.rs
  - crates/xvision-dashboard/src/routes/mod.rs
  - crates/xvision-dashboard/src/server.rs
  - crates/xvision-dashboard/src/state.rs
  - crates/xvision-dashboard/src/sse/**
  - crates/xvision-dashboard/Cargo.toml
  - crates/xvision-dashboard/tests/agent_runs_stream.rs
  - crates/xvision-observability/src/bus_subscriber.rs
  - crates/xvision-observability/src/lib.rs
  - crates/xvision-observability/Cargo.toml
  - frontend/web/src/api/agent-runs.ts
  - frontend/web/src/api/agent-runs.test.ts
  - frontend/web/src/features/agent-runs/SpanInspector.tsx
  - frontend/web/src/features/agent-runs/SpanInspector.test.tsx
  - frontend/web/src/features/agent-runs/RunStatusStrip.tsx
  - frontend/web/src/features/agent-runs/RunStatusStrip.test.tsx
  - frontend/web/src/stores/trace-dock.ts
  - frontend/web/src/stores/trace-dock.test.ts
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-observability/src/sqlite.rs
  - crates/xvision-observability/src/events.rs
  - crates/xvision-observability/src/types.rs
  - crates/xvision-observability/src/rows.rs
  - crates/xvision-observability/src/recorder.rs
  - crates/xvision-observability/src/redactor.rs
  - crates/xvision-observability/src/blobs.rs
  - crates/xvision-observability/src/janitor.rs
  - crates/xvision-observability/src/retention.rs
  - crates/xvision-observability/src/otel.rs
  - crates/xvision-observability/src/export.rs
  - crates/xvision-agent-client/**
  - xvision-agentd/**
interfaces_used:
  - xvision_observability::RunEventBus
  - xvision_observability::RunEvent
  - GET /api/agent-runs/:id (from export-cli)
  - axum::response::sse
parallel_safe: false
parallel_conflicts:
  - agent-run-observability-export-cli (stacks on its routes/agent_runs.rs)
  - agent-run-observability-ui (stacks on its agent-runs.ts mock→real cutover)
verification:
  - cargo test -p xvision-observability
  - cargo test -p xvision-dashboard --test agent_runs_stream
  - (cd frontend/web && pnpm test)
  - (cd frontend/web && pnpm typecheck)
  - (cd frontend/web && pnpm build)
acceptance:
  - **Bus subscriber API** (`crates/xvision-observability/src/bus_subscriber.rs`): the bus exposes a `subscribe_run(run_id) -> BroadcastStream<RunEvent>` helper that returns a tokio broadcast receiver filtered to one run. Implementation: a per-run `tokio::sync::broadcast::Sender<RunEvent>` is registered with the bus's recorder fan-out (as a "subscriber recorder" that forwards events into the broadcast channel). Channel capacity = 256; lagged consumers see a `Lagged` marker.
  - **SSE route** `GET /api/agent-runs/:id/stream` (new) emits `text/event-stream` with one SSE event per `RunEvent`. Event format: `event: <variant_snake_case>\ndata: <serialized RunEvent JSON>\n\n`. Heartbeat: a `: ping\n\n` comment every 15 s so proxies don't time out.
  - On the first SSE event, the route writes a `event: snapshot` block containing the current `xvn.agent_run.v1` JSON snapshot so the consumer has full context, then streams new events as they arrive.
  - SSE route honors the same auth gating as `GET /api/agent-runs/:id`.
  - **Frontend consumer**: `openAgentRunStream` (already present in agent-runs.ts with a mock branch + EventSource skeleton) is upgraded to translate SSE events into typed `AgentRunStreamEvent` updates and route them through `trace-dock` zustand store actions. `assistant_text_delta` events accumulate into a per-span text buffer in the store; SpanInspector renders the buffer when a `model.call` span is selected. `run_finished` / `run_interrupted` close the stream.
  - **RunStatusStrip live updates**: when an SSE stream is open, the strip's status pill reflects in-flight tool/model spans (current span name + elapsed). Existing strip behavior unchanged when the stream is closed.
  - **Backpressure handling**: if the broadcast channel lags, the SSE writer emits an `event: lagged` marker and the client recreates the connection (existing exponential backoff in agent-runs.ts handles reconnect; this just triggers it).
  - **Tests**: 1 dashboard integration test (axum test harness; publish event → SSE client receives it); 3+ frontend tests for the trace-dock SSE event reducer; 1 SpanInspector test asserting accumulated text renders for a selected model.call span.
---

# Scope

End-to-end streaming surface from the bus to the trace dock. PR #224
puts events on the bus, PR #226 exposes the static `GET /api/agent-runs/:id`,
PR #227 wires the UI mock→real cutover with a placeholder
`openAgentRunStream`. This contract closes the loop by adding the SSE
route, the bus→broadcast subscriber that backs it, and the trace-dock
state plumbing that consumes streamed events.

It is the surface the deferred follow-up bullet on `team/board.md`
points at:

> Deferred follow-up (not yet a contract): SSE-driven streaming-event
> surfacing in the trace dock. Was the fourth bullet of the original
> `qa-eval-running-status-streaming` contract; will be filed after
> `agent-run-observability-ipc-emission` lands.

Stacked on #224 / #226 / #227 / ipc-emission-v2 so it can use the real
producer events including `assistant_text_delta`.

# Out of scope

- Producing the events — that is `agent-run-observability-ipc-emission`
  (v1) and `agent-run-observability-ipc-emission-v2` (text deltas + per-
  iteration model events + cancellation + sidecar-side overload).
- Persisting any new tables. The SSE route is read-side only; persistence
  already happens through the Phase A SqliteRecorder on the same bus.
- Re-designing the trace dock — additive only; the strip/dock/inspector
  components stay in place, this track wires data into them.
- OTel — the bus subscriber and SSE route are independent of the OTel
  tee. Both can subscribe simultaneously.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-run-observability-sse-stream status
git -C .worktrees/agent-run-observability-sse-stream log --oneline -5 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-run-observability-sse-stream \
  -b task/agent-run-observability-sse-stream \
  origin/task/agent-run-observability-export-cli
```

After ipc-emission-v2 lands, rebase to pick up `assistant_text_delta`.

# Notes

Append checkpoints / PR links below.
