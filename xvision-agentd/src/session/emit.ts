import { createHash, randomUUID } from "node:crypto"
import { emitNotification } from "../transport/event-client.js"

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
  AssistantTextDelta: "event.assistant_text_delta",
  Overloaded: "event.overloaded",
  Error: "event.error",
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
}): void {
  void emitNotification(NOTIFY.RunStarted, {
    run_id: params.run_id,
    objective: params.objective,
    started_at_ms: params.started_at_ms,
    provider_id: params.provider_id,
    model_id: params.model_id,
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
}): void {
  void emitNotification(NOTIFY.ModelCallFinished, {
    span_id: params.span_id,
    run_id: params.run_id,
    provider: params.provider,
    model: params.model,
    input_tokens: params.input_tokens,
    output_tokens: params.output_tokens,
    ...(typeof params.total_cost === "number" ? { total_cost: params.total_cost } : {}),
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
}): void {
  void emitNotification(NOTIFY.AssistantTextDelta, params)
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
