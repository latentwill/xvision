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
import { activeRunId } from "./active-run.js"
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
   * Advance to the next step (one `session.step` invocation). Bumps
   * `step_index` so frames from this step land in their own
   * `(recording_id, slot_role, step_index)` group on the Rust side. Call
   * once per `session.step` before the agent runs.
   */
  beginStep(): void

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

export interface FrameRecorderOptions {
  /**
   * The slot role this recording is for (e.g. "trader"). Stamped on every
   * emitted frame envelope as `slot_role` so the Rust consumer keys frames
   * to the matching `TrajectoryKey.slot_role`. Free-form per the terminology
   * lock (slot names are user-defined). Defaults to `"default"` when the
   * caller does not supply one.
   */
  slotRole?: string
}

/**
 * Create a FrameRecorder for one recording (one run). The same recorder
 * instance is shared across all steps + model calls of the run; `step_index`
 * is bumped per `session.step` via `beginStep()` and `frame_index` is
 * monotonic across the whole recording, so every emitted frame has a unique,
 * ordered `(step_index, frame_index)` coordinate.
 */
export function createFrameRecorder(opts: FrameRecorderOptions = {}): FrameRecorder {
  const slotRole = opts.slotRole ?? "default"
  // step_index starts at -1 so the first beginStep() lands on step 0. If a
  // caller records a frame before any beginStep() (defensive), it is stamped
  // step 0 via the clamp in emit().
  let stepIndex = -1
  // frame_index is monotonic across the entire recording (never reset). The
  // Rust store PK is (recording_id, slot_role, step_index, frame_index), so a
  // globally-monotonic frame_index is trivially unique+ordered within any
  // step group as well.
  let frameIndex = 0

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
    emitFrame({
      // run_id is read at emit time from the module-local active-run
      // registry (set by session.step). Empty string when no run is active
      // (defensive — recording is only enabled inside an active step).
      run_id: activeRunId() ?? "",
      slot_role: slotRole,
      step_index: stepIndex < 0 ? 0 : stepIndex,
      frame_index: frameIndex++,
      frame,
    })
  }

  return {
    beginStep(): void {
      stepIndex += 1
    },

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
