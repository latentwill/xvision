---
track: agent-run-observability-ipc-emission
lane: foundation
wave: agent-run-observability
worktree: .worktrees/agent-run-observability-ipc-emission
branch: task/agent-run-observability-ipc-emission
base: origin/main
status: ready
depends_on: []
blocks:
  - agent-run-observability-otel-bridge
  - agent-run-observability-export-cli
  - agent-run-observability-ui
stacking: none
allowed_paths:
  - crates/xvision-agent-client/src/**
  - crates/xvision-agent-client/Cargo.toml
  - crates/xvision-agent-client/tests/**
  - crates/xvision-observability/src/events.rs
  - crates/xvision-observability/src/lib.rs
  - crates/xvision-observability/Cargo.toml
  - crates/xvision-observability/tests/ipc_emission_smoke.rs
  - xvision-agentd/src/transport/**
  - xvision-agentd/src/session/**
  - xvision-agentd/src/methods/session.ts
  - xvision-agentd/src/index.ts
  - xvision-agentd/test/**
  - docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md
forbidden_paths:
  - crates/xvision-observability/src/sqlite.rs
  - crates/xvision-observability/src/bus.rs
  - crates/xvision-observability/src/recorder.rs
  - crates/xvision-observability/src/redactor.rs
  - crates/xvision-observability/src/blobs.rs
  - crates/xvision-observability/src/config.rs
  - crates/xvision-observability/src/janitor.rs
  - crates/xvision-observability/src/retention.rs
  - crates/xvision-observability/src/rows.rs
  - crates/xvision-observability/src/types.rs
  - crates/xvision-observability/src/sqlite.rs
  - crates/xvision-engine/migrations/**
  - frontend/web/**
interfaces_used:
  - xvision_observability::RunEvent (all variants)
  - xvision_observability::RunEventBus
  - xvision_observability::SqliteRecorder
  - xvision_agent_client::AgentClient
parallel_safe: false
parallel_conflicts:
  - qa-agentd-budget-enforcement (xvision-agentd/src/session/build-agent.ts, /session/budget.ts, /methods/session.ts — coordinate via team/queue; budget track is P1, this one should rebase onto it once merged)
verification:
  - cargo test -p xvision-agent-client
  - cargo test -p xvision-observability --test ipc_emission_smoke
  - (cd xvision-agentd && pnpm install --frozen-lockfile && pnpm test)
  - cargo build -p xvision-engine
acceptance:
  - Sidecar (xvision-agentd) pushes JSON-RPC notifications back to the Rust client during a session.step call. Notifications: event.model_request, event.model_response, event.tool_call_started, event.tool_call_finished, event.tool_call_failed, event.assistant_text_delta, event.error, event.overloaded.
  - The reverse-notification path is delivered over a new dedicated socket (separate from the callback socket used for synchronous tool.invoke RPC) so request/response routing stays disjoint from notification streaming. Notifications are id-less JSON-RPC 2.0 messages.
  - xvision-agent-client exposes a `RunEventSink` constructor that takes an `Arc<RunEventBus>` and, when set, translates each notification kind 1:1 to a `RunEvent` variant per the plan's IPC table.
  - The `agent_runs` row written on `RunStarted` carries `sidecar_version`, `cline_sdk_version`, `protocol_version` resolved from the IPC handshake recorded in `AgentClient::versions()`.
  - On sidecar crash mid-run (supervisor child exit while a run is open), the client emits `RunInterrupted` for every open `run_id` so the recorder can mark spans `interrupted`.
  - Tool metadata (`tool_version`, `tool_hash`, `side_effect_level`, `is_run_terminator` for the run-terminating `submit_decision` tool) is stamped on the `ToolCallStarted` event published when the IPC handler routes a sidecar→Rust `tool.invoke`.
  - Pipeline-level `RunStarted` / `RunFinished` are emitted by the call-site that drives the agent client (initially the new helper in `xvision-agent-client::run_session`); spec compliance with the existing `xvision-engine::eval::pipeline::run_pipeline` integration is deferred to the eval-engine wave that wires `run_session` in.
  - Smoke test `crates/xvision-observability/tests/ipc_emission_smoke.rs` drives the test mock provider (XVISION_TEST_MOCK_PROVIDER=1) and asserts the bus receives the expected event sequence into a real SqliteRecorder. Final SQLite state: 1 `agent_runs` row with sidecar fingerprint, ≥1 `spans` row, ≥1 `tool_calls` row, 0 dropped events.
  - Backpressure: when the bus subscriber lags, `event.overloaded` notifications surface as `BackpressureDropped` RunEvents and the recorder writes a `supervisor_notes` warn row.
---

# Scope

Wire the IPC notification path so events produced inside the Cline-SDK sidecar
(`xvision-agentd`) become `RunEvent`s on the Phase-A `RunEventBus`, where
the existing `SqliteRecorder` can persist them. This is **Step 8 of the
Cline SDK migration** per
`docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md` §Phase B.

Two halves:

1. **Sidecar (TypeScript)** — add a notification-push surface to
   `xvision-agentd`. The existing transport only answers RPC requests; this
   track introduces a sidecar→client notification path so per-iteration
   model/tool events can be streamed without polling. Hook the Cline Agent
   loop in `xvision-agentd/src/session/build-agent.ts` so each
   model-call / tool-call iteration emits an `event.*` notification before
   the aggregated `session.step` reply.

2. **Rust client (xvision-agent-client)** — a `RunEventSink` wrapper that
   subscribes to the notification socket, translates each notification to a
   `RunEvent`, and publishes to a caller-provided `Arc<RunEventBus>`. The
   sink owns span-id assignment (one per model call / tool call); span
   parents form the agent.run → model.call / tool.call tree per the plan.
   Sidecar fingerprint (`sidecar_version`, `cline_sdk_version`,
   `protocol_version`) is captured from `RuntimeHealthResult` and stamped
   on `RunStarted`.

# Out of scope

- The `OtelTeeRecorder` and the `otel` cargo feature — that is
  `agent-run-observability-otel-bridge`.
- Export CLI (`xvn run inspect`) and HTTP routes — that is
  `agent-run-observability-export-cli`.
- The Run Detail UI route — that is `agent-run-observability-ui`.
- Touching the existing `xvision-engine::eval::pipeline::run_pipeline`
  surface beyond the `RunEventSink` consumer hand-off. Integration into
  the live eval pipeline is the eval-engine wave's job; this track only
  has to expose `run_session` and prove it end-to-end against the mock
  provider.
- Migration 018 — already landed in Phase A.
- The `SqliteRecorder`, `Redactor`, `BlobStore`, `Janitor` — already
  landed; only consumed via existing API.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-run-observability-ipc-emission status
git -C .worktrees/agent-run-observability-ipc-emission log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/agent-run-observability-ipc-emission
#   - base is up to date with origin/main
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-run-observability-ipc-emission \
  -b task/agent-run-observability-ipc-emission origin/main
```

# Notes

Append checkpoints / PR links below.
