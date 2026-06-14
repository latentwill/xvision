// frontend/web/src/api/types-agent-runs.ts
//
// Types mirror the Rust data model in
// docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md.
// When the backend lands ts-rs derives, replace this file with the
// generated bindings.

import type { UnifiedEvent } from "./unified-events";

export type RunStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted"
  | "agent_failure";

/**
 * Retention modes mirror the recorder's on-disk policy (see
 * docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md §retention).
 *
 * - `hash_only` — default. Prompts and tool payloads are hashed; no raw
 *   text on disk. Inspector shows hashes + redaction notes.
 * - `redacted` — redacted payload snippets retained alongside hashes.
 * - `full_debug` — raw prompts, responses, and tool I/O retained on disk.
 *   Surfaces a banner because PII/credential leakage risk increases.
 */
export type RetentionMode = "hash_only" | "redacted" | "full_debug";

export type SpanKind =
  | "agent.run"
  | "agent.plan"
  | "agent.decision"
  // WS-17 span taxonomy: the model invocation that produces the trade
  // decision (`decision.model`) and its captured chain-of-thought
  // (`decision.reasoning`, nested under `decision.model`). These replace
  // the generic `model.call` / `model.reasoning` on the trading path —
  // the slot ROLES (trader/regime/filter) were retired, so it's a single
  // decision-model call now.
  | "decision.model"
  | "decision.reasoning"
  // Legacy wire values, retained so historical exports / older recorded
  // rows still type-check. The engine no longer emits these (the variant
  // was renamed to DecisionModel/DecisionReasoning).
  | "model.call"
  | "model.reasoning"
  | "tool.call"
  | "tool.validate_input"
  | "tool.validate_output"
  | "approval.request"
  | "approval.response"
  | "sandbox.exec"
  | "supervisor.review"
  | "financial.eval"
  | "artifact.write"
  | "ipc.notification"
  | "skill.invoke"
  | "broker.call"
  | "recovery.attempt"
  | "state.transition"
  // WS-11a OPTI trace scope: the autooptimizer *cycle* projected onto the
  // trace-dock surface. These rows are NOT agent-run spans — the OPTI scope
  // reducer (`features/autooptimizer/opti-trace-reducer.ts`) synthesizes them
  // from the existing `CycleProgressEvent` SSE stream so the optimizer cycle
  // reads as a live trace (operator-labeled rows) in the dock. They share the
  // `RunSpan` shape + `SpanTree` rendering, hence they live on `SpanKind`.
  //   opti.cycle      — the cycle root (CycleStarted → CycleFinished)
  //   opti.parent     — the selected parent strategy for the cycle
  //   opti.experiment — one proposed candidate (MutationProposed)
  //   opti.gate       — a candidate's gate verdict (kept/suspect/rejected)
  //   opti.honesty    — the per-cycle honesty check (null-result canary)
  //   opti.judge      — a reviewer finding on a candidate
  //   opti.flywheel   — the DSPy flywheel compile step
  //   opti.eval-run   — WS-11b: the candidate's persisted eval run, nested
  //                     under its experiment. A navigable drill-link node
  //                     (its `attributes.eval_run_id` points at the
  //                     `/agent-runs/:runId` trace); NOT an inline embed of
  //                     that run's span tree.
  | "opti.cycle"
  | "opti.parent"
  | "opti.experiment"
  | "opti.gate"
  | "opti.honesty"
  | "opti.judge"
  | "opti.flywheel"
  | "opti.eval-run"
  // WS-8 taxonomy convergence: a synthetic span kind for the bar-level engine
  // lifecycle signals written to the `events` table (and streamed live as
  // `engine_event` SSE frames). These were dropped from the trace before WS-8
  // — projecting each `EngineEvent` onto a `RunSpan` with this kind lets them
  // flow through the existing tree / inspector / filter machinery. The actual
  // `EngineEvent.kind` (e.g. `risk_veto`, `order_signed`) is carried in
  // `attributes.engine_event_kind`; the family/label/color resolve off that.
  | "engine.event";

/**
 * Trace-dock-visible side of a broker submit. `Close` / `Short` are
 * derived from the trader's action, not the wire-level Buy/Sell, so
 * short-sale fills (#14 round-2 intake) show up as `short` instead
 * of an ambiguous `sell`. Mirrors `xvision_observability::BrokerSide`.
 */
export type BrokerSide = "buy" | "sell" | "close" | "short";

/** Terminal state of a broker submit. Mirrors `BrokerCallOutcome`. */
export type BrokerCallOutcome = "filled" | "rejected" | "cancelled" | "failed";

/**
 * Detail payload surfaced on a `broker.call` span. The dashboard
 * normalises the matching `broker_call_started` + `broker_call_finished`
 * events into this shape so the SpanInspector can render side / qty /
 * fill / error without joining two events.
 */
export type BrokerCallDetail = {
  side: BrokerSide;
  symbol: string;
  qty: number;
  intended_price: number | null;
  order_type: string;
  venue: string;
  idempotency_key: string | null;
  outcome: BrokerCallOutcome | null;
  fill_price: number | null;
  fill_qty: number | null;
  fee: number | null;
  broker_order_id: string | null;
  error_class: string | null;
  error_message: string | null;
  /**
   * Severity tag for the trace dock. `"warn"` means the broker
   * rejected the order but the run continues (the agent gets the
   * error fed back on the next decision cycle and self-heals).
   * `"error"` means the run terminated. `null` on filled /
   * cancelled outcomes. Added by `agent-error-feedback-self-healing`.
   */
  severity: "warn" | "error" | null;
};

export type SpanStatus = "ok" | "error" | "in_progress";

export type RunSpan = {
  span_id: string;
  parent_span_id: string | null;
  name: string;
  kind: SpanKind;
  started_at: string; // ISO
  finished_at: string | null; // ISO, null = in-flight
  status: SpanStatus;
  attributes: Record<string, unknown>;
  /**
   * Human-readable error message extracted from the span's
   * `error_json` payload (observability schema). Present iff
   * `status === "error"`. Surfaced as a first-class field so the
   * inspector can render it without reaching into `attributes`.
   *
   * Per qa-trace-error-surfacing (2026-05-17, operator walk-through):
   * a failed LLM call must show its error in the trace dock, not
   * silently render as "Completed".
   */
  error_message?: string;
  // Prototype-driven extensions: live in `attributes` server-side but
  // surface as first-class so the inspector can render them as pull-quotes.
  prompt?: string;
  response?: string;
  response_partial?: string;
  args?: unknown;
  result?: unknown;
  decision_idx?: number;
  provider?: string;
  model?: string;
  /** `prompt_hash` from the matching `model_calls` row (kept as `hash`
   * for back-compat with existing fixtures + SpanInspector field row). */
  hash?: string;
  response_hash?: string;
  /** Blob-store refs for the prompt + completion bodies. Only populated
   * when retention is `redacted` or `full_debug`. The ref itself is
   * surfaced in SpanInspector for inline first-load body previews. */
  prompt_payload_ref?: string;
  response_payload_ref?: string;
  tokens_in?: number;
  tokens_out?: number;
  cost?: number;
  /**
   * Populated on `broker.call` spans (qa-trace-broker-spans). The
   * SpanInspector renders side / qty / fill status / error from this
   * payload alongside model.call rows.
   */
  broker_call?: BrokerCallDetail;
  streaming?: boolean;
};

export type ModelCall = {
  model_call_id: string;
  span_id: string;
  provider: string;
  model: string;
  input_tokens: number | null;
  output_tokens: number | null;
  cost_usd: number | null;
  prompt_hash: string;
  response_hash?: string | null;
  prompt_text: string | null;
  prompt_payload_ref?: string | null;
  response_payload_ref?: string | null;
  response_text: string | null;
};

export type ToolCall = {
  tool_call_id: string;
  span_id: string;
  tool_name: string;
  input_json: unknown;
  output_json: unknown | null;
  error: string | null;
  started_at: string;
  finished_at: string | null;
};

export type AgentRunAccounting = {
  source: "agent_model_calls" | "eval_model_calls" | "eval_actuals" | "none";
  eval_run_id: string | null;
  eval_mode: "backtest" | "live" | string | null;
  eval_status: string | null;
  eval_actual_input_tokens: number | null;
  eval_actual_output_tokens: number | null;
  eval_model_calls: number;
  eval_model_call_input_tokens: number | null;
  eval_model_call_output_tokens: number | null;
  eval_model_call_cost_usd: number | null;
};

export type MemoryRecallItem = {
  id: string;
  score: number;
  text_preview: string;
};

export type MemoryRecallPayload = {
  run_id: string;
  flywheel_cycle_id?: string | null;
  decision_id: number;
  namespace: string;
  items: MemoryRecallItem[];
};

export type MemoryWritePayload = {
  run_id: string;
  flywheel_cycle_id?: string | null;
  decision_id: number;
  namespace: string;
  memory_item_id: string;
  text_preview: string;
};

export type AgentRunMemoryEvent =
  | {
      kind: "memory_recall";
      created_at: string;
      payload: MemoryRecallPayload;
    }
  | {
      kind: "memory_write";
      created_at: string;
      payload: MemoryWritePayload;
    }
  | {
      kind: string;
      created_at: string;
      payload: unknown;
    };

export type AgentRunMemoryEventsResponse = {
  run_id: string;
  events: AgentRunMemoryEvent[];
};

/**
 * Trajectory mode set on the `agent_runs` row by the engine
 * (migration 039). Declared here so the UI can render defensively
 * even before all API paths expose the field.
 *
 * - `live`   — real-time execution against a live provider (default).
 * - `record` — live execution with trajectory frames recorded to the
 *              TrajectoryStore for later replay.
 * - `replay` — deterministic re-execution driven by previously-recorded
 *              frames; zero provider calls.
 */
export type TrajectoryMode = "live" | "record" | "replay";

export type AgentRunSummary = {
  run_id: string;
  objective: string;
  strategy_id: string | null;
  agent_id: string | null;
  started_at: string;
  finished_at: string | null;
  status: RunStatus;
  // Pre-rolled aggregates (avoid client-side scans for the strip).
  span_count: number;
  model_call_count: number;
  tool_call_count: number;
  error_count: number;
  total_cost_usd: number;
  total_input_tokens: number;
  total_output_tokens: number;
  duration_ms: number | null;
  financial_eval_id: string | null;
  /**
   * Retention mode the run was recorded under. Drives the header badge
   * and (when `full_debug`) the warning banner about on-disk payloads.
   */
  retention_mode: RetentionMode;
  /**
   * Trajectory execution mode (migration 039). Absent on runs predating
   * the migration — treat as `"live"` for rendering. Populated by the
   * Cline runtime; always `"live"` on the LlmDispatch path.
   */
  trajectory_mode?: TrajectoryMode;
  /**
   * Fraction of model-call steps served from recorded frames during a
   * `replay` run. Absent on `live` / `record` runs. Range [0, 1].
   */
  replay_hit_ratio?: number | null;
  /**
   * Count of trajectory events that could not be replayed / delivered
   * due to buffer pressure. Absent when zero or on non-replay runs.
   */
  dropped_events?: number;
  /**
   * Machine-readable reason the replay was terminated or aborted early.
   * Known values (from Stages 2–3):
   *   `replay_divergence`        — agent made a different tool call than recorded.
   *   `replay_frames_exhausted`  — ran out of recorded frames before agent finished.
   * Absent on runs that completed cleanly or are not in replay mode.
   */
  recovery_reason?: string | null;
  /**
   * Per-run pause flag (eval `RunSummary.paused`, migration 061). When the
   * agent-run summary endpoint joins the eval run record it is populated;
   * absent on runs/endpoints that don't carry it yet. The Live Trading page
   * strip reads it to derive PAUSED vs ACTIVE. B-III wires the
   * pause/resume transport that flips it.
   */
  paused?: boolean;
  /**
   * RFC3339 timestamp of the most recent pause; `null`/absent when never
   * paused or after resume. Mirrors `RunSummary.paused_at`.
   */
  paused_at?: string | null;
  /**
   * Mode of the parent eval run, normalized server-side to
   * `"backtest" | "live"` (legacy `'paper'` rows read back as
   * `"backtest"`). `null`/absent when the agent run has no parent eval
   * run. Served by `GET /api/agent-runs` (LEFT JOIN onto `eval_runs`).
   */
  eval_mode?: "backtest" | "live" | null;
  /**
   * Raw status of the parent eval run
   * (`queued|running|completed|failed|cancelled`). `null`/absent without
   * a parent. Used to demote agent runs stuck in `running` whose parent
   * eval run is already terminal — they render as STALE, never live.
   */
  eval_run_status?: string | null;
  /**
   * THE live-money discriminator (xvision-9pi): `true` iff the parent
   * eval run has `mode = live` AND that eval run is non-terminal.
   * Backtests, orphaned children of finished live runs, and parentless
   * runs are all `false`. Only this signal may drive "live" UI.
   */
  is_live_money?: boolean;
  /**
   * One-shot "flatten positions" request flag (eval `RunSummary.flatten_requested`,
   * migration 062). `true` ⇒ the live executor will close all open broker
   * positions on its next cycle and clear the flag, WITHOUT terminating the
   * run. Absent on runs/endpoints that don't carry it yet. The Live Trading page
   * (spec §2.7) reads it to show the pending-flatten state; B-III's transport
   * flips it via the [Flatten positions] inline action.
   */
  flatten_requested?: boolean;
  /**
   * v2 export accounting provenance. Present when a backend
   * `xvn.agent_run.v2` export is normalized; absent for older detail
   * envelopes and v1 exports.
   */
  accounting?: AgentRunAccounting | null;
};

export type AgentRunDetail = {
  summary: AgentRunSummary;
  spans: RunSpan[];
  model_calls: ModelCall[];
  tool_calls: ToolCall[];
};

// ---------------------------------------------------------------------------
// SSE stream events (real wire protocol shipped in #235)
// ---------------------------------------------------------------------------
//
// Wire format produced by `crates/xvision-dashboard/src/sse/mod.rs`:
//
//   event: snapshot              data: <AgentRunExport JSON>      // first frame
//   event: <variant_snake_case>  data: <RunEvent JSON>            // tail
//   event: lagged                data: {"dropped": n}             // synthetic
//
// Variants below correspond 1:1 to the Rust `RunEvent` enum in
// `crates/xvision-observability/src/events.rs`. We intentionally keep the
// payload types loose (`unknown` / minimal interfaces) for variants the
// frontend does not yet consume — the recorder owns the canonical shape
// and reshaping them here would just create drift.
//
// The mock branch keeps emitting `summary` / `span` arms; both are
// retained additively so the dock keeps working in test/dev MODE.

export type StreamSpanStartedData = {
  span_id: string;
  run_id: string;
  parent_span_id: string | null;
  kind: SpanKind;
  name: string;
  started_at: string;
  otel_trace_id?: string | null;
  otel_span_id?: string | null;
  attributes_json?: string | null;
};

export type StreamSpanFinishedData = {
  span_id: string;
  ended_at: string;
  status: SpanStatus;
  error_json?: string | null;
};

export type StreamModelCallFinishedData = {
  span_id: string;
  provider: string;
  model: string;
  input_token_count?: number | null;
  output_token_count?: number | null;
  cost_usd?: number | null;
  prompt_hash: string;
  response_hash?: string | null;
};

export type StreamToolCallStartedData = {
  span_id: string;
  tool_name: string;
  input_hash: string;
  requires_approval?: boolean;
  is_run_terminator?: boolean;
};

export type StreamToolCallFinishedData = {
  span_id: string;
  output_hash?: string | null;
  exit_code?: number | null;
};

export type StreamToolCallFailedData = {
  span_id: string;
  error_json?: string | null;
};

export type StreamToolCallCancelledData = {
  span_id: string;
  reason?: string | null;
};

/**
 * SSE payload for `broker_call_started` / `broker_call_finished`. Mirrors
 * `xvision_observability::Broker{Started,Finished}Event`. The trace
 * dock typically invalidates the canonical agent-run detail on these
 * events rather than reconstructing the broker_call payload from the
 * deltas — keep parity with the model_call_finished / tool_call_finished
 * arms.
 */
export type StreamBrokerCallStartedData = {
  span_id: string;
  run_id: string;
  side: BrokerSide;
  symbol: string;
  qty: number;
  intended_price: number | null;
  order_type: string;
  venue: string;
  idempotency_key: string | null;
};

export type StreamBrokerCallFinishedData = {
  span_id: string;
  outcome: BrokerCallOutcome;
  fill_price: number | null;
  fill_qty: number | null;
  fee: number | null;
  broker_order_id: string | null;
  error_class: string | null;
  error_message: string | null;
  /**
   * `"warn"` for recoverable broker errors that fed back to the
   * agent (the run continues); `"error"` for fatal errors that
   * terminated the run; `null` on filled / cancelled outcomes.
   * Added by `agent-error-feedback-self-healing`.
   */
  severity?: string | null;
};

export type StreamAssistantTextDeltaData = {
  span_id: string;
  run_id: string;
  delta_len: number;
  /**
   * The chunk text. Empty string when the producer ships counts only
   * (older sidecars / providers that haven't switched to streaming SSE).
   * Concatenated per-span by `useTraceDock.streamingState.bodiesBySpan`
   * to drive the trace dock's live response pull-quote.
   */
  delta_text?: string;
};

export type StreamSidecarErrorData = {
  run_id: string;
  message: string;
  severity: string;
};

export type StreamLaggedData = { dropped: number };

// Loose payloads for events we surface but don't yet render against.
// Shapes match `crates/xvision-observability/src/events.rs` and may
// gain typed fields as consumers need them.

/**
 * Mirrors `crates/xvision-observability/src/events.rs` `EngineEvent` struct
 * (serde-serialized field names). The `kind` string carries the producer-
 * defined event kind (e.g. `decision_started`, `guardrail_fired`); `payload_json`
 * is an optional opaque JSON blob whose shape is per-kind. `span_id` is only
 * present when the event is scoped to a specific span; `run_id` is always set.
 * `created_at` is an ISO-8601 UTC timestamp string.
 */
export type StreamEngineEventData = {
  run_id: string;
  span_id?: string | null;
  kind: string;
  payload_json?: string | null;
  created_at: string;
};

export type StreamCheckpointWrittenData = { run_id: string; path?: string | null };
export type StreamSupervisorNoteData = { run_id: string; message: string };
export type StreamArtifactWrittenData = { run_id: string; path?: string | null };
export type StreamBackpressureDroppedData = { dropped: number };
export type StreamMemoryRecallData = MemoryRecallPayload;
export type StreamMemoryWriteData = MemoryWritePayload;

/**
 * Stream events surfaced to the dock + components. Mock arms (`summary`,
 * `span`) are kept additive so the test/dev mock branch keeps working;
 * real branch arms map 1:1 to the SSE wire vocabulary.
 */
export type AgentRunStreamEvent =
  // Mock-branch arms (Phase 0 fixtures + Vitest).
  | { event: "span"; data: RunSpan }
  | { event: "summary"; data: AgentRunSummary }
  // Real-branch arms (SSE wire protocol).
  | { event: "snapshot"; data: AgentRunDetail }
  // WS-8 Part 2 B2: the LIVE tail now arrives as `UnifiedEvent` frames on a
  // single stable `event: unified` name (the backend projects each `RunEvent`
  // via `RunEventProjector`). The dock folds these through the shared
  // fidelity-complete projection so span detail reconstructs without a refetch.
  // The per-`RunEvent`-name arms below are retained ONLY for the legacy mock
  // path + back-compat; the real wire no longer emits them.
  | { event: "unified"; data: UnifiedEvent }
  | { event: "run_started"; data: { run_id: string; objective: string; started_at: string } }
  | { event: "run_finished"; data: { run_id: string; finished_at: string; status: RunStatus } }
  | { event: "run_interrupted"; data: { run_id: string; finished_at: string; reason: string } }
  | { event: "span_started"; data: StreamSpanStartedData }
  | { event: "span_finished"; data: StreamSpanFinishedData }
  | { event: "model_call_finished"; data: StreamModelCallFinishedData }
  | { event: "tool_call_started"; data: StreamToolCallStartedData }
  | { event: "tool_call_finished"; data: StreamToolCallFinishedData }
  | { event: "tool_call_failed"; data: StreamToolCallFailedData }
  | { event: "tool_call_cancelled"; data: StreamToolCallCancelledData }
  | { event: "broker_call_started"; data: StreamBrokerCallStartedData }
  | { event: "broker_call_finished"; data: StreamBrokerCallFinishedData }
  | { event: "engine_event"; data: StreamEngineEventData }
  | { event: "assistant_text_delta"; data: StreamAssistantTextDeltaData }
  | { event: "sidecar_error"; data: StreamSidecarErrorData }
  | { event: "checkpoint_written"; data: StreamCheckpointWrittenData }
  | { event: "supervisor_note"; data: StreamSupervisorNoteData }
  | { event: "artifact_written"; data: StreamArtifactWrittenData }
  | { event: "backpressure_dropped"; data: StreamBackpressureDroppedData }
  | { event: "memory_recall"; data: StreamMemoryRecallData }
  | { event: "memory_write"; data: StreamMemoryWriteData }
  | { event: "lagged"; data: StreamLaggedData };
