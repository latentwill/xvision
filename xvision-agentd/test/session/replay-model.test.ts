/**
 * Tests for buildReplayModel (Stage 3, Task 1).
 *
 * Verifies:
 *   (a) A recorded frame sequence is replayed as ordered AgentModelEvents
 *       with no network call.
 *   (b) Non-model frames (ToolResult, RetryOrCancel) are skipped.
 *   (c) Multi-turn recordings advance the cursor correctly.
 *   (d) ReplayExhaustedError is thrown when stream() is called more times
 *       than recorded turns.
 */
import { describe, it, expect } from "vitest"
import { buildReplayModel, ReplayExhaustedError } from "../../src/session/replay-model.js"
import type { TrajectoryFrame } from "../../src/session/frame-types.js"

// Minimal typed helper so tests read as documented frame sequences.
function mkFrame<K extends TrajectoryFrame["kind"]>(
  kind: K,
  rest: Omit<Extract<TrajectoryFrame, { kind: K }>, "kind" | "ts_ms">,
): TrajectoryFrame {
  return { kind, ts_ms: Date.now(), ...rest } as TrajectoryFrame
}

describe("buildReplayModel", () => {
  // ---------------------------------------------------------------------------
  // Task 1 acceptance criterion: replays recorded frames as AgentModelEvents
  // ---------------------------------------------------------------------------

  it("replays recorded frames as AgentModelEvents in order (plan example)", async () => {
    const frames: TrajectoryFrame[] = [
      { kind: "Request", ts_ms: 1, messages: [], tools: [], system_prompt: "x" },
      { kind: "TextDelta", ts_ms: 2, text: "he" },
      {
        kind: "ToolCallDelta",
        ts_ms: 3,
        tool_name: "submit_decision",
        input: { action: "buy" },
      },
      {
        kind: "Usage",
        ts_ms: 4,
        input_tokens: 10,
        output_tokens: 2,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        total_cost: 0,
      },
      { kind: "Finish", ts_ms: 5, reason: "stop" },
    ]

    const model = buildReplayModel(frames)
    const out: string[] = []
    for await (const ev of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
      out.push(ev.type)
    }

    expect(out).toEqual(["text-delta", "tool-call-delta", "usage", "finish"])
  })

  it("yields text-delta events with correct text", async () => {
    const frames: TrajectoryFrame[] = [
      { kind: "Request", ts_ms: 1, messages: [], tools: [] },
      { kind: "TextDelta", ts_ms: 2, text: "hello world" },
      { kind: "Finish", ts_ms: 3, reason: "stop" },
    ]

    const model = buildReplayModel(frames)
    const events: { type: string; text?: string }[] = []
    for await (const ev of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
      events.push(ev as { type: string; text?: string })
    }

    expect(events[0]).toMatchObject({ type: "text-delta", text: "hello world" })
    expect(events[1]).toMatchObject({ type: "finish", reason: "stop" })
  })

  it("skips ToolResult and RetryOrCancel frames (control-flow, not model events)", async () => {
    const frames: TrajectoryFrame[] = [
      { kind: "Request", ts_ms: 1, messages: [], tools: [] },
      {
        kind: "ToolCallDelta",
        ts_ms: 2,
        tool_call_id: "tc-0",
        tool_name: "echo",
        input: { msg: "hi" },
      },
      { kind: "ToolResult", ts_ms: 3, tool_call_id: "tc-0", output: { echoed: "hi" } },
      { kind: "RetryOrCancel", ts_ms: 4, reason: "retry" },
      {
        kind: "Usage",
        ts_ms: 5,
        input_tokens: 5,
        output_tokens: 3,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        total_cost: 0.001,
      },
      { kind: "Finish", ts_ms: 6, reason: "tool-calls" },
    ]

    const model = buildReplayModel(frames)
    const types: string[] = []
    for await (const ev of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
      types.push(ev.type)
    }

    // ToolResult and RetryOrCancel are not model output events
    expect(types).not.toContain("tool-result")
    expect(types).not.toContain("retry-or-cancel")
    expect(types).toEqual(["tool-call-delta", "usage", "finish"])
  })

  it("maps Usage frame fields to camelCase AgentModelEvent usage", async () => {
    const frames: TrajectoryFrame[] = [
      { kind: "Request", ts_ms: 1, messages: [], tools: [] },
      {
        kind: "Usage",
        ts_ms: 2,
        input_tokens: 100,
        output_tokens: 200,
        cache_read_tokens: 30,
        cache_write_tokens: 40,
        total_cost: 0.05,
      },
      { kind: "Finish", ts_ms: 3, reason: "stop" },
    ]

    const model = buildReplayModel(frames)
    let usageEvent: unknown
    for await (const ev of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
      if (ev.type === "usage") usageEvent = ev
    }

    expect(usageEvent).toMatchObject({
      type: "usage",
      usage: {
        inputTokens: 100,
        outputTokens: 200,
        cacheReadTokens: 30,
        cacheWriteTokens: 40,
        totalCost: 0.05,
      },
    })
  })

  it("maps ReasoningDelta frames to reasoning-delta events", async () => {
    const frames: TrajectoryFrame[] = [
      { kind: "Request", ts_ms: 1, messages: [], tools: [] },
      { kind: "ReasoningDelta", ts_ms: 2, text: "thinking..." },
      { kind: "Finish", ts_ms: 3, reason: "stop" },
    ]

    const model = buildReplayModel(frames)
    const types: string[] = []
    for await (const ev of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
      types.push(ev.type)
    }

    expect(types).toContain("reasoning-delta")
  })

  it("handles multi-turn recording: cursor advances across stream() calls", async () => {
    // Turn 1: text delta
    // Turn 2: tool call
    const frames: TrajectoryFrame[] = [
      { kind: "Request", ts_ms: 1, messages: [], tools: [] },
      { kind: "TextDelta", ts_ms: 2, text: "turn-one" },
      { kind: "Finish", ts_ms: 3, reason: "stop" },
      { kind: "Request", ts_ms: 4, messages: [], tools: [] },
      {
        kind: "ToolCallDelta",
        ts_ms: 5,
        tool_call_id: "tc-1",
        tool_name: "submit_decision",
        input: { action: "sell" },
      },
      { kind: "Finish", ts_ms: 6, reason: "tool-calls" },
    ]

    const model = buildReplayModel(frames)

    // First call: turn 0
    const turn0: string[] = []
    for await (const ev of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
      turn0.push(ev.type)
    }
    expect(turn0).toEqual(["text-delta", "finish"])

    // Second call: turn 1
    const turn1: string[] = []
    for await (const ev of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
      turn1.push(ev.type)
    }
    expect(turn1).toEqual(["tool-call-delta", "finish"])
  })

  it("works with no leading Request frame (frames before first Request)", async () => {
    // If recording starts mid-stream (no leading Request), the frames are
    // treated as turn 0.
    const frames: TrajectoryFrame[] = [
      { kind: "TextDelta", ts_ms: 1, text: "no-request-lead" },
      { kind: "Finish", ts_ms: 2, reason: "stop" },
    ]

    const model = buildReplayModel(frames)
    const types: string[] = []
    for await (const ev of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
      types.push(ev.type)
    }
    expect(types).toEqual(["text-delta", "finish"])
  })

  // ---------------------------------------------------------------------------
  // Task 4: Frame exhaustion → ReplayExhaustedError
  // ---------------------------------------------------------------------------

  it("throws ReplayExhaustedError when stream() is called more times than recorded turns", async () => {
    const frames: TrajectoryFrame[] = [
      { kind: "Request", ts_ms: 1, messages: [], tools: [] },
      { kind: "TextDelta", ts_ms: 2, text: "only turn" },
      { kind: "Finish", ts_ms: 3, reason: "stop" },
    ]

    const model = buildReplayModel(frames)

    // First call: ok
    for await (const _ of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
      // consume
    }

    // Second call: exhausted
    await expect(async () => {
      for await (const _ of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
        // consume
      }
    }).rejects.toThrow(ReplayExhaustedError)
  })

  it("ReplayExhaustedError carries turn and totalTurns fields", async () => {
    const frames: TrajectoryFrame[] = [
      { kind: "Request", ts_ms: 1, messages: [], tools: [] },
      { kind: "Finish", ts_ms: 2, reason: "stop" },
    ]

    const model = buildReplayModel(frames)
    // consume turn 0
    for await (const _ of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
      /* noop */
    }

    let caught: unknown
    try {
      for await (const _ of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
        /* noop */
      }
    } catch (e) {
      caught = e
    }

    expect(caught).toBeInstanceOf(ReplayExhaustedError)
    const err = caught as ReplayExhaustedError
    expect(err.turn).toBe(1)
    expect(err.totalTurns).toBe(1)
  })

  it("throws ReplayExhaustedError on empty frame list", async () => {
    const model = buildReplayModel([])

    await expect(async () => {
      for await (const _ of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
        // consume
      }
    }).rejects.toThrow(ReplayExhaustedError)
  })

  it("does not make any network calls (pure in-memory replay)", async () => {
    // If buildReplayModel tried to call a real provider, it would throw or
    // require credentials. This test simply verifies no exception escapes from
    // the stream being a construct with no external dependencies.
    const frames: TrajectoryFrame[] = [
      { kind: "Request", ts_ms: 1, messages: [], tools: [] },
      { kind: "TextDelta", ts_ms: 2, text: "pure memory" },
      { kind: "Finish", ts_ms: 3, reason: "stop" },
    ]

    const model = buildReplayModel(frames)
    const events: unknown[] = []
    // Should complete without throwing despite no credentials/network.
    for await (const ev of await model.stream({ messages: [] } as Parameters<typeof model.stream>[0])) {
      events.push(ev)
    }

    expect(events.length).toBe(2) // text-delta + finish
  })
})
