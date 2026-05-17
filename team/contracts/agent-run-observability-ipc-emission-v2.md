---
track: agent-run-observability-ipc-emission-v2
lane: leaf
wave: agent-run-observability-followups
worktree: .worktrees/agent-run-observability-ipc-emission-v2
branch: task/agent-run-observability-ipc-emission-v2
base: task/agent-run-observability-ipc-emission
status: ready
depends_on:
  - agent-run-observability-ipc-emission
blocks:
  - agent-run-observability-sse-stream
  - qa-eval-trace-fidelity
stacking: declared:agent-run-observability-ipc-emission
allowed_paths:
  - xvision-agentd/src/transport/event-client.ts
  - xvision-agentd/src/session/emit.ts
  - xvision-agentd/src/session/build-agent.ts
  - xvision-agentd/src/session/model-wrapper.ts
  - xvision-agentd/src/session/tool-shim.ts
  - xvision-agentd/src/methods/session.ts
  - xvision-agentd/test/**
  - crates/xvision-agent-client/src/event_sink.rs
  - crates/xvision-agent-client/src/client.rs
  - crates/xvision-agent-client/src/protocol.rs
  - crates/xvision-agent-client/tests/event_sink_v2.rs
  - crates/xvision-observability/src/events.rs
  - crates/xvision-observability/src/lib.rs
  - crates/xvision-observability/src/sqlite.rs
forbidden_paths:
  - crates/xvision-observability/src/bus.rs
  - crates/xvision-observability/src/recorder.rs
  - crates/xvision-observability/src/redactor.rs
  - crates/xvision-observability/src/blobs.rs
  - crates/xvision-observability/src/config.rs
  - crates/xvision-observability/src/janitor.rs
  - crates/xvision-observability/src/retention.rs
  - crates/xvision-observability/src/types.rs
  - crates/xvision-observability/src/rows.rs
  - crates/xvision-observability/src/otel.rs
  - crates/xvision-observability/src/export.rs
  - crates/xvision-engine/migrations/**
  - frontend/web/**
  - crates/xvision-cli/**
  - crates/xvision-dashboard/**
interfaces_used:
  - xvision_observability::RunEvent::AssistantTextDelta
  - xvision_observability::RunEvent::ModelCallFinished
  - xvision_observability::RunEvent::ToolCallCancelled
  - xvision_observability::RunEvent::BackpressureDropped
  - xvision_observability::RunEventBus
parallel_safe: false
parallel_conflicts:
  - agent-run-observability-ipc-emission (this stacks on it; rebase forward when parent lands)
  - qa-agentd-budget-enforcement (xvision-agentd/src/methods/session.ts, session/build-agent.ts — same coordination as the parent)
verification:
  - cargo test -p xvision-agent-client
  - cargo test -p xvision-agent-client --test event_sink_v2
  - (cd xvision-agentd && pnpm test)
  - cargo build -p xvision-engine
acceptance:
  - **assistant_text_delta**: sidecar wraps the Cline `AgentModel` with a forwarding wrapper that taps each `text-delta` event from the underlying model stream and emits `event.assistant_text_delta` notifications with span_id, run_id, and `delta_len` (length only — payload stays in memory until the final response is captured by ModelCallFinished). Rust translates to `RunEvent::AssistantTextDelta` and publishes on the bus. SqliteRecorder remains silent (stream-only per Phase A decision); recorder is no-op for this variant.
  - **Per-iteration `ModelCallStarted`**: the AgentModel wrapper emits `event.model_call_started` at the start of each `stream()` invocation with span_id, run_id, provider, model. Dispatch publishes `SpanStarted(ModelCall)`. The existing `event.model_call_finished` notification is upgraded to reference the same `span_id` (currently a synthesized one per step) — the wrapper assigns a span id per `stream()` call and threads it through.
  - **`event.overloaded`** (sidecar side): `event-client.ts` tracks outbound write-buffer depth via `socket.writableLength`. When it crosses a threshold (default 64 KiB or 200 queued bytes — tunable via env `XVISION_EVENT_BUFFER_HIGH_WATER`), emit `event.overloaded` with `run_id` (best-effort from `active-run.ts`), `dropped: 0`, `note: "outbound buffer high"`. When the buffer drains below 50% of threshold, emit a follow-up `event.overloaded` with `dropped: 0`, `note: "outbound buffer cleared"`.
  - **`event.tool_call_cancelled`**: when a tool execution is aborted via the Cline cancellation signal (AbortSignal passed to `execute`), tool-shim emits `event.tool_call_cancelled` with span_id, run_id, reason. Rust translates to `RunEvent::ToolCallCancelled`. If the Cline SDK does not expose an AbortSignal on the execute callback (verify against `@cline/sdk` v0.0.41 surface — fall back to a no-op + comment explaining the dependency on upstream support).
  - Unit tests in event_sink.rs for the 4 new notification kinds (single dispatch case each).
  - Integration test `crates/xvision-agent-client/tests/event_sink_v2.rs` drives all four notification kinds end-to-end via the in-process fake sidecar pattern from `event_sink_smoke.rs`.
  - vitest in xvision-agentd asserts the AgentModel wrapper forwards events correctly and the overload threshold triggers a notification.
---

# Scope

Phase B v1 (PR #224) emitted the essential 5 notification kinds. This
follow-up adds the 4 that v1 explicitly deferred so the recorder and the
trace dock can show streaming text, per-iteration model events, sidecar
backpressure, and tool cancellation.

The new mechanic is the **AgentModel wrapper** — a thin forwarding
`AgentModel` impl that the sidecar's `buildAgent` wraps around the real
provider model. The wrapper passes every request through to the inner
model and republishes each event in the returned async iterable while
also emitting an `event.*` notification. This is the same shape as the
mock provider in `xvision-agentd/src/testing/mock-provider.ts`, so the
pattern is proven against Cline's runtime.

Schema and recorder do not change. Migration 018 already declared
`AssistantTextDelta` / `ToolCallCancelled` / `BackpressureDropped` /
`ModelCallStarted` (the latter is implicit via SpanStarted with
`kind=model.call`). This contract just wires the producer side.

# Out of scope

- SSE consumer for `assistant_text_delta` in the frontend — that is
  `agent-run-observability-sse-stream`. This track lands the event
  emission; the consumer track lands the streaming surface.
- New tables or columns on the recorder — none needed; existing
  `AssistantTextDelta` discards the delta text and writes nothing
  (per Phase A decision), and ToolCallCancelled / overload counters
  surface as `supervisor_notes` warn rows via the existing recorder
  code path.
- Touching `xvision-observability::otel` (`OtelTeeRecorder` already
  subscribes to the bus; new events flow automatically when published).
- The export-cli — `xvn_run.json` totals already aggregate from the
  same tables; new events don't change the export shape.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-run-observability-ipc-emission-v2 status
git -C .worktrees/agent-run-observability-ipc-emission-v2 log --oneline -5 origin/main..HEAD
# Confirm:
#   - branch is task/agent-run-observability-ipc-emission-v2
#   - HEAD descends from task/agent-run-observability-ipc-emission
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-run-observability-ipc-emission-v2 \
  -b task/agent-run-observability-ipc-emission-v2 \
  origin/task/agent-run-observability-ipc-emission
```

# Notes

Append checkpoints / PR links below.
