# xvn Trace Observability Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the redesigned trace the primary operator surface for ClineSDK-backed xvision runs, so observability rows, replay metadata, and lossless trajectory data are visible together instead of living in separate places.

**Architecture:** Use the existing SQLite agent-run ledger and trajectory store as canonical sources, then project them into the trace dock as one correlated view. The summary strip should answer "what mode is this run in, how healthy is it, and what went wrong?" while the inspector should answer "what exactly happened at this span / tool / frame?" This is a trace projection problem, not a new runtime problem.

**Tech Stack:** Rust, TypeScript, React, SQLite, SSE, JSON export, existing trace-dock store.

---

### Task 1: Extend the backend export / summary payload for trace use

**Files:**
- Modify: `crates/xvision-observability/src/export.rs`
- Modify: `crates/xvision-engine/src/agent/observability.rs`
- Modify: `crates/xvision-observability/src/events.rs`
- Modify: `crates/xvision-observability/src/types.rs`
- Modify: `frontend/web/src/api/types-agent-runs.ts`
- Modify: `frontend/web/src/api/agent-runs.ts`

- [ ] **Step 1: Verify the export shape carries trace-critical fields**

Ensure the run export includes the fields needed by the redesigned trace:
- trajectory_mode
- retention_mode
- replay_hit_ratio
- dropped_events
- recovery_reason
- sidecar_version
- cline_sdk_version
- protocol_version

- [ ] **Step 2: Preserve the fidelity split between summary and deep data**

The export should expose enough to render the trace, but the deep payloads should still come from the proper artifact / blob path.

- [ ] **Step 3: Add correlation-friendly identifiers where the UI needs them**

If any identifier is missing from the summary payload, add it here instead of forcing the frontend to guess.

- [ ] **Step 4: Keep the observability ledger honest about terminal states**

The trace should distinguish:
- completed
- error
- interrupted
- replay divergence
- replay frames exhausted

Do not collapse these into a single "finished" bucket in the exported data.

### Task 2: Project observability into the redesigned trace dock

**Files:**
- Modify: `frontend/web/src/features/agent-runs/TraceDock.tsx`
- Modify: `frontend/web/src/features/agent-runs/SpanInspector.tsx`
- Modify: `frontend/web/src/features/agent-runs/RunStatusStrip.tsx`
- Modify: `frontend/web/src/features/agent-runs/TrajectoryModeBadge.tsx`
- Modify: `frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.tsx`
- Modify: `frontend/web/src/features/agent-runs/FlameGraph.tsx`
- Modify: `frontend/web/src/features/agent-runs/PullQuote.tsx`
- Modify: `frontend/web/src/features/agent-runs/TraceDownloadButton.tsx`

- [ ] **Step 1: Make the trace strip explain the run state at a glance**

The status strip should show:
- current trajectory mode
- replay hit ratio when in replay
- dropped events when non-zero
- recovery reason when present
- retention mode when relevant

- [ ] **Step 2: Make the inspector show the observability identity**

The inspector should expose enough metadata to connect the span to the ledger:
- run_id
- span_id
- tool_call_id when present
- slot_role / step / frame coordinates when relevant
- provider / model fingerprint
- terminal status and recovery reason

- [ ] **Step 3: Use the redesigned trace to surface the missing signal classes**

Add visible slots for the things the report said were lossy or absent:
- reasoning deltas
- cache token counts
- retry / failover markers
- submit_decision payload links
- abort / timeout semantics

- [ ] **Step 4: Keep the UI honest about what is body text vs what is hash-only**

The trace should clearly label whether a payload is:
- fully retained
- redacted
- hash-only
- unavailable because of mode / retention

- [ ] **Step 5: Keep the export button aligned with the trace view**

If the trace dock can show it, the exported JSON should contain the same semantic labels and identifiers.

### Task 3: Reconcile trajectory frames with RunEvents in the view model

**Files:**
- Modify: `frontend/web/src/features/agent-runs/SpanInspector.tsx`
- Modify: `frontend/web/src/features/agent-runs/TraceDock.tsx`
- Modify: `frontend/web/src/features/agent-runs/mock-fixtures.ts`
- Modify: `frontend/web/src/features/agent-runs/*.test.tsx`
- Modify: `crates/xvision-agent-client/src/event_sink.rs`
- Modify: `crates/xvision-engine/src/agent/cline_recording.rs`

- [ ] **Step 1: Add a correlation view for trajectory vs observability**

Show the operator when a span has:
- a matching recorded frame
- a matching export row
- a missing frame
- a recording failure
- a replay divergence

- [ ] **Step 2: Add explicit drift states instead of silent mismatch**

If the observability ledger and the trajectory store disagree, surface it as a visible state in the trace rather than hiding it in logs.

- [ ] **Step 3: Normalize terminal states into user-facing labels**

Use the same naming everywhere in the trace dock, export JSON, and badges.

- [ ] **Step 4: Keep the correlation logic keyed on stable identities**

Do not let the UI rely on display names alone. Use the stable IDs and coordinates from the backend model.

### Task 4: Add regression coverage for the redesigned trace

**Files:**
- Modify: `frontend/web/src/features/agent-runs/SpanInspector.test.tsx`
- Modify: `frontend/web/src/features/agent-runs/TraceDownloadButton.test.tsx`
- Modify: `frontend/web/src/features/agent-runs/TrajectoryModeBadge.test.tsx`
- Modify: `frontend/web/src/features/agent-runs/RunStatusStrip.test.tsx`
- Modify: `frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.test.tsx`
- Modify: `frontend/web/src/features/agent-runs/FlameGraph.test.tsx`
- Modify: `crates/xvision-agent-client/src/event_sink.rs` tests
- Modify: `crates/xvision-observability/src/export.rs` tests

- [ ] **Step 1: Cover the mode chips and summary fields**

Validate live / record / replay rendering, replay hit ratio, dropped events, and recovery reason.

- [ ] **Step 2: Cover the fidelity labels**

Validate that full-debug, redacted, and hash-only surfaces are not conflated.

- [ ] **Step 3: Cover the trace drift states**

Add test fixtures for:
- successful replay
- replay divergence
- exhausted replay frames
- recording failure
- error-terminal run

- [ ] **Step 4: Cover the high-value payload surfaces**

Make sure the operator can inspect at least the following from the redesigned trace:
- decision payload links
- provider / model fingerprint
- run terminal outcome
- span-to-frame correlation

- [ ] **Step 5: Run the frontend and Rust tests that protect the trace path**

Use the existing unit tests for the dock plus the relevant Rust tests for export / event parsing.
