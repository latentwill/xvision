import { createHash, randomUUID } from "node:crypto"
import { emitNotification } from "../transport/event-client.js"
import type { TrajectoryFrame } from "./frame-types.js"

/**
 * Notification methods sent from the sidecar to the Rust client.
 *
 * Names must match what `xvision-agent-client` accepts on the event
 * socket. Keep this enum in sync with `RunEventSink::dispatch` in the
 * Rust crate.
 */
export const NOTIFY = {
  RunStarted: "event.run_started",
  RunFinished: "event.run_finished",
  ToolCallStarted: "event.tool_call_started",
  ToolCallFinished: "event.tool_call_finished",
  ToolCallFailed: "event.tool_call_failed",
  ToolCallCancelled: "event.tool_call_cancelled",
  ModelCallStarted: "event.model_call_started",
  ModelCallFinished: "event.model_call_finished",
  DecisionRecorded: "event.decision_recorded",
  AssistantTextDelta: "event.assistant_text_delta",
  Overloaded: "event.overloaded",
  Error: "event.error",
  TrajectoryFrame: "event.trajectory_frame",
} as const

export function newSpanId(): string {
  return `sp-${randomUUID()}`
}

/** SHA-256 hex of a stable JSON serialization. Used for input/output hashes. */
export function hashJson(value: unknown): string {
  const json = JSON.stringify(value ?? null)
  return createHash("sha256").update(json).digest("hex")
}

export function emitRunStarted(params: {
  run_id: string
  objective: string
  started_at_ms: number
  provider_id: string
  model_id: string
  trajectory_mode?: "record" | "live" | "replay"
}): void {
  void emitNotification(NOTIFY.RunStarted, {
    run_id: params.run_id,
    objective: params.objective,
    started_at_ms: params.started_at_ms,
    provider_id: params.provider_id,
    model_id: params.model_id,
    ...(params.trajectory_mode !== undefined ? { trajectory_mode: params.trajectory_mode } : {}),
  })
}

export function emitRunFinished(params: {
  run_id: string
  status: "completed" | "failed" | "cancelled"
  finished_at_ms: number
  error?: string | undefined
}): void {
  void emitNotification(NOTIFY.RunFinished, {
    run_id: params.run_id,
    status: params.status,
    finished_at_ms: params.finished_at_ms,
    ...(params.error !== undefined ? { error: params.error } : {}),
  })
}

export function emitToolCallStarted(params: {
  span_id: string
  run_id: string
  tool_name: string
  input_hash: string
}): void {
  void emitNotification(NOTIFY.ToolCallStarted, params)
}

export function emitToolCallFinished(params: {
  span_id: string
  run_id: string
  output_hash: string
}): void {
  void emitNotification(NOTIFY.ToolCallFinished, params)
}

export function emitToolCallFailed(params: {
  span_id: string
  run_id: string
  error: string
}): void {
  void emitNotification(NOTIFY.ToolCallFailed, params)
}

export function emitModelCallFinished(params: {
  span_id: string
  run_id: string
  provider: string
  model: string
  input_tokens: number
  output_tokens: number
  total_cost?: number | undefined
  prompt: string
  response: string
}): void {
  void emitNotification(NOTIFY.ModelCallFinished, {
    span_id: params.span_id,
    run_id: params.run_id,
    provider: params.provider,
    model: params.model,
    input_tokens: params.input_tokens,
    output_tokens: params.output_tokens,
    ...(typeof params.total_cost === "number" ? { total_cost: params.total_cost } : {}),
    prompt: params.prompt,
    response: params.response,
  })
}

export function emitToolCallCancelled(params: {
  span_id: string
  run_id: string
  reason?: string | undefined
}): void {
  void emitNotification(NOTIFY.ToolCallCancelled, {
    span_id: params.span_id,
    run_id: params.run_id,
    ...(params.reason !== undefined ? { reason: params.reason } : {}),
  })
}

export function emitModelCallStarted(params: {
  span_id: string
  run_id: string
  provider: string
  model: string
}): void {
  void emitNotification(NOTIFY.ModelCallStarted, params)
}

export function emitAssistantTextDelta(params: {
  span_id: string
  run_id: string
  delta_len: number
  text?: string | undefined
}): void {
  void emitNotification(NOTIFY.AssistantTextDelta, {
    span_id: params.span_id,
    run_id: params.run_id,
    delta_len: params.delta_len,
    ...(params.text !== undefined ? { text: params.text } : {}),
  })
}

export function emitDecisionRecorded(params: {
  span_id: string
  run_id: string
  action: string
  outcome: "bought" | "sold" | "closed" | "held" | "unknown"
  asset?: string | undefined
  active_positions?: unknown
  decision_json: string
}): void {
  void emitNotification(NOTIFY.DecisionRecorded, {
    span_id: params.span_id,
    run_id: params.run_id,
    action: params.action,
    outcome: params.outcome,
    ...(params.asset !== undefined ? { asset: params.asset } : {}),
    ...(params.active_positions !== undefined ? { active_positions: params.active_positions } : {}),
    decision_json: params.decision_json,
  })
}

/**
 * Emitted by `event-client.ts` when the outbound notification socket's
 * `writableLength` crosses the configured high-water mark, and again when
 * it drains below 50% of that mark. Best-effort: if no active run is
 * known we still emit with `run_id: ""` so the dispatch path can record
 * the warning against the bus's "unknown run" fallback.
 */
export function emitOverloaded(params: {
  run_id: string
  dropped: number
  note: string
}): void {
  void emitNotification(NOTIFY.Overloaded, params)
}

export function emitError(params: { run_id: string; message: string; severity: "info" | "warn" | "error" }): void {
  void emitNotification(NOTIFY.Error, params)
}

/**
 * Coordinate envelope wrapping one trajectory frame for the
 * `event.trajectory_frame` notification.
 *
 * Mirrors `ParsedTrajectoryFrame` / `parse_trajectory_frame_notification` in
 * `crates/xvision-agent-client/src/event_sink.rs`. The Rust parser requires
 * ALL of `run_id`, `slot_role`, `step_index`, `frame_index` and the frame body
 * under the `frame` key; a payload missing any of them parses to `None` and is
 * silently dropped. Keep this shape byte-for-byte in sync with that parser.
 */
export interface TrajectoryFrameEnvelope {
  run_id: string
  slot_role: string
  step_index: number
  frame_index: number
  frame: TrajectoryFrame
}

/**
 * Emit a single trajectory frame over the notification socket.
 *
 * Frames are non-droppable (a missing frame invalidates the recording), so
 * this always emits — unlike the lossy observability ring that can drop events
 * under pressure. The Rust consumer routes `event.trajectory_frame`
 * notifications to the bounded `FrameChannel` which applies backpressure
 * (blocks the producer) rather than dropping.
 *
 * The frame travels inside a coordinate envelope so the Rust consumer can
 * route it to the correct `RecordingId` at the correct `(slot_role,
 * step_index, frame_index)` position. `run_id` comes from `activeRunId()`;
 * `slot_role` / `step_index` / `frame_index` are threaded by the
 * `FrameRecorder` (see `frame-recorder.ts`).
 */
export function emitFrame(envelope: TrajectoryFrameEnvelope): void {
  void emitNotification(NOTIFY.TrajectoryFrame, envelope)
}
