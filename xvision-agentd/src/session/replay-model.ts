/**
 * buildReplayModel — generalizes mock-provider.ts to serve recorded frames.
 *
 * Takes a recorded TrajectoryFrame sequence and returns an AgentModel whose
 * stream() replays the non-Request frames as AgentModelEvents in order.
 *
 * Replay determinism (Stage 3, Item 1):
 *   - Each stream() call advances a per-call cursor (identical to mock-provider.ts)
 *     through the recorded steps (groups of frames between Request frames).
 *   - Zero network: no provider or gateway is involved.
 *
 * Frame exhaustion (Stage 3, Item 4):
 *   - If stream() is called more times than there are recorded turns,
 *     ReplayExhaustedError is thrown. The caller must treat this as a corrupt
 *     recording and abort the run rather than falling back to a live provider.
 *
 * Frame → AgentModelEvent mapping:
 *   - Request        → skipped (marks the boundary between turns, not yielded)
 *   - TextDelta      → { type: "text-delta", text }
 *   - ReasoningDelta → { type: "reasoning-delta", text }
 *   - ToolCallDelta  → { type: "tool-call-delta", toolCallId?, toolName?, input? }
 *   - ToolResult     → skipped (tool results come from tool execution, not the model)
 *   - Usage          → { type: "usage", usage: { inputTokens, outputTokens, ... } }
 *   - RetryOrCancel  → skipped (control-flow frame, not a model output event)
 *   - Finish         → { type: "finish", reason, error? }
 */

import type { TrajectoryFrame } from "./frame-types.js"
import type { AgentModel, AgentModelRequest } from "./model-wrapper.js"

// Re-export so callers can catch by type.
export class ReplayExhaustedError extends Error {
  constructor(public readonly turn: number, public readonly totalTurns: number) {
    super(
      `Replay exhausted: turn ${turn} requested but recording only has ${totalTurns} turn(s). ` +
        "Recording is corrupt — refusing to fall back to a live provider.",
    )
    this.name = "ReplayExhaustedError"
  }
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

/**
 * Split a flat frame sequence into per-turn groups.
 *
 * A "turn" starts at a Request frame (or the beginning if the first frame
 * is not a Request) and ends just before the next Request frame.
 * Request frames themselves are excluded from the yielded events.
 *
 * If there are no frames, returns an empty array.
 */
function splitIntoTurns(frames: TrajectoryFrame[]): TrajectoryFrame[][] {
  const turns: TrajectoryFrame[][] = []
  let current: TrajectoryFrame[] = []

  for (const frame of frames) {
    if (frame.kind === "Request") {
      // Starting a new turn boundary. If we have accumulated frames from a
      // previous turn, close it off. Then start fresh.
      if (current.length > 0) {
        turns.push(current)
        current = []
      }
      // Request frame itself is not added to any turn's event list.
    } else {
      current.push(frame)
    }
  }

  // Push the last turn if non-empty.
  if (current.length > 0) {
    turns.push(current)
  }

  return turns
}

/**
 * Convert a single recorded frame into its corresponding AgentModelEvent.
 *
 * Returns `null` for frame kinds that are not model output events
 * (ToolResult, RetryOrCancel).
 */
function frameToEvent(frame: TrajectoryFrame): AgentModelEvent | null {
  switch (frame.kind) {
    case "TextDelta":
      return { type: "text-delta", text: frame.text }

    case "ReasoningDelta":
      return { type: "reasoning-delta", text: frame.text }

    case "ToolCallDelta":
      return {
        type: "tool-call-delta",
        ...(frame.tool_call_id !== undefined ? { toolCallId: frame.tool_call_id } : {}),
        ...(frame.tool_name !== undefined ? { toolName: frame.tool_name } : {}),
        ...(frame.input !== undefined ? { input: frame.input } : {}),
      }

    case "Usage":
      return {
        type: "usage",
        usage: {
          inputTokens: frame.input_tokens,
          outputTokens: frame.output_tokens,
          cacheReadTokens: frame.cache_read_tokens,
          cacheWriteTokens: frame.cache_write_tokens,
          totalCost: frame.total_cost,
        },
      }

    case "Finish":
      return {
        type: "finish",
        reason: frame.reason,
        ...(frame.error !== undefined ? { error: frame.error } : {}),
      }

    // These are control-flow frames, not model output events.
    case "ToolResult":
    case "RetryOrCancel":
    case "Request":
      return null
  }
}

/**
 * Build an AgentModel that replays a recorded TrajectoryFrame sequence.
 *
 * stream() is called once per agent turn. On each call, the model yields
 * the AgentModelEvents corresponding to that turn's frames from the recording.
 *
 * If the agent calls stream() more times than turns were recorded,
 * ReplayExhaustedError is thrown — never a live provider call.
 */
export function buildReplayModel(frames: TrajectoryFrame[]): AgentModel {
  const turns = splitIntoTurns(frames)
  let cursor = 0

  return {
    async *stream(_request: AgentModelRequest): AsyncIterable<AgentModelEvent> {
      const turnIndex = cursor++

      if (turnIndex >= turns.length) {
        throw new ReplayExhaustedError(turnIndex, turns.length)
      }

      const turnFrames = turns[turnIndex]!
      for (const frame of turnFrames) {
        const event = frameToEvent(frame)
        if (event !== null) {
          yield event
        }
      }
    },
  }
}
