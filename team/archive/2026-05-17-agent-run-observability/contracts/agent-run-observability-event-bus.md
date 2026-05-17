---
track: agent-run-observability-event-bus
lane: foundation
wave: agent-run-observability
worktree: .worktrees/agent-run-observability-event-bus
branch: task/agent-run-observability-event-bus
base: origin/main
status: ready
depends_on:
  - agent-run-observability-schema
blocks:
  - agent-run-observability-ipc-emission
  - agent-run-observability-otel-bridge
stacking: none
allowed_paths:
  - crates/xvision-observability/src/bus.rs
  - crates/xvision-observability/src/recorder.rs
  - crates/xvision-observability/src/sqlite.rs
  - crates/xvision-observability/src/noop.rs
  - crates/xvision-observability/src/lib.rs
  - crates/xvision-observability/tests/**
forbidden_paths:
  - crates/xvision-engine/src/agent/**
  - crates/xvision-agent-client/**
  - xvision-agentd/**
  - frontend/web/src/**
  - crates/xvision-engine/migrations/**
interfaces_used:
  - Row types and helpers from `agent-run-observability-schema`
  - `tokio` mpsc / broadcast (workspace dep already)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo build -p xvision-observability
  - cargo test -p xvision-observability
  - cargo test -p xvision-observability --test event_bus_synthetic
acceptance:
  - `RunEventBus` published in `xvision-observability` with bounded capacity (default 4096, configurable), MPSC ingestion, fan-out to N subscribers.
  - `RunEvent` enum covers every variant the plan's emission-point table maps to (RunStarted, RunFinished, SpanStarted, SpanFinished, ModelCallFinished, ToolCallStarted, ToolCallFinished, ToolCallFailed, ToolCallCancelled, AssistantTextDelta, CheckpointWritten, SidecarError, BackpressureDropped, SupervisorNote, ArtifactWritten, RunInterrupted).
  - `AgentRunRecorder` trait matches the plan's recorder API (start_run, finish_run, start_span, finish_span, record_model_call, record_tool_call, record_approval, record_sandbox_result, record_checkpoint, record_supervisor_note, record_artifact, mark_interrupted).
  - **Attribute API guardrail enforced:** the recorder trait does NOT accept `&str` payloads as attributes — only hashes, counts, ids. A `#[deny(clippy::…)]` or compile_fail test demonstrates this.
  - `SqliteRecorder` subscribed to the bus, writes the rows defined by the schema crate. FIFO ordering preserved per `run_id`.
  - `NoopRecorder` for tests / off-mode.
  - Bus overflow drops the oldest event, increments a per-run drop counter, and emits a `BackpressureDropped` event so a downstream `supervisor_notes` row records the gap.
  - Synthetic-event integration test that publishes a representative event stream (1 run, 3 model calls, 5 tool calls, 1 interrupted span) and verifies the SQLite rows match expectations.
---

# Scope

Phase A leaf #2 of the agent-run-observability wave. Builds on the schema
crate. No emission producers yet — that's the IPC-emission leaf in Phase B.

Deliverables:

- `RunEventBus` (bounded, async, drop-counted) and the `RunEvent` enum.
- `AgentRunRecorder` trait with the attribute-API guardrail.
- `SqliteRecorder` + `NoopRecorder` subscribed to the bus.
- Synthetic-event integration test.

# Out of scope

- `OtelTeeRecorder` and `tracing-opentelemetry` plumbing — that's `agent-run-observability-otel-bridge` (Phase B).
- Producing events from a real IPC handler — that's `agent-run-observability-ipc-emission` (Phase B).
- CLI / dashboard surfaces.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/agent-run-observability-event-bus \
  -b task/agent-run-observability-event-bus origin/main
```

# Notes

- Per-`run_id` FIFO ordering is required. Cross-run ordering is best-effort.
- The bus runs over `tokio::sync::mpsc` to a single consumer task that
  fans out to subscribers. Avoid `broadcast` here — late subscribers would
  see only forward-going events, which complicates testing.
- The synthetic-event test should be the contract for downstream Phase B
  emission leaves: if their handlers produce these events, this leaf's
  recorder writes the expected rows.
- Default bus capacity (4096) is a guess. The plan calls out that the bus
  is the backpressure boundary; tune via `observability.toml` after
  `agent-run-observability-ipc-emission` lands and we have real
  throughput data.
