/**
 * Frame recording tests (Task 5, Stage 2).
 *
 * Asserts that when `record: true` is set on a run, the model-wrapper tap and
 * tool-shim emit the expected ordered TrajectoryFrame values via emitFrame
 * (→ event.trajectory_frame notifications).
 *
 * Coverage:
 *   - Text step: Request → TextDelta → Usage → Finish
 *   - Tool step: Request → ToolCallDelta → ToolResult (success) → Usage → Finish
 *   - Tool error: ToolResult carries `error` field when callRust throws
 *   - tsMs is monotonically non-decreasing across all frames in a run
 *   - Without record:true, no trajectory_frame events are emitted
 */
import { describe, it, expect, beforeEach, vi } from "vitest"
import {
  handleSessionStartRun,
  handleSessionStep,
  handleSessionEndRun,
  __setStoreForTesting,
} from "../../src/methods/session.js"
import { createStore } from "../../src/session/store.js"
import { resetRegistry, handleToolRegistrySet } from "../../src/methods/tool-registry.js"
import {
  installMockProvider,
  setMockScript,
  resetMockScript,
} from "../../src/testing/mock-provider.js"
import * as eventClient from "../../src/transport/event-client.js"
import type { TrajectoryFrame } from "../../src/session/frame-types.js"
import { NOTIFY, type TrajectoryFrameEnvelope } from "../../src/session/emit.js"

type EmitSpy = { mock: { calls: unknown[][] } }

const ECHO_DESC = {
  name: "echo",
  version: "1.0.0",
  description: "echoes its input",
  input_schema: { type: "object", properties: { msg: { type: "string" } }, required: ["msg"] },
  output_schema: { type: "object" },
  timeout_ms: 5000,
  side_effect_level: "pure",
  requires_approval: false,
}

/** Base params for a recording-enabled run. */
function makeParams(run_id: string, allowed_tools: string[] = ["echo"]) {
  return {
    run_id,
    provider_id: "xvision-mock",
    model_id: "mock-model",
    api_key: "test",
    system_prompt: "you are a test",
    allowed_tools,
    budget_limits: { max_input_tokens: 10000, max_output_tokens: 10000, max_wall_ms: 30000 },
    record: true,
  }
}

/** Collect the trajectory frame ENVELOPES emitted during a test. */
function collectEnvelopes(spy: EmitSpy): TrajectoryFrameEnvelope[] {
  return spy.mock.calls
    .filter((call) => call[0] === NOTIFY.TrajectoryFrame)
    .map((call) => call[1] as TrajectoryFrameEnvelope)
}

/** Collect trajectory frame bodies (unwrapped from the envelope). */
function collectFrames(spy: EmitSpy): TrajectoryFrame[] {
  return collectEnvelopes(spy).map((env) => env.frame)
}

describe("trajectory frame recording", () => {
  // emitNotification is the low-level sink for all notifications including
  // trajectory frames. We spy on it to capture frames without needing a socket.
  let emitSpy: EmitSpy

  beforeEach(() => {
    installMockProvider()
    resetMockScript()
    resetRegistry()
    handleToolRegistrySet({ tools: [ECHO_DESC] })
    __setStoreForTesting(createStore({ now: () => Date.now() }))
    vi.restoreAllMocks()
    emitSpy = vi.spyOn(eventClient, "emitNotification").mockResolvedValue(undefined) as unknown as EmitSpy
  })

  // -----------------------------------------------------------------------
  // Text step
  // -----------------------------------------------------------------------

  it("text step emits Request → TextDelta → Usage → Finish in order", async () => {
    setMockScript([{ text: "hello world" }])
    handleSessionStartRun(makeParams("run-frame-text"))

    await handleSessionStep({ run_id: "run-frame-text", prompt: "hi" })
    handleSessionEndRun({ run_id: "run-frame-text" })

    const frames = collectFrames(emitSpy)

    expect(frames.length).toBeGreaterThanOrEqual(4)

    // First frame must be Request
    expect(frames[0]?.kind).toBe("Request")
    const req = frames[0] as { kind: "Request"; messages: unknown; tools: unknown; ts_ms: number }
    expect(Array.isArray(req.messages)).toBe(true)
    expect(Array.isArray(req.tools)).toBe(true)
    expect(typeof req.ts_ms).toBe("number")

    const kinds = frames.map((f) => f.kind)

    // TextDelta must appear
    expect(kinds).toContain("TextDelta")
    const td = frames.find((f) => f.kind === "TextDelta") as
      | { kind: "TextDelta"; text: string; ts_ms: number }
      | undefined
    expect(td?.text).toBe("hello world")

    // Usage must appear
    expect(kinds).toContain("Usage")

    // Last frame must be Finish
    const last = frames[frames.length - 1]
    expect(last?.kind).toBe("Finish")
    const fin = last as { kind: "Finish"; reason: string; ts_ms: number }
    expect(fin.reason).toBe("stop")
  })

  // -----------------------------------------------------------------------
  // Tool step
  // -----------------------------------------------------------------------

  it("tool step emits Request → ToolCallDelta → ToolResult (success) → Usage → Finish", async () => {
    const callRustMod = await import("../../src/transport/callback-client.js")
    vi.spyOn(callRustMod, "callRust").mockResolvedValue({ echoed: "hi" })

    setMockScript([
      { toolCall: { name: "echo", input: { msg: "hi" } } },
      { text: "done" },
    ])
    handleSessionStartRun(makeParams("run-frame-tool"))

    await handleSessionStep({ run_id: "run-frame-tool", prompt: "go" })
    handleSessionEndRun({ run_id: "run-frame-tool" })

    const frames = collectFrames(emitSpy)
    const kinds = frames.map((f) => f.kind)

    // Request must be first
    expect(frames[0]?.kind).toBe("Request")

    // ToolCallDelta must appear
    expect(kinds).toContain("ToolCallDelta")
    const tcd = frames.find((f) => f.kind === "ToolCallDelta") as
      | { kind: "ToolCallDelta"; tool_name?: string; ts_ms: number }
      | undefined
    expect(tcd?.tool_name).toBe("echo")

    // ToolResult must appear after ToolCallDelta
    expect(kinds).toContain("ToolResult")
    const trFrame = frames.find((f) => f.kind === "ToolResult") as
      | { kind: "ToolResult"; output: unknown; error?: string; ts_ms: number }
      | undefined
    expect(trFrame?.output).toEqual({ echoed: "hi" })
    expect(trFrame?.error).toBeUndefined()

    // ToolResult must come AFTER ToolCallDelta in the frame sequence
    const tcdIdx = kinds.indexOf("ToolCallDelta")
    const trIdx = kinds.indexOf("ToolResult")
    expect(trIdx).toBeGreaterThan(tcdIdx)

    // Finish must be last
    expect(frames[frames.length - 1]?.kind).toBe("Finish")
  })

  // -----------------------------------------------------------------------
  // Tool error: error-as-data ToolResult
  // -----------------------------------------------------------------------

  it("tool error emits ToolResult with error field (error-as-data, not thrown)", async () => {
    const callRustMod = await import("../../src/transport/callback-client.js")
    vi.spyOn(callRustMod, "callRust").mockRejectedValue(new Error("tool failed badly"))

    setMockScript([
      { toolCall: { name: "echo", input: { msg: "oops" } } },
      { text: "recovered" },
    ])
    handleSessionStartRun(makeParams("run-frame-err"))

    // Should NOT throw — errors are returned as data
    const result = await handleSessionStep({ run_id: "run-frame-err", prompt: "go" })
    handleSessionEndRun({ run_id: "run-frame-err" })

    // The step should still complete (Cline sees error-as-data, not an exception)
    expect(["completed", "aborted"]).toContain(result.status)

    const frames = collectFrames(emitSpy)
    const trFrame = frames.find((f) => f.kind === "ToolResult") as
      | { kind: "ToolResult"; output: unknown; error?: string; ts_ms: number }
      | undefined

    expect(trFrame).toBeDefined()
    expect(trFrame?.error).toBe("tool failed badly")
    // output should be the error-as-data object
    expect(trFrame?.output).toEqual({ error: "tool failed badly" })
  })

  // -----------------------------------------------------------------------
  // Monotonically non-decreasing tsMs
  // -----------------------------------------------------------------------

  it("tsMs is monotonically non-decreasing across all frames in a text step", async () => {
    setMockScript([{ text: "ts-check" }])
    handleSessionStartRun(makeParams("run-frame-ts"))

    await handleSessionStep({ run_id: "run-frame-ts", prompt: "hi" })
    handleSessionEndRun({ run_id: "run-frame-ts" })

    const frames = collectFrames(emitSpy)
    expect(frames.length).toBeGreaterThanOrEqual(3)

    for (let i = 1; i < frames.length; i++) {
      const prev = frames[i - 1] as { ts_ms: number }
      const curr = frames[i] as { ts_ms: number }
      expect(curr.ts_ms).toBeGreaterThanOrEqual(prev.ts_ms)
    }
  })

  // -----------------------------------------------------------------------
  // Gap #8: coordinate envelope shape matches the Rust parser
  // -----------------------------------------------------------------------

  it("each frame travels in a {run_id, slot_role, step_index, frame_index, frame} envelope", async () => {
    setMockScript([{ text: "envelope check" }])
    handleSessionStartRun({ ...makeParams("run-frame-env"), slot_role: "trader" })

    await handleSessionStep({ run_id: "run-frame-env", prompt: "hi" })
    handleSessionEndRun({ run_id: "run-frame-env" })

    const envelopes = collectEnvelopes(emitSpy)
    expect(envelopes.length).toBeGreaterThanOrEqual(3)

    for (const env of envelopes) {
      // All four coordinate fields the Rust `parse_trajectory_frame_notification`
      // requires, plus the frame body under `frame`.
      expect(env.run_id).toBe("run-frame-env")
      expect(env.slot_role).toBe("trader")
      expect(typeof env.step_index).toBe("number")
      expect(typeof env.frame_index).toBe("number")
      expect(env.frame).toBeDefined()
      expect(typeof env.frame.kind).toBe("string")
    }

    // Single step → step_index 0 for every frame.
    expect(envelopes.every((e) => e.step_index === 0)).toBe(true)

    // frame_index is strictly increasing (monotonic) across the recording.
    for (let i = 1; i < envelopes.length; i++) {
      expect(envelopes[i]!.frame_index).toBeGreaterThan(envelopes[i - 1]!.frame_index)
    }
    // First frame_index is 0.
    expect(envelopes[0]!.frame_index).toBe(0)
  })

  it("defaults slot_role to \"default\" when the run omits it", async () => {
    setMockScript([{ text: "no slot role" }])
    handleSessionStartRun(makeParams("run-frame-default-slot"))

    await handleSessionStep({ run_id: "run-frame-default-slot", prompt: "hi" })
    handleSessionEndRun({ run_id: "run-frame-default-slot" })

    const envelopes = collectEnvelopes(emitSpy)
    expect(envelopes.length).toBeGreaterThan(0)
    expect(envelopes.every((e) => e.slot_role === "default")).toBe(true)
  })

  it("bumps step_index per session.step across multiple steps", async () => {
    // Two steps in one run → frames from step 2 carry step_index 1.
    setMockScript([{ text: "step one" }])
    handleSessionStartRun({ ...makeParams("run-frame-multistep"), slot_role: "trader" })

    await handleSessionStep({ run_id: "run-frame-multistep", prompt: "first" })
    const afterFirst = collectEnvelopes(emitSpy).length
    expect(collectEnvelopes(emitSpy).every((e) => e.step_index === 0)).toBe(true)

    setMockScript([{ text: "step two" }])
    await handleSessionStep({ run_id: "run-frame-multistep", prompt: "second" })
    handleSessionEndRun({ run_id: "run-frame-multistep" })

    const all = collectEnvelopes(emitSpy)
    expect(all.length).toBeGreaterThan(afterFirst)
    const secondStepFrames = all.slice(afterFirst)
    expect(secondStepFrames.length).toBeGreaterThan(0)
    expect(secondStepFrames.every((e) => e.step_index === 1)).toBe(true)
    // frame_index stays monotonic across step boundaries (never resets).
    for (let i = 1; i < all.length; i++) {
      expect(all[i]!.frame_index).toBeGreaterThan(all[i - 1]!.frame_index)
    }
  })

  // -----------------------------------------------------------------------
  // No frames without record:true
  // -----------------------------------------------------------------------

  it("does NOT emit trajectory_frame events when record is not set", async () => {
    setMockScript([{ text: "no record" }])
    // Same as makeParams but without `record: true`
    handleSessionStartRun({
      run_id: "run-no-record",
      provider_id: "xvision-mock",
      model_id: "mock-model",
      api_key: "test",
      system_prompt: "you are a test",
      allowed_tools: ["echo"],
      budget_limits: { max_input_tokens: 10000, max_output_tokens: 10000, max_wall_ms: 30000 },
    })

    await handleSessionStep({ run_id: "run-no-record", prompt: "hi" })
    handleSessionEndRun({ run_id: "run-no-record" })

    const frames = collectFrames(emitSpy)
    expect(frames.length).toBe(0)
  })
})
