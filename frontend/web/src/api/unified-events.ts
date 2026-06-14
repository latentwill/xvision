// frontend/web/src/api/unified-events.ts
//
// Hand-authored TypeScript mirror of the Rust UnifiedEvent envelope.
//
// SOURCE OF TRUTH: crates/xvision-observability/src/unified_event.rs
//   - `UnifiedEvent`   — the envelope (event_id, session_id, run_id, span_id,
//     parent_event_id, seq, ts, scope, actor, source, blob_hash, payload).
//   - `UnifiedPayload` — `#[serde(tag = "kind", content = "data",
//     rename_all = "snake_case")]` (ADJACENT tagging), so each variant is
//     `{ kind, data }`: `kind` is the discriminant whose VALUE matches the
//     verbatim snake_case strings in `payload_event_name()`, and `data` holds
//     the variant's fields. UNIT variants (no fields) serialize as `{ kind }`
//     with NO `data` key.
//   - `Actor` / `EventSource` / `EventScope` / `ToolPolicyOutcome` enums.
//
// There is NO ts-rs generation for the observability crate — this file is
// hand-maintained. When the Rust enum changes, update this mirror and the
// per-row reducer (`stores/message-row-reducer.ts`) together. The `kind`
// string literals below are the contract; keep them byte-for-byte equal to
// the Rust serde tags.
//
// Detail payload structs (ToolCallStartedEvent, ToolCallFinishedEvent, …) are
// reused from the agent-run vocabulary (`crates/xvision-observability/src/events.rs`).
// Fields the frontend does not yet consume are typed loosely (nullable /
// optional) so the recorder stays the canonical shape and this mirror does
// not create drift.

// ─── Envelope sub-types ──────────────────────────────────────────────────

/** Who or what produced the event. Mirrors Rust `Actor` (snake_case). */
export type Actor = "operator" | "agent" | "system" | "hook" | "optimizer";

/** Which surface emitted the event. Mirrors Rust `EventSource` (snake_case). */
export type EventSource =
  | "chat_rail"
  | "agent_run"
  | "engine"
  | "optimizer"
  | "hook";

/**
 * The scope an event is attached to. Mirrors Rust `EventScope` — a flat
 * `(kind, id)` pair. `kind` is the snake_case scope discriminant
 * (`workspace`, `run`, `strategy`, …); `id` is present when the scope names
 * one. Rust skips `id` when `None`, so it may be absent or null on the wire.
 */
export type EventScope = {
  kind: string;
  id?: string | null;
};

/** Outcome of a server-side tool-policy check. Mirrors `ToolPolicyOutcome`. */
export type ToolPolicyOutcome = "auto_approved" | "needs_approval" | "denied";

// ─── Reused agent-run detail structs ──────────────────────────────────────
// (crates/xvision-observability/src/events.rs)

export type RunStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted"
  | "agent_failure";

export type SpanStatus = "ok" | "error" | "cancelled" | "interrupted";

/** Mirrors `RunStartedEvent`. */
export type RunStartedEvent = {
  run_id: string;
  objective: string;
  strategy_id: string | null;
  eval_run_id: string | null;
  source_cli_job_id: string | null;
  started_at: string; // ISO (DateTime<Utc>)
  retention_mode: string;
  sidecar_version: string | null;
  cline_sdk_version: string | null;
  protocol_version: string | null;
  skills_json: string | null;
  mcp_servers_json: string | null;
};

/** Mirrors `RunFinishedEvent`. */
export type RunFinishedEvent = {
  run_id: string;
  finished_at: string;
  status: RunStatus;
  final_artifact_id: string | null;
  error: string | null;
};

/** Mirrors `RunInterruptedEvent`. */
export type RunInterruptedEvent = {
  run_id: string;
  finished_at: string;
  reason: string;
};

/** Mirrors `SpanStartedEvent`. */
export type SpanStartedEvent = {
  span_id: string;
  run_id: string;
  parent_span_id: string | null;
  kind: string;
  name: string;
  started_at: string;
  otel_trace_id: string | null;
  otel_span_id: string | null;
  attributes_json: string | null;
};

/** Mirrors `SpanFinishedEvent`. */
export type SpanFinishedEvent = {
  span_id: string;
  ended_at: string;
  status: SpanStatus;
  error_json: string | null;
};

/** Mirrors `ModelCallFinishedEvent`. */
export type ModelCallFinishedEvent = {
  span_id: string;
  provider: string;
  model: string;
  input_token_count: number | null;
  output_token_count: number | null;
  cost_usd: number | null;
  prompt_hash: string;
  response_hash: string | null;
  prompt_payload_ref: string | null;
  response_payload_ref: string | null;
  tool_calls_requested: string | null;
  capability_path: string | null;
};

/** Mirrors `ToolCallStartedEvent`. `origin` is the externally-tagged Rust
 * `ToolOrigin` enum: `"Native"` | `"ClineBuiltin"` | `{ "Mcp": "<server>" }`. */
export type ToolOrigin = "Native" | "ClineBuiltin" | { Mcp: string };

export type ToolCallStartedEvent = {
  span_id: string;
  tool_name: string;
  origin: ToolOrigin;
  tool_version: string | null;
  tool_hash: string | null;
  side_effect_level: string;
  risk_level: string;
  requires_approval: boolean;
  is_run_terminator: boolean;
  input_hash: string;
  input_payload_ref: string | null;
};

/** Mirrors `ToolCallFinishedEvent`. */
export type ToolCallFinishedEvent = {
  span_id: string;
  output_hash: string | null;
  output_payload_ref: string | null;
  exit_code: number | null;
};

/** Mirrors `ToolCallFailedEvent`. */
export type ToolCallFailedEvent = {
  span_id: string;
  error_json: string | null;
};

/** Mirrors `ToolCallCancelledEvent`. */
export type ToolCallCancelledEvent = {
  span_id: string;
  reason: string | null;
};

/** Mirrors `BrokerCallStartedEvent`. */
export type BrokerCallStartedEvent = {
  span_id: string;
  run_id: string;
  side: string;
  symbol: string;
  qty: number;
  intended_price: number | null;
  order_type: string;
  venue: string;
  idempotency_key: string | null;
};

/** Mirrors `BrokerCallFinishedEvent`. */
export type BrokerCallFinishedEvent = {
  span_id: string;
  outcome: string;
  fill_price: number | null;
  fill_qty: number | null;
  fee: number | null;
  broker_order_id: string | null;
  error_class: string | null;
  error_message: string | null;
  severity?: string | null;
};

/** Mirrors `CheckpointWrittenEvent`. */
export type CheckpointWrittenEvent = {
  checkpoint_id: string;
  run_id: string;
  span_id: string;
  sequence: number;
  kind: string;
  input_hash: string;
  output_hash: string | null;
  input_payload_ref: string | null;
  output_payload_ref: string | null;
};

/** Mirrors `MemoryRecallEvent`. */
export type MemoryRecallItem = {
  id: string;
  score: number;
  text_preview: string;
};
export type MemoryRecallEvent = {
  run_id: string;
  /** `<run_id>:<decision_id>` flywheel correlation key — optional on older
   * persisted payloads (Rust `skip_serializing_if = "Option::is_none"`). */
  flywheel_cycle_id?: string | null;
  decision_id: number;
  namespace: string;
  items: MemoryRecallItem[];
};

/** Mirrors `MemoryWriteEvent`. */
export type MemoryWriteEvent = {
  run_id: string;
  flywheel_cycle_id?: string | null;
  decision_id: number;
  namespace: string;
  memory_item_id: string;
  text_preview: string;
};

/** Mirrors `ArtifactWrittenEvent`. */
export type ArtifactWrittenEvent = {
  artifact_id: string;
  run_id: string;
  kind: string;
  title: string | null;
  summary: string | null;
  hypothesis: string | null;
  recommendation: string | null;
  evidence_json: string | null;
  next_experiments_json: string | null;
  created_at: string;
};

/** Mirrors `SupervisorNoteEvent`. */
export type SupervisorNoteEvent = {
  run_id: string;
  role: string;
  content: string;
  severity: string;
  created_at: string;
};

/** Mirrors `EngineEvent`. */
export type EngineEvent = {
  run_id: string;
  span_id: string | null;
  kind: string;
  payload_json: string | null;
  created_at: string;
};

/** Mirrors `SidecarErrorEvent`. */
export type SidecarErrorEvent = {
  run_id: string;
  message: string;
  severity: string;
};

/** Mirrors `BackpressureDroppedEvent`. */
export type BackpressureDroppedEvent = {
  run_id: string;
  dropped: number;
  note: string;
};

// ─── Net-new unified payload detail structs ───────────────────────────────
// (crates/xvision-observability/src/unified_event.rs)

/** Mirrors `ToolPolicyChecked`. */
export type ToolPolicyChecked = {
  span_id: string;
  tool_name: string;
  outcome: ToolPolicyOutcome;
  /** `research` | `act` — mode in force when the check ran. */
  mode: string;
};

/** Mirrors `ToolDenied`. */
export type ToolDenied = {
  span_id: string;
  tool_name: string;
  /** Stable machine code, e.g. `write_tool_in_research_mode`. */
  code: string;
  message: string;
};

/** Mirrors `CheckpointRestored`. */
export type CheckpointRestored = {
  checkpoint_id: string;
  run_id: string | null;
  session_id: string | null;
  restored: string[];
};

/** Mirrors `CheckpointRestoreFailed`. */
export type CheckpointRestoreFailed = {
  checkpoint_id: string;
  code: string;
  message: string;
};

/** Mirrors `FocusEvent`. */
export type FocusEvent = {
  scope_kind: string;
  scope_id: string | null;
  path: string;
  content_hash: string | null;
};

/** Mirrors `OptimizationCandidate`. */
export type OptimizationCandidate = {
  optimization_id: string;
  candidate_index: number;
  optimizer: string;
};

/** Mirrors `OptimizationCandidateMetric`. */
export type OptimizationCandidateMetric = {
  optimization_id: string;
  candidate_index: number;
  metric: string;
  value: number;
  /** `train` | `holdout`. */
  split: string;
};

/** Mirrors `OptimizationCompleted`. */
export type OptimizationCompleted = {
  optimization_id: string;
  selected_candidate_index: number | null;
  minted_agent_id: string | null;
};

/** Mirrors `TypedError` — a never-silent, machine-coded error. */
export type TypedError = {
  /** Stable machine code (e.g. `missing_capability_optimizer`). */
  code: string;
  message: string;
  remediation?: string | null;
};

// ─── UnifiedPayload — adjacently-tagged union ({ kind, data }) ─────────────
//
// The `kind` literals below MUST match `payload_event_name()` in
// `crates/xvision-observability/src/unified_event.rs` byte-for-byte.
//
// Rust uses `#[serde(tag = "kind", content = "data")]` (ADJACENT tagging), so
// every variant that carries fields serializes as `{ kind, data }` with the
// variant's fields nested under `data`:
//   - Newtype variants (`RunStarted(RunStartedEvent)`)   → { kind, data: DetailStruct }
//   - Struct variants  (`AssistantTokenDelta { text }`)  → { kind, data: { text } }
// UNIT variants (no fields — `AssistantMessageStarted`, `SessionCompleted`)
// serialize as `{ kind }` with NO `data` key.

export type UnifiedPayload =
  // ── Session lifecycle (rail-originated) ──
  | { kind: "session_created"; data: { scope_label: string } }
  | { kind: "session_resumed"; data: { from_seq: number } }
  | { kind: "session_interrupted"; data: { reason: string } }
  | { kind: "session_completed" }
  | { kind: "session_failed"; data: { message: string } }

  // ── Run lifecycle (agent-run, reused from RunEvent) ──
  | { kind: "run_started"; data: RunStartedEvent }
  | { kind: "run_finished"; data: RunFinishedEvent }
  | { kind: "run_interrupted"; data: RunInterruptedEvent }
  | { kind: "span_started"; data: SpanStartedEvent }
  | { kind: "span_finished"; data: SpanFinishedEvent }
  | { kind: "model_call_finished"; data: ModelCallFinishedEvent }

  // ── Assistant output ──
  | { kind: "assistant_message_started" }
  | { kind: "assistant_token_delta"; data: { text: string } }
  | { kind: "assistant_content_block"; data: { block: unknown } }
  | { kind: "assistant_message_done"; data: { draft_id: string | null } }

  // ── Tool lifecycle ──
  | { kind: "tool_requested"; data: ToolCallStartedEvent }
  | { kind: "tool_policy_checked"; data: ToolPolicyChecked }
  | { kind: "tool_approved"; data: { span_id: string; approver: string } }
  | { kind: "tool_started"; data: { span_id: string } }
  | { kind: "tool_delta"; data: { span_id: string; text: string } }
  | { kind: "tool_finished"; data: ToolCallFinishedEvent }
  | { kind: "tool_failed"; data: ToolCallFailedEvent }
  | { kind: "tool_cancelled"; data: ToolCallCancelledEvent }
  | { kind: "tool_denied"; data: ToolDenied }

  // ── Broker (xvision-specific, reused) ──
  | { kind: "broker_call_started"; data: BrokerCallStartedEvent }
  | { kind: "broker_call_finished"; data: BrokerCallFinishedEvent }

  // ── Checkpoints ──
  | { kind: "checkpoint_created"; data: CheckpointWrittenEvent }
  | { kind: "checkpoint_restored"; data: CheckpointRestored }
  | { kind: "checkpoint_restore_failed"; data: CheckpointRestoreFailed }

  // ── Focus chain ──
  | { kind: "focus_loaded"; data: FocusEvent }
  | { kind: "focus_edited"; data: FocusEvent }
  | { kind: "focus_injected"; data: FocusEvent }

  // ── Optimization (offline; surfaced live in the rail) ──
  | { kind: "optimization_candidate_started"; data: OptimizationCandidate }
  | { kind: "optimization_candidate_metric"; data: OptimizationCandidateMetric }
  | { kind: "optimization_candidate_selected"; data: OptimizationCandidate }
  | { kind: "optimization_completed"; data: OptimizationCompleted }

  // ── Provenance / supervision (reused) ──
  | { kind: "memory_recall"; data: MemoryRecallEvent }
  | { kind: "memory_write"; data: MemoryWriteEvent }
  | { kind: "artifact_written"; data: ArtifactWrittenEvent }
  | { kind: "supervisor_note"; data: SupervisorNoteEvent }
  | { kind: "engine_event"; data: EngineEvent }

  // ── Errors (typed, never silent) ──
  | { kind: "error_missing_capability"; data: TypedError }
  | { kind: "error_missing_tool"; data: TypedError }
  | { kind: "error_invalid_schema"; data: TypedError }
  | { kind: "error_provider_unavailable"; data: TypedError }
  | { kind: "error_policy_denied"; data: TypedError }
  | { kind: "error_persistence_failed"; data: TypedError }
  | { kind: "sidecar_error"; data: SidecarErrorEvent }
  | { kind: "backpressure_dropped"; data: BackpressureDroppedEvent };

/** The kind discriminant union — every literal above. */
export type UnifiedPayloadKind = UnifiedPayload["kind"];

// ─── UnifiedEvent envelope ────────────────────────────────────────────────

/**
 * The unified event envelope. Every chat-rail row and every trace-dock row
 * is a projection of one of these. Mirrors Rust `UnifiedEvent`.
 *
 * `session_id` / `run_id` / `span_id` / `parent_event_id` / `blob_hash` are
 * `skip_serializing_if = "Option::is_none"` on the Rust side, so they may be
 * absent or null on the wire.
 */
export type UnifiedEvent = {
  /** Globally-unique id (ULID). Stable across reconnects → reducer dedupe key. */
  event_id: string;
  session_id?: string | null;
  run_id?: string | null;
  span_id?: string | null;
  parent_event_id?: string | null;
  /** Monotonic per-session sequence number. Reducer orders + dedupes on it. */
  seq: number;
  ts: string; // ISO (DateTime<Utc>)
  scope: EventScope;
  actor: Actor;
  source: EventSource;
  blob_hash?: string | null;
  payload: UnifiedPayload;
};

// ─── Narrowing helpers ────────────────────────────────────────────────────

/** Narrow a payload to a specific `kind`. */
export function isPayloadKind<K extends UnifiedPayloadKind>(
  payload: UnifiedPayload,
  kind: K,
): payload is Extract<UnifiedPayload, { kind: K }> {
  return payload.kind === kind;
}

/** The error_* kinds + sidecar_error — the set the reducer maps to error rows. */
export const ERROR_PAYLOAD_KINDS = [
  "error_missing_capability",
  "error_missing_tool",
  "error_invalid_schema",
  "error_provider_unavailable",
  "error_policy_denied",
  "error_persistence_failed",
  "sidecar_error",
] as const satisfies readonly UnifiedPayloadKind[];

export type ErrorPayloadKind = (typeof ERROR_PAYLOAD_KINDS)[number];

export function isErrorPayload(
  payload: UnifiedPayload,
): payload is Extract<UnifiedPayload, { kind: ErrorPayloadKind }> {
  return (ERROR_PAYLOAD_KINDS as readonly string[]).includes(payload.kind);
}
