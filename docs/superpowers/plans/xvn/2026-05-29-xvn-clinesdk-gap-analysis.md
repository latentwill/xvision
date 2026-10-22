# xvn ClineSDK Gap Analysis Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Compare the ClineSDK research report against the current xvision codebase, separate already-completed plumbing from real gaps, and turn the remaining gaps into a focused trace/observability plan.

**Architecture:** xvision already has the important runtime plumbing: the event sink, trajectory recording path, export endpoints, and trace-dock chrome. The remaining problem is not "can we record anything"; it is "can the redesigned trace make the observability and trajectory data legible, correlated, and reviewable without inventing a second source of truth." This plan treats the SQLite agent-run ledger and trajectory store as canonical, then maps them into the trace UI.

**Tech Stack:** Rust, TypeScript, React, SQLite, SSE, JSON export, existing agent-run observability schema.

---

### Task 1: Reconcile the report with current xvision reality

**Files:**
- Read: `docs/superpowers/notes/2026-05-25-clinesdk-trace-alignment-audit.md`
- Read: `crates/xvision-agent-client/src/event_sink.rs`
- Read: `crates/xvision-agent-client/src/client.rs`
- Read: `crates/xvision-engine/src/agent/cline_recording.rs`
- Read: `frontend/web/src/features/agent-runs/TrajectoryModeBadge.tsx`
- Read: `frontend/web/src/features/agent-runs/TraceDownloadButton.tsx`
- Read: `frontend/web/src/features/agent-runs/SpanInspector.tsx`
- Read: `frontend/web/src/features/agent-runs/RunStatusStrip.tsx`

- [ ] **Step 1: Build a closed/open matrix for the report claims**

Classify each report gap as one of:
- already closed in xvision today
- partially closed, but still missing trace projection
- still open and requires code changes

Use the code paths above as the evidence base.

- [ ] **Step 2: Identify the real operator-facing gap**

Write down the one-sentence gap statement in repo terms:
- observability exists,
- trajectory recording exists,
- but the redesigned trace does not yet project them into a single reviewable operator experience.

- [ ] **Step 3: Name the current false positives**

Call out report assumptions that are stale now, especially:
- trajectory recording being "missing" end-to-end
- trace-mode metadata being absent
- export/download plumbing being absent

- [ ] **Step 4: Name the remaining open gaps**

Keep only the gaps that still matter after the current implementation is accounted for:
- trace correlation between RunEvent rows and trajectory frames
- terminal-state normalization for error / abort / replay divergence
- reasoning / cache / retry visibility in the trace view
- submit_decision / artifact visibility from the operator surface

- [ ] **Step 5: Produce the implementation handoff note**

Write a short summary that points the execution plan toward trace observability rather than more runtime plumbing.

### Task 2: Define the xvn trace-observability boundary

**Files:**
- Read: `crates/xvision-observability/src/export.rs`
- Read: `crates/xvision-observability/src/events.rs`
- Read: `crates/xvision-observability/src/types.rs`
- Read: `crates/xvision-engine/src/agent/observability.rs`
- Read: `frontend/web/src/api/types-agent-runs.ts`
- Read: `frontend/web/src/features/agent-runs/mock-fixtures.ts`

- [ ] **Step 1: Define the canonical trace summary fields**

Make sure the summary model carries the fields operators need to judge run health at a glance:
- trajectory_mode
- retention_mode
- replay_hit_ratio
- dropped_events
- recovery_reason
- sidecar_version
- cline_sdk_version
- protocol_version

- [ ] **Step 2: Define the correlation keys**

Write down the keys that let the UI correlate the two worlds without ambiguity:
- run_id
- span_id
- tool_call_id
- slot_role
- step_index
- frame_index
- recording_id when applicable

- [ ] **Step 3: Define the fidelity split**

Document which data belongs in:
- the lossy observability ledger
- the lossless trajectory export
- the trace dock summary
- the deep inspector

This is the core design decision the new trace must not blur.

- [ ] **Step 4: Record the known lossy areas explicitly**

Keep the known fidelity gaps visible in the plan:
- reasoning deltas
- cache token accounting
- retry / failover markers
- decision payloads
- terminal abort semantics

### Task 3: Select the execution track for the redesigned trace

**Files:**
- Create: `docs/superpowers/plans/xvn/2026-05-29-xvn-trace-observability-plan.md`
- Optional follow-up: `docs/superpowers/plans/xvn/2026-05-29-xvn-trace-observability-fixtures.md`

- [ ] **Step 1: Decide which gaps are backend vs UI**

Split the work into:
- backend schema / export changes
- dashboard projection changes
- test fixtures / regression checks

- [ ] **Step 2: Decide whether any field needs a schema migration**

If a missing field must be queryable from the run detail API or export JSON, treat it as backend work, not a React-only workaround.

- [ ] **Step 3: Decide whether the trace dock should be the primary view**

If yes, the dock becomes the place where operators inspect the observability summary, while the export JSON remains the portable artifact.

- [ ] **Step 4: Freeze the plan boundary**

The next plan should only cover the remaining observability projection work; it should not reopen the already-complete record/replay plumbing.
