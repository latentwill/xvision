/**
 * FrameRecorder — converts AgentModelEvents (and the per-step request frame
 * and per-tool ToolResult frames) into TrajectoryFrame values and emits them
 * via `emitFrame`.
 *
 * Frames are emitted in order of occurrence with monotonically non-decreasing
 * tsMs. The recorder does NOT drop frames — a missing frame invalidates the
 * recording (Stage 3 replay cannot recover from gaps).
 *
 * Usage:
 *   const recorder = createFrameRecorder()
 *   recorder.recordRequest(request)
 *   for await (const ev of stream) {
 *     recorder.recordEvent(ev)
 *     yield ev
 *   }
 *   // tool results are recorded by tool-shim.ts via recorder.recordToolResult
 */

import type { AgentModelRequest } from "./model-wrapper.js"
import { emitFrame } from "./emit.js"
import type {
  TrajectoryFrame,
  ToolResultFrame,
  RequestFrame,
  TextDeltaFrame,
  ReasoningDeltaFrame,
  ToolCallDeltaFrame,
  UsageFrame,
  FinishFrame,
} from "./frame-types.js"

// AgentModelEvent structural mirror — same as model-wrapper.ts.
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

export interface FrameRecorder {
  /**
   * Record the Request frame BEFORE the first event from stream().
   * Must be called exactly once per stream() invocation.
   */
  recordRequest(request: AgentModelRequest): void

  /**
   * Record a single AgentModelEvent. Call for every event yielded by the
   * inner model stream, in order.
   */
  recordEvent(ev: AgentModelEvent): void

  /**
   * Record a ToolResult frame. Called by tool-shim.ts after the Rust callback
   * returns (or throws), before the result is returned to Cline.
   */
  recordToolResult(toolCallId: string, output: unknown, error?: string): void
}

/** Create a FrameRecorder. One recorder is used per stream() invocation. */
export function createFrameRecorder(): FrameRecorder {
  // lastTs ensures non-decreasing tsMs even if two frames land at the same
  // wall-clock millisecond.
  let lastTs = 0

  function now(): number {
    const t = Date.now()
    if (t > lastTs) {
      lastTs = t
    } else {
      lastTs += 1
    }
    return lastTs
  }

  function emit(frame: TrajectoryFrame): void {
    emitFrame(frame)
  }

  return {
    recordRequest(request: AgentModelRequest): void {
      const frame: RequestFrame = {
        kind: "Request",
        ts_ms: now(),
        messages: request.messages as unknown,
        tools: (request.tools ?? []) as unknown,
        ...(request.systemPrompt !== undefined
          ? { system_prompt: request.systemPrompt }
          : {}),
      }
      emit(frame)
    },

    recordEvent(ev: AgentModelEvent): void {
      const tsMs = now()

      switch (ev.type) {
        case "text-delta": {
          const frame: TextDeltaFrame = {
            kind: "TextDelta",
            ts_ms: tsMs,
            text: ev.text,
          }
          emit(frame)
          break
        }

        case "reasoning-delta": {
          const frame: ReasoningDeltaFrame = {
            kind: "ReasoningDelta",
            ts_ms: tsMs,
            text: (ev as { type: "reasoning-delta"; text: string }).text,
          }
          emit(frame)
          break
        }

        case "tool-call-delta": {
          const tcd = ev as {
            type: "tool-call-delta"
            toolCallId?: string
            toolName?: string
            input?: unknown
          }
          const frame: ToolCallDeltaFrame = {
            kind: "ToolCallDelta",
            ts_ms: tsMs,
            ...(tcd.toolCallId !== undefined ? { tool_call_id: tcd.toolCallId } : {}),
            ...(tcd.toolName !== undefined ? { tool_name: tcd.toolName } : {}),
            ...(tcd.input !== undefined ? { input: tcd.input } : {}),
          }
          emit(frame)
          break
        }

        case "usage": {
          const u = (
            ev as {
              type: "usage"
              usage: {
                inputTokens?: number
                outputTokens?: number
                cacheReadTokens?: number
                cacheWriteTokens?: number
                totalCost?: number
              }
            }
          ).usage
          const frame: UsageFrame = {
            kind: "Usage",
            ts_ms: tsMs,
            input_tokens: u.inputTokens ?? 0,
            output_tokens: u.outputTokens ?? 0,
            cache_read_tokens: u.cacheReadTokens ?? 0,
            cache_write_tokens: u.cacheWriteTokens ?? 0,
            total_cost: u.totalCost ?? 0,
          }
          emit(frame)
          break
        }

        case "finish": {
          const fe = ev as { type: "finish"; reason: string; error?: string }
          const frame: FinishFrame = {
            kind: "Finish",
            ts_ms: tsMs,
            reason: fe.reason,
            ...(fe.error !== undefined ? { error: fe.error } : {}),
          }
          emit(frame)
          break
        }

        default:
          // Unknown event type — skip silently; new SDK event types should
          // not break recording of known ones.
          break
      }
    },

    recordToolResult(toolCallId: string, output: unknown, error?: string): void {
      const frame: ToolResultFrame = {
        kind: "ToolResult",
        ts_ms: now(),
        tool_call_id: toolCallId,
        output,
        ...(error !== undefined ? { error } : {}),
      }
      emit(frame)
    },
  }
}
