# ClineSDK Trace-Alignment Audit — §1

**Branch audited:** `feat/cline-runtime-unification`
**Audit date:** 2026-05-25
**Auditor task:** §1 of `cline-live-followups` plan — read-only audit, no code changes.
**Purpose:** Determine whether the ClineSDK `AgentRuntimeEvent` stream maps cleanly onto xvision's
`RunEvent` / `TrajectoryFrame` model before §2 expands live recording.

---

## 1. Architecture Layers (for reference)

```
ClineSDK Agent (AgentRuntimeEvent)
        ↓  tapped by model-wrapper.ts + tool-shim.ts
AgentModelEvent (internal SDK type: text-delta | reasoning-delta | tool-call-delta | usage | finish)
        ↓  frame-recorder.ts → TrajectoryFrame variants (Request / TextDelta / … / Finish)
        ↓  emit.ts → event-client.ts → Unix socket NDJSON
Sidecar JSON-RPC notification (event.run_started / event.model_call_finished / … / event.trajectory_frame)
        ↓  event_sink.rs dispatch()
RunEvent enum (RunStarted / ModelCallFinished / ToolCallStarted | SpanStarted pair / …)
        ↓  RunEventBus → SqliteRecorder
Persisted rows  (agent_runs, spans, tool_calls, model_calls, events, …)

TrajectoryFrame stream (separate, non-droppable):
  event.trajectory_frame notification → parse_trajectory_frame_notification()
  → TrajectoryFramePersister::persist() → TrajectoryStore::append_frame()
  → trajectory_frames table  [NOT YET WIRED end-to-end — Gap #1]
```

---

## 2. Event Mapping Table

Each row covers one event category. Columns:

- **ClineSDK event** — the `AgentRuntimeEvent` type from `agent.subscribe()` (or the `AgentModelEvent` internal type).
- **Sidecar notification** — the NDJSON JSON-RPC method emitted on the event socket by `emit.ts`.
- **Rust RunEvent** — the `RunEvent` variant(s) produced by `event_sink.rs::dispatch()`.
- **TrajectoryFrame variant** — the `TrajectoryFrame` variant recorded by `frame-recorder.ts` / `tool-shim.ts`.
- **Alignment** — `clean` / `lossy` / `dropped` / `reordered` / `ambiguous`.

| Category | ClineSDK event | Sidecar notification | Rust RunEvent | TrajectoryFrame variant | Alignment |
|---|---|---|---|---|---|
| Session lifecycle — start | `run-started` (AgentRuntimeEvent) | `event.run_started` (emitted by `handleSessionStartRun` in `session.ts:87`) | `RunEvent::RunStarted` | — (not a frame-level concern) | **clean** — `run_id`, `objective`, `started_at_ms`, `provider_id`, `model_id` all propagate faithfully |
| Session lifecycle — end (normal) | `run-finished` (AgentRuntimeEvent) | `event.run_finished` (emitted by `handleSessionEndRun` in `session.ts:101`) | `RunEvent::RunFinished` with `status` from `parse_run_status()` | — | **lossy** — `final_artifact_id` is always `None`; `error` is only set when a budget abort fires the `.error` emitter; normal `session.end_run` emits `status:"completed"` with no error |
| Session lifecycle — agent failure / abort | `run-failed` (AgentRuntimeEvent) — thrown path | `event.error` (emitted by `session.ts:219` catch block) | `RunEvent::SidecarError` | — | **lossy** — mapped to `SidecarError` (not `RunFinished`). The `run_finished` notification is never emitted when `session.step` throws, so the run row stays **open** in the recorder. `RunInterrupted` is emitted only by `mark_runs_interrupted()` on sidecar-crash detection in `client.rs`, not on clean SDK errors. |
| Model request frame | (not a top-level AgentRuntimeEvent; fired inside `wrapAgentModel.stream()`) | — (no sidecar notification; goes directly to `frame-recorder.ts::recordRequest`) | — | `TrajectoryFrame::Request` (recorded before first downstream event — `model-wrapper.ts:83`) | **clean** — full messages / tools / system_prompt captured. Emitted via `emitFrame` which calls `emitNotification(NOTIFY.TrajectoryFrame, frame)`. Only active when `config.record === true`. |
| Model call — span start | — (synthesized inside `wrapAgentModel.stream()` before first event) | `event.model_call_started` (emitted at `model-wrapper.ts:96`) | `RunEvent::SpanStarted` (kind=ModelCall) | — | **clean** — span_id is per-stream UUID; provider/model stamped correctly |
| Model call — text streaming | `assistant-text-delta` (AgentRuntimeEvent, Layer 1) | `event.assistant_text_delta` (emitted at `model-wrapper.ts:103`) | `RunEvent::AssistantTextDelta` | `TrajectoryFrame::TextDelta` (recorded by `frame-recorder.ts::recordEvent`) | **lossy** — the sidecar notification carries only `delta_len`, not the actual text (`emit.ts:emitAssistantTextDelta` omits `text` field). The trajectory frame carries the full text correctly. The `RunEvent::AssistantTextDelta::delta_text` field (`event_sink.rs:331`) reads from `str_field("text")` which will be empty for the observability channel. Only the trajectory frame path preserves the full text. |
| Model call — reasoning streaming | `assistant-reasoning-delta` (AgentRuntimeEvent) | — (no sidecar notification emitted) | — | `TrajectoryFrame::ReasoningDelta` (recorded by `frame-recorder.ts::recordEvent` case `reasoning-delta`) | **dropped** for the observability (RunEvent) path. Trajectory frame captures it correctly. No `event.reasoning_delta` notification exists in `emit.ts`. Reasoning tokens are invisible in spans / model_calls rows. |
| Model call — token usage / cost | `usage-updated` (AgentRuntimeEvent) | `event.model_call_finished` (aggregated — `model-wrapper.ts:112` accumulates across the stream, emits once on finish) | `RunEvent::ModelCallFinished` + `RunEvent::SpanFinished` | `TrajectoryFrame::Usage` (recorded per `usage` event chunk) | **lossy** — the observability path receives only the per-stream aggregate (sum of all usage events). The trajectory path records every individual Usage frame, preserving intermediate accumulation. Cache read/write tokens (`cacheReadTokens`, `cacheWriteTokens`) are accumulated in `model-wrapper.ts` but NOT forwarded in `emitModelCallFinished` (`emit.ts:81-91`), so `ModelCallFinishedEvent::input_token_count` / `output_token_count` are correct but cache tokens are dropped from the RunEvent. |
| Model call — span end | `finish` (AgentModelEvent internal) | `event.model_call_finished` (includes `input_tokens`, `output_tokens`, `total_cost`) | `RunEvent::SpanFinished` + `RunEvent::ModelCallFinished` | `TrajectoryFrame::Finish` (reason, error) | **clean** for span lifecycle; **lossy** for prompt/response hashes — `prompt_hash` is set to a synthetic `"agentd-step:<provider>:<model>"` marker (`event_sink.rs:313`) rather than a real content hash. `response_hash`, `prompt_payload_ref`, `response_payload_ref`, `tool_calls_requested` are all `None`. |
| Tool call — start | `tool-started` (AgentRuntimeEvent) | `event.tool_call_started` (`tool-shim.ts:80`) | `RunEvent::SpanStarted` (kind=ToolCall) + `RunEvent::ToolCallStarted` | `TrajectoryFrame::ToolCallDelta` (from model stream — streamed input) | **ambiguous** — the `span_id` used for `emitToolCallStarted` is a freshly minted `newSpanId()` at `tool-shim.ts:54`, independent of the `tool_call_id` from `AgentToolContext`. The trajectory `ToolCallDelta` frame uses `tool_call_id` from the model stream. The `ToolResult` frame uses `context?.toolCallId ?? spanId` (`tool-shim.ts:59`). Cross-correlating the span-id (observability) with the tool_call_id (trajectory) requires the caller to join on tool_name + temporal ordering — no shared key. |
| Tool call — result (success) | `tool-finished` (AgentRuntimeEvent) | `event.tool_call_finished` (`tool-shim.ts:89`) | `RunEvent::SpanFinished` (status=Ok) + `RunEvent::ToolCallFinished` | `TrajectoryFrame::ToolResult` (recorded before return — `tool-shim.ts:99`) | **lossy** — `ToolCallFinishedEvent::exit_code` is always `None` (`event_sink.rs:237`). `output_payload_ref` is always `None`. `ToolCallStartedEvent::side_effect_level` is hardcoded `ReadOnly`, `risk_level` hardcoded `SafeRead`, `requires_approval` false, `is_run_terminator` false for all tools (`event_sink.rs:213-219`). Actual descriptor metadata (from `ToolDescriptor`) is not forwarded through the notification. |
| Tool call — result (error) | (SDK catches thrown errors; tool-shim returns error-as-data) | `event.tool_call_failed` (`tool-shim.ts:106`) | `RunEvent::SpanFinished` (status=Error) + `RunEvent::ToolCallFailed` | `TrajectoryFrame::ToolResult` with `error` field set (`tool-shim.ts:111`) | **clean** — error-as-data pattern honoured; ToolResult frame has `error` field; ToolCallFailed has `error_json`. The run does NOT abort on tool error (correct ClineSDK behaviour). |
| `submit_decision` lifecycle termination | `run-finished` (AgentRuntimeEvent with `finishReason: "stop"` — tool has `lifecycle: { completesRun: true }`) | `event.run_finished` emitted by `handleSessionEndRun` (called explicitly by Rust after `session.step` returns) | `RunEvent::RunFinished` with `status=Completed` | `TrajectoryFrame::ToolCallDelta` (model streams the tool call) + `TrajectoryFrame::ToolResult` (shim records `{ ok: true }`) | **lossy** — the decision JSON is captured locally in `session.store.decisionJson` and surfaced in `StepResult.decision_json`, but is NOT emitted as a `RunEvent` or trajectory frame payload. The artifact that terminates the run (the submitted JSON) has no observability row of its own. `ArtifactWrittenEvent` is never emitted by the sidecar. |
| Usage / cost metadata (run-level aggregate) | `usage-updated` (fires per step from SDK) | `event.model_call_finished` per stream invocation | `RunEvent::ModelCallFinished` per stream | `TrajectoryFrame::Usage` per chunk | **lossy** — cache tokens dropped from observability (see model call row above). `total_cost` is optional and some providers may not emit it (`emitModelCallFinished` only includes it when `typeof params.total_cost === "number"` — `emit.ts:emitModelCallFinished`). |
| Provider failover / retry | `status-notice` (AgentRuntimeEvent — SDK emits this on retry/failover) | — (no sidecar notification) | — | `TrajectoryFrame::RetryOrCancel` (`frame-recorder.ts` maps `finish` event with reason indicating retry — but retry is driven by the `Agent` runtime internally, not surfaced as a distinct AgentModelEvent to the wrapper) | **dropped** for observability. **ambiguous** for trajectory: `RetryOrCancel` frame exists in the Rust schema and TypeScript types but `frame-recorder.ts::recordEvent` never emits it — the event type `"retry"` / `"failover"` does not appear in the `AgentModelEvent` union the recorder handles. The `default` branch in `recordEvent` silently skips unknown types. |
| Cancellation (operator abort) | `run-failed` with `AbortSignal` reason | `event.tool_call_cancelled` (if mid-tool — `tool-shim.ts:66-75`) + `event.error` (from `session.step` catch or timer abort) | `RunEvent::ToolCallCancelled` + `RunEvent::SpanFinished`(Cancelled) for tool; `RunEvent::SidecarError` for the run | `TrajectoryFrame::RetryOrCancel` — intended for this case but NOT emitted (same gap as retry above) | **lossy** — tool-level cancellation is captured (signal abort path in `tool-shim.ts:61-75`). Run-level abort from wall-timer (`agent.abort(new Error("budget_wall_ms_exceeded"))`) emits `event.error`, which maps to `SidecarError` not `RunFinished`, leaving the run row open. |
| Wall-clock timeout (budget) | `run-failed` with error message | `event.error` with `message: "budget_wall_ms_exceeded"` (`session.ts:212`) | `RunEvent::SidecarError` | — | **lossy** — same issue as cancellation: `RunFinished` is not emitted after a budget abort; only `event.error`. The Rust run row stays open unless the caller also calls `session.end_run` (Rust caller does this via `AgentClient::end_run`). If `end_run` is called after abort, `handleSessionEndRun` emits `event.run_finished` with `status:"completed"` — which is the WRONG status for an aborted run. |
| Sidecar crash / recovery | N/A (process-level kill) | — (no notification from dead process) | `RunEvent::RunInterrupted` (emitted by `mark_runs_interrupted()` in `event_sink.rs:554`) | — | **clean** for crash detection: the Rust client detects connection drop and calls `mark_runs_interrupted()`. All open span rows are marked `interrupted` by the recorder. No trajectory corruption detection is triggered from this path (the `TrajectoryFramePersister` consumer would die when the store is abandoned, but `corrupt` is not persisted unless `persist()` was in-flight). |
| Backpressure / overload | — (not an SDK event; generated by `event-client.ts` when write buffer > 64 KB) | `event.overloaded` (`event-client.ts:checkBackpressure`) | `RunEvent::BackpressureDropped` | — | **clean** for the signal; `dropped` count is always `0` (the overload notification itself doesn't count actual dropped events — it's a buffer-high watermark alert, not a drop counter). |
| Terminal success state | `run-finished` (AgentRuntimeEvent) | `event.run_finished` with `status:"completed"` | `RunEvent::RunFinished` | `TrajectoryFrame::Finish` with `reason:"stop"` | **clean** for normal success path |
| Terminal error state | `run-failed` (AgentRuntimeEvent) | `event.error` (NOT `event.run_finished`) | `RunEvent::SidecarError` (NOT `RunFinished`) | `TrajectoryFrame::Finish` with `reason:"error"` | **ambiguous** — the observability run row is left open (`RunFinished` never arrives); the trajectory Finish frame correctly marks the terminal state. The two paths diverge on error terminals. |
| Corrupt recording detection | (N/A — architectural) | — | — | `TrajectoryFramePersister::persist()` returns `Err` → caller marks recording `corrupt` | **clean** by design — lossless `FrameChannel` backpressure + consumer-death detection. But the caller that processes `event.trajectory_frame` notifications is NOT yet wired into the `AgentClient::spawn_with_event_sink` call chain (Gap #1). |

---

## 3. Gap List

### Gap #1 — Live record→sidecar wiring incomplete (critical for §2)

**Risk:** Production recordings never persist.

**Description:** `TrajectoryFramePersister` and `TrajectoryStore` are fully implemented and tested via direct store seeding. However, `AgentClient::spawn_with_event_sink` (`client.rs:58-120`) accepts a `RunEventBus` but has no parameter for `TrajectoryStore` or `RecordingId`. The `parse_trajectory_frame_notification()` function (`event_sink.rs:446`) exists and is tested, but the `dispatch()` function in `event_sink.rs:148` does NOT handle `"event.trajectory_frame"` — it falls through to the silent `_ => return None` default at line 376.

**Code site:** `crates/xvision-agent-client/src/event_sink.rs:376` (missing arm for `"event.trajectory_frame"`) and `crates/xvision-agent-client/src/client.rs` (missing `TrajectoryStore` parameter on `spawn_with_event_sink`).

---

### Gap #2 — Run row left open on error / abort terminals

**Risk:** SQLite accumulates permanently-open run rows; eval-review surfaces stale data.

**Description:** When `session.step` throws (SDK error, uncaught exception) the `catch` block in `session.ts:217` emits `event.error`, which maps to `RunEvent::SidecarError`, not `RunEvent::RunFinished`. The recorder never closes the run. Similarly, budget-wall abort emits `event.error` + eventually `event.run_finished` with `status:"completed"` (wrong status). On normal budget abort without error-path, the Rust caller must call `AgentClient::end_run` which triggers `handleSessionEndRun` → `emitRunFinished(status:"completed")` — status is incorrect for aborted runs.

**Code site:** `xvision-agentd/src/methods/session.ts:217` (catch block emits error but not run_finished); `xvision-agentd/src/methods/session.ts:101` (end_run always emits `"completed"`).

---

### Gap #3 — `RetryOrCancel` TrajectoryFrame never emitted

**Risk:** Replay determinism is broken when the SDK internally retries a model call (provider failover, 529 overload, etc.). The replayer cannot reconstruct the retry branch.

**Description:** `frame-types.ts` and the Rust `TrajectoryFrame` enum both define `RetryOrCancel`. The `frame-recorder.ts::recordEvent` switch handles `text-delta | reasoning-delta | tool-call-delta | usage | finish` — there is no case for a retry/failover signal. The `AgentModelEvent` union in `model-wrapper.ts` does not include a retry variant either. The ClineSDK internal retry is not exposed as an `AgentModelEvent`; it surfaces only as a `status-notice` in `AgentRuntimeEvent` (which the sidecar does not subscribe to — it only wraps the `AgentModel.stream()` iterator). The `default: break` in `recordEvent` silently drops any unrecognised event type.

**Code site:** `xvision-agentd/src/session/frame-recorder.ts:default` branch (line ~115); `xvision-agentd/src/session/model-wrapper.ts` — `AgentModelEvent` union definition; ClineSDK does not expose retry as a stream event.

**Note:** This cannot be determined purely statically — whether the ClineSDK runtime exposes retry as a distinct `AgentModelEvent` or only as a Layer-3 `status-notice` requires a runtime fixture. Recommend capturing a provider-error fixture in §2 to confirm.

---

### Gap #4 — Cache token counts dropped from RunEvent observability

**Risk:** Cost tracking and budget enforcement undercount tokens when providers use prompt caching (Anthropic Claude caching, OpenAI cached reads).

**Description:** `model-wrapper.ts` accumulates `cacheReadTokens` and `cacheWriteTokens` from the `usage` event stream (lines 108-113) but `emitModelCallFinished` (`emit.ts:81`) only forwards `input_tokens`, `output_tokens`, and `total_cost`. The `event.model_call_finished` notification carries no cache token fields. `event_sink.rs::dispatch` for `event.model_call_finished` reads `i64_field("input_tokens")` / `i64_field("output_tokens")` / `f64_field("total_cost")` (lines 305-308) but no cache fields. The trajectory `UsageFrame` correctly captures all four counts.

**Code site:** `xvision-agentd/src/session/emit.ts:emitModelCallFinished` function; `crates/xvision-agent-client/src/event_sink.rs:302-309`.

---

### Gap #5 — `submit_decision` payload not in observability layer

**Risk:** The most important per-decision artifact (the trading decision JSON) has no `ArtifactWrittenEvent` or dedicated observability row. Eval-review cannot surface it from `agent_runs` directly.

**Description:** `submit-decision.ts::buildSubmitDecisionTool` captures the decision JSON via `capture(JSON.stringify(input))` and stores it in `session.decisionJson`. It is surfaced in `StepResult.decision_json` returned to the Rust caller. The Rust engine reads it from the JSON-RPC response body. No `event.artifact_written` (or equivalent) is ever emitted by the sidecar. `RunEvent::ArtifactWritten` exists but has no emitter in this path. The decision is only available from the RPC response, not the event stream.

**Code site:** `xvision-agentd/src/session/submit-decision.ts:28` (capture, no emit); `crates/xvision-observability/src/events.rs:ArtifactWrittenEvent` (unused by this path).

---

### Gap #6 — Tool descriptor metadata not forwarded (side-effect level, approval, run-terminator flag)

**Risk:** `ToolCallStarted` rows in the DB always show `side_effect_level=ReadOnly`, `risk_level=SafeRead`, `requires_approval=false`, `is_run_terminator=false` even for write-capable or approval-required tools.

**Description:** `event_sink.rs:207-219` hardcodes all four fields when constructing `ToolCallStartedEvent` from `event.tool_call_started`. The sidecar notification (`emit.ts:emitToolCallStarted`) does not include these fields. The `ToolDescriptor` struct (which has `side_effect_level` and `requires_approval`) is available in `tool-shim.ts` at `buildTool(d, opts)` but is not forwarded into the notification payload.

**Code site:** `xvision-agentd/src/session/tool-shim.ts:buildTool` (omits descriptor metadata from notification); `crates/xvision-agent-client/src/event_sink.rs:207-219` (hardcoded values).

---

### Gap #7 — Reasoning tokens invisible to observability

**Risk:** Extended-thinking model runs (Anthropic Claude extended thinking, o1/o3 reasoning) produce `ReasoningDelta` frames in the trajectory store but zero observability signal. Token attribution for thinking tokens is invisible in `model_calls` rows.

**Description:** `frame-recorder.ts::recordEvent` handles `reasoning-delta` and emits `TrajectoryFrame::ReasoningDelta` — good for replay. But `model-wrapper.ts` has no `emitReasoningDelta` call (only `emitAssistantTextDelta` for `text-delta`). The `AgentModelEvent` union in `model-wrapper.ts` includes `reasoning-delta` but the only action taken is recording the frame. The `event.assistant_text_delta` notification never fires for reasoning tokens.

**Code site:** `xvision-agentd/src/session/model-wrapper.ts` — the `reasoning-delta` branch yields the event but emits nothing to the socket.

---

### Gap #8 — `emitFrame` drops `run_id` and coordinate fields

**Risk:** `event.trajectory_frame` notifications cannot be routed to the correct `RecordingId` by the Rust consumer without `run_id`. If multiple concurrent sessions ever emit frames (concurrency is currently serialised, but see active-run.ts comment about v1 limitation), frames will be unroutable.

**Description:** `emit.ts:emitFrame` emits `NOTIFY.TrajectoryFrame` with only the raw `TrajectoryFrame` body (no `run_id`, `slot_role`, `step_index`, `frame_index`). The sidecar's `parse_trajectory_frame_notification()` in `event_sink.rs:446` expects all four coordinate fields plus the frame body under `"frame"`. The current emit shape would fail `parse_trajectory_frame_notification` — `params.get("run_id")` would return `None` and the function would return `None` silently.

**Code site:** `xvision-agentd/src/session/emit.ts:emitFrame` (emits frame body only, no coordinates); `crates/xvision-agent-client/src/event_sink.rs:447` (expects `run_id`, `slot_role`, `step_index`, `frame_index` fields).

This is a protocol mismatch: the emit shape and the parse shape are incompatible. §2 must fix this before frame persistence can work end-to-end.

---

## 4. Fixture Recommendations for §2

§2 should capture the following representative recordings to validate the wired path and provide regression fixtures:

| Fixture | What it captures | Recommended path |
|---|---|---|
| `fixtures/traj_success_submit_decision.ndjson` | Full happy path: Request → TextDelta → ToolCallDelta(submit_decision) → ToolResult → Usage → Finish(stop). Validates the clean categories. | `crates/xvision-engine/tests/fixtures/` |
| `fixtures/traj_tool_error_recovery.ndjson` | Tool call that throws (error-as-data ToolResult) + model continues to next turn. Validates Gap #2 / ToolCallFailed path. | `crates/xvision-engine/tests/fixtures/` |
| `fixtures/traj_budget_wall_abort.ndjson` | Run that hits wall-clock timeout mid-step. Validates the `budget_wall_ms_exceeded` abort path and whether `RunFinished` vs `SidecarError` are emitted correctly (Gap #2). | `crates/xvision-engine/tests/fixtures/` |
| `fixtures/traj_provider_error.ndjson` | Model call that fails with a provider 5xx / 529 mid-stream. Validates whether `RetryOrCancel` ever appears (Gap #3) and what the Finish frame `reason` value is on provider error. | `crates/xvision-engine/tests/fixtures/` |
| `fixtures/traj_user_cancellation.ndjson` | AbortSignal fired mid-tool-execution. Validates `ToolCallCancelled` emission and that the span closes correctly. | `crates/xvision-engine/tests/fixtures/` |
| `fixtures/traj_reasoning_model.ndjson` | A run using an extended-thinking model. Validates `ReasoningDelta` frame presence and confirms whether reasoning tokens are absent from observability (Gap #7). | `crates/xvision-engine/tests/fixtures/` |

Fixtures should be captured by running the mock agentd (`tests/fixtures/mock_agentd.js` on the cline branch) with record mode enabled once Gap #1 and Gap #8 are resolved, or by constructing NDJSON manually from the frame-types schema.

---

## 5. §2 Readiness Assessment

### What §2 must wire (based on Gap #1 + Gap #8)

1. **Fix `emitFrame` coordinate fields** (`xvision-agentd/src/session/emit.ts`): `emitFrame` must include `run_id` (from `activeRunId()`), `slot_role`, `step_index`, and `frame_index` alongside the frame body under a `"frame"` key. The `frame-recorder.ts` must thread step_index and frame_index monotonically. This makes the emit shape match `parse_trajectory_frame_notification()`.

2. **Wire `"event.trajectory_frame"` in `dispatch()`** (`crates/xvision-agent-client/src/event_sink.rs`): Add a match arm for `"event.trajectory_frame"` that calls `parse_trajectory_frame_notification()` and routes the result to a `TrajectoryFramePersister`. The persister must be created before the session starts and destroyed when it ends.

3. **Pass `TrajectoryStore` + `RecordingId` through `AgentClient::spawn_with_event_sink`** (`crates/xvision-agent-client/src/client.rs`): The sink needs access to the store so `persist()` can write frames. The `RecordingId` is created by `TrajectoryStore::begin_recording()` which requires a `TrajectoryKey` — the engine must mint the key and recording before `start_run` and close it after `end_run`.

4. **Thread `record=true` from the Rust caller to `StartRunParams`** (`crates/xvision-agent-client/src/protocol.rs`): The `StartRunParams` struct already has `decision_schema` but not a `record` field. Add it and pass it from the engine when trajectory recording is requested for a run.

### ClineSDK-event mismatches §2 must handle

- **Provider retry / failover events (Gap #3):** Cannot be handled at the `AgentModelEvent` level — the ClineSDK does not expose retry as a stream event. §2 must either accept that `RetryOrCancel` frames are never emitted (replay cannot reproduce retry branches) or instrument at the `AgentRuntimeEvent` `status-notice` level to synthesize a marker. Recommend documenting as a known limitation and capturing a runtime fixture (Gap #3 cannot be resolved statically).

- **Error-terminal run status (Gap #2):** §2 must decide whether the Rust caller or the sidecar is responsible for emitting `RunFinished` on abort/error. The cleanest fix is for `session.ts::handleSessionStep` to always emit `event.run_finished` (with the correct status) in its `finally` block before the RPC response is returned, so the observability stream is always complete.

- **`submit_decision` artifact (Gap #5):** If the eval-review surface needs the submitted decision as a retrievable artifact, §2 must add `event.artifact_written` emission in `submit-decision.ts` or accept that decision JSON is only in `StepResult.decision_json` (RPC response only). The `ArtifactWrittenEvent` type already exists; it just needs a call site.

---

*Evidence citations: all file:line references are to `feat/cline-runtime-unification` read via `git show`. ClineSDK runtime behaviour not observable statically (retry, reasoning token attribution, provider-error stream shape) is flagged as requiring runtime fixture capture in §2.*
