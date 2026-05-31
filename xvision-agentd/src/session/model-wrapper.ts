/**
 * AgentModel forwarding wrapper.
 *
 * Wraps the underlying `AgentModel` passed to `buildAgent` (mock or real
 * provider) so each `stream()` invocation can:
 *   - Emit `event.model_call_started` before the first downstream event
 *     (per-iteration span boundary, replacing the v1 synthesized pair).
 *   - Emit `event.assistant_text_delta` for each `text-delta` event,
 *     carrying span_id + run_id + `delta_len` (payload stays in memory).
 *   - Accumulate token usage from `usage` events.
 *   - Emit `event.model_call_finished` on the terminal `finish` event,
 *     attached to the per-stream span_id.
 *
 * The wrapper re-yields every event from the inner model unchanged so
 * the Cline runtime sees the same stream it would without us in the
 * middle.
 *
 * Concurrency: identical to `active-run.ts` — the sidecar serializes
 * step requests, so one `stream()` is in flight at a time per session.
 * If Cline's runtime ever calls `stream()` concurrently inside one step,
 * each call still gets its own local `span_id` / usage accumulator
 * because they live inside the async generator's closure.
 */

import {
  emitAssistantTextDelta,
  emitModelCallStarted,
  emitModelCallFinished,
  newSpanId,
} from "./emit.js"
import { activeRunId } from "./active-run.js"
import type { FrameRecorder } from "./frame-recorder.js"

// Structural mirror of @cline/shared agent.d.ts — same approach as
// mock-provider.ts. We can't import the types directly because the
// package's `export *` re-exports without `.js` extensions, which our
// `NodeNext` resolution rejects.
export interface AgentModelRequest {
  messages: readonly unknown[]
  tools?: readonly unknown[]
  systemPrompt?: string
  signal?: AbortSignal
  [extra: string]: unknown
}

type AgentModelEvent =
  | { type: "text-delta"; text: string }
  | { type: "reasoning-delta"; text: string }
  | {
      type: "tool-call-delta"
      toolCallId?: string
      toolName?: string
      input?: unknown
    }
  | {
      type: "usage"
      usage: {
        inputTokens?: number
        outputTokens?: number
        cacheReadTokens?: number
        cacheWriteTokens?: number
        totalCost?: number
      }
    }
  | { type: "finish"; reason: string; error?: string }

type CapturedToolCall = {
  tool_call_id?: string
  tool_name?: string
  input?: unknown
}

export interface AgentModel {
  stream(
    request: AgentModelRequest,
  ): AsyncIterable<AgentModelEvent> | Promise<AsyncIterable<AgentModelEvent>>
}

export interface WrapModelOptions {
  /** Provider id stamped onto emitted events. */
  provider: string
  /** Model id stamped onto emitted events. */
  model: string
  /**
   * When provided, each stream() invocation will record a Request frame
   * before the first event and one frame per AgentModelEvent via the recorder.
   * ToolResult frames are recorded by tool-shim.ts using the same recorder
   * instance; pass the same recorder to shimRegistryToTools.
   */
  recorder?: FrameRecorder
}

/**
 * Return a fresh `AgentModel` that forwards `stream()` to `inner` while
 * tapping each event for observability emission.
 *
 * If `activeRunId()` is undefined when `stream()` is called (no run is
 * currently in flight from the sidecar's perspective), the wrapper
 * silently forwards events without emission. This protects tests that
 * exercise the model directly without going through `session.step`.
 */
export function wrapAgentModel(
  inner: AgentModel,
  opts: WrapModelOptions,
): AgentModel {
  return {
    async *stream(request: AgentModelRequest): AsyncIterable<AgentModelEvent> {
      const runId = activeRunId()
      const spanId = newSpanId()
      let emittedStart = false

      let inputTokens = 0
      let outputTokens = 0
      let totalCost: number | undefined
      const promptText = serializeModelPrompt(request)
      const responseCapture = createResponseCapture()

      // Record the Request frame BEFORE the first downstream event so
      // replay can reconstruct the full input side of this step.
      if (opts.recorder) {
        opts.recorder.recordRequest(request)
      }

      const innerStream = await inner.stream(request)
      try {
        for await (const ev of innerStream) {
          if (!emittedStart && runId) {
            emitModelCallStarted({
              span_id: spanId,
              run_id: runId,
              provider: opts.provider,
              model: opts.model,
            })
            emittedStart = true
          }

          if (ev.type === "text-delta" && runId) {
            const text = (ev as { text: string }).text
            responseCapture.text += text
            emitAssistantTextDelta({
              span_id: spanId,
              run_id: runId,
              delta_len: text.length,
              text,
            })
          } else if (ev.type === "reasoning-delta") {
            responseCapture.reasoning += (ev as { text: string }).text
          } else if (ev.type === "tool-call-delta") {
            responseCapture.tool_calls.push(compactToolCallDelta(ev))
          } else if (ev.type === "usage") {
            const u = (ev as { usage: { inputTokens?: number; outputTokens?: number; totalCost?: number } }).usage
            if (typeof u.inputTokens === "number") inputTokens += u.inputTokens
            if (typeof u.outputTokens === "number") outputTokens += u.outputTokens
            if (typeof u.totalCost === "number") {
              totalCost = (totalCost ?? 0) + u.totalCost
            }
          } else if (ev.type === "finish") {
            responseCapture.finish_reason = ev.reason
            if (ev.error !== undefined) responseCapture.error = ev.error
          }

          // Record the event frame BEFORE yielding so the recorder sees
          // events in the same order the consumer does. Errors in recording
          // must not suppress the event — yield regardless.
          if (opts.recorder) {
            opts.recorder.recordEvent(ev)
          }

          yield ev
        }
      } finally {
        // Emit finish per stream() call, regardless of how the iterator
        // terminated (normal completion, abort, or thrown error).
        // Without this, an early-abort path would leave the span open
        // forever on the Rust side. Only emit if we actually emitted
        // start so spans always come in pairs.
        if (emittedStart && runId) {
          emitModelCallFinished({
            span_id: spanId,
            run_id: runId,
            provider: opts.provider,
            model: opts.model,
            input_tokens: inputTokens,
            output_tokens: outputTokens,
            ...(totalCost !== undefined ? { total_cost: totalCost } : {}),
            prompt: promptText,
            response: serializeModelResponse(responseCapture),
          })
        }
      }
    },
  }
}

function serializeModelPrompt(request: AgentModelRequest): string {
  const { signal: _signal, ...serializable } = request
  return stableStringify(serializable)
}

function createResponseCapture(): {
  text: string
  reasoning: string
  tool_calls: CapturedToolCall[]
  finish_reason?: string
  error?: string
} {
  return {
    text: "",
    reasoning: "",
    tool_calls: [],
  }
}

function compactToolCallDelta(ev: {
  toolCallId?: string
  toolName?: string
  input?: unknown
}): CapturedToolCall {
  return {
    ...(ev.toolCallId !== undefined ? { tool_call_id: ev.toolCallId } : {}),
    ...(ev.toolName !== undefined ? { tool_name: ev.toolName } : {}),
    ...(ev.input !== undefined ? { input: ev.input } : {}),
  }
}

function serializeModelResponse(capture: {
  text: string
  reasoning: string
  tool_calls: CapturedToolCall[]
  finish_reason?: string
  error?: string
}): string {
  return stableStringify({
    text: capture.text,
    reasoning: capture.reasoning,
    tool_calls: capture.tool_calls,
    ...(capture.finish_reason !== undefined ? { finish_reason: capture.finish_reason } : {}),
    ...(capture.error !== undefined ? { error: capture.error } : {}),
  })
}

function stableStringify(value: unknown): string {
  return JSON.stringify(value, (_key, v) => {
    if (typeof v === "bigint") return v.toString()
    if (v instanceof Error) return { name: v.name, message: v.message }
    return v
  })
}
