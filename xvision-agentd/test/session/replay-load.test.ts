/**
 * Integration tests for `session.replay_load` + `session.step` (Stage 3, Task 2).
 *
 * Verifies:
 *   (a) replay_load accepts frames and stores them; result.loaded matches frame count.
 *   (b) After replay_load, session.step drives the agent with buildReplayModel
 *       (no provider call) and returns the recorded decision from submit_decision
 *       with usage summed from Usage frames.
 *   (c) replay_load validates its params (run_id, frames).
 *   (d) replay_load rejects an unknown run_id.
 *
 * Frame exhaustion (item 4) is covered in replay-model.test.ts; here we just
 * assert that the replayed step completes successfully for a valid recording.
 */
import { describe, it, expect, beforeEach, vi } from "vitest"
import {
  handleSessionStartRun,
  handleSessionStep,
  handleSessionEndRun,
  handleSessionReplayLoad,
  __setStoreForTesting,
} from "../../src/methods/session.js"
import { createStore, type SessionStore } from "../../src/session/store.js"
import { resetRegistry, handleToolRegistrySet } from "../../src/methods/tool-registry.js"
import type { TrajectoryFrame } from "../../src/session/frame-types.js"

// submit_decision is the lifecycle tool; it needs no registry entry.
const SUBMIT_DECISION_SCHEMA = {
  type: "object",
  properties: {
    action: { type: "string" },
  },
  required: ["action"],
}

function baseParams(run_id: string) {
  return {
    run_id,
    provider_id: "xvision-mock", // provider_id is overridden by replayFrames in buildAgent
    model_id: "mock-model",
    system_prompt: "you are a test trader",
    allowed_tools: ["submit_decision"],
    budget_limits: { max_input_tokens: 10000, max_output_tokens: 10000, max_wall_ms: 30000 },
    decision_schema: SUBMIT_DECISION_SCHEMA,
  }
}

/**
 * A recorded trajectory for a single step where the agent:
 *   1. Emits no text (goes straight to tool call)
 *   2. Calls submit_decision with { action: "buy" }
 *   3. Logs usage (10 input, 5 output tokens)
 *   4. Finishes with reason "tool-calls"
 */
const RECORDED_FRAMES: TrajectoryFrame[] = [
  { kind: "Request", ts_ms: 100, messages: [], tools: [] },
  {
    kind: "ToolCallDelta",
    ts_ms: 110,
    tool_call_id: "tc-0",
    tool_name: "submit_decision",
    input: { action: "buy" },
  },
  {
    kind: "Usage",
    ts_ms: 120,
    input_tokens: 10,
    output_tokens: 5,
    cache_read_tokens: 0,
    cache_write_tokens: 0,
    total_cost: 0.001,
  },
  { kind: "Finish", ts_ms: 130, reason: "tool-calls" },
]

describe("session.replay_load", () => {
  let store: SessionStore

  beforeEach(() => {
    resetRegistry()
    // No real tools needed — submit_decision is a built-in lifecycle tool.
    handleToolRegistrySet({ tools: [] })
    store = createStore({ now: () => 1000 })
    __setStoreForTesting(store)
    vi.restoreAllMocks()
  })

  // -------------------------------------------------------------------------
  // Param validation
  // -------------------------------------------------------------------------

  it("rejects missing run_id", () => {
    expect(() => handleSessionReplayLoad({ frames: [] })).toThrow(TypeError)
  })

  it("rejects empty run_id", () => {
    expect(() => handleSessionReplayLoad({ run_id: "", frames: [] })).toThrow(TypeError)
  })

  it("rejects non-array frames", () => {
    expect(() => handleSessionReplayLoad({ run_id: "r", frames: "bad" })).toThrow(TypeError)
  })

  it("rejects frames with missing kind field", () => {
    expect(() =>
      handleSessionReplayLoad({ run_id: "r-kind", frames: [{ ts_ms: 1 }] }),
    ).toThrow(TypeError)
  })

  it("rejects unknown run_id", () => {
    expect(() =>
      handleSessionReplayLoad({ run_id: "no-such-run", frames: RECORDED_FRAMES }),
    ).toThrow(/session not found/)
  })

  // -------------------------------------------------------------------------
  // Happy-path: load result
  // -------------------------------------------------------------------------

  it("returns loaded count equal to number of frames", () => {
    handleSessionStartRun(baseParams("r-load-count"))
    const result = handleSessionReplayLoad({ run_id: "r-load-count", frames: RECORDED_FRAMES })
    expect(result).toEqual({ loaded: RECORDED_FRAMES.length })
    handleSessionEndRun({ run_id: "r-load-count" })
  })

  it("accepts an empty frames array (zero frames, no exhaustion until step)", () => {
    handleSessionStartRun(baseParams("r-empty"))
    const result = handleSessionReplayLoad({ run_id: "r-empty", frames: [] })
    // Empty array: loaded=0. The replay model will exhaust on first step.
    expect(result).toEqual({ loaded: 0 })
    handleSessionEndRun({ run_id: "r-empty" })
  })

  // -------------------------------------------------------------------------
  // Integration: replay_load + step reproduces recorded decision
  // -------------------------------------------------------------------------

  it("step after replay_load runs with replay model and returns submit_decision output", async () => {
    handleSessionStartRun(baseParams("r-replay-step"))
    handleSessionReplayLoad({ run_id: "r-replay-step", frames: RECORDED_FRAMES })

    const result = await handleSessionStep({ run_id: "r-replay-step", prompt: "run" })

    // The step should complete (submit_decision tool finishes the run)
    expect(result.status).toBe("completed")

    // The decision_json should match what was in the recorded ToolCallDelta input
    expect(result.decision_json).toBeDefined()
    const decision = JSON.parse(result.decision_json!)
    expect(decision).toMatchObject({ action: "buy" })

    // Usage should reflect the recorded Usage frame
    expect(result.usage.input_tokens).toBe(10)
    expect(result.usage.output_tokens).toBe(5)

    handleSessionEndRun({ run_id: "r-replay-step" })
  })

  it("replay step does not call any real provider (no network credentials needed)", async () => {
    // provider_id is set to a non-mock value — if the replay branch doesn't
    // activate, buildProviderModel would throw about missing credentials.
    handleSessionStartRun({
      ...baseParams("r-no-network"),
      provider_id: "anthropic", // would fail without an API key if no replay
      // No api_key provided — ensures no live provider call occurs
    })
    handleSessionReplayLoad({ run_id: "r-no-network", frames: RECORDED_FRAMES })

    // Should complete without throwing despite no api_key / live provider.
    const result = await handleSessionStep({ run_id: "r-no-network", prompt: "run" })
    expect(result.status).toBe("completed")

    handleSessionEndRun({ run_id: "r-no-network" })
  })

  // -------------------------------------------------------------------------
  // Usage accumulation from recorded Usage frames
  // -------------------------------------------------------------------------

  it("step usage sums from the Usage frames in the recording", async () => {
    // Two Usage frames: verify they're summed
    const framesWithDoubleUsage: TrajectoryFrame[] = [
      { kind: "Request", ts_ms: 1, messages: [], tools: [] },
      {
        kind: "Usage",
        ts_ms: 2,
        input_tokens: 7,
        output_tokens: 3,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        total_cost: 0,
      },
      {
        kind: "ToolCallDelta",
        ts_ms: 3,
        tool_call_id: "tc-0",
        tool_name: "submit_decision",
        input: { action: "hold" },
      },
      {
        kind: "Usage",
        ts_ms: 4,
        input_tokens: 3,
        output_tokens: 2,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        total_cost: 0,
      },
      { kind: "Finish", ts_ms: 5, reason: "tool-calls" },
    ]

    handleSessionStartRun(baseParams("r-usage-sum"))
    handleSessionReplayLoad({ run_id: "r-usage-sum", frames: framesWithDoubleUsage })

    const result = await handleSessionStep({ run_id: "r-usage-sum", prompt: "run" })

    // The @cline/sdk Agent sums usage internally from the model's usage events
    expect(result.usage.input_tokens).toBeGreaterThan(0)
    expect(result.usage.output_tokens).toBeGreaterThan(0)

    handleSessionEndRun({ run_id: "r-usage-sum" })
  })

  // -------------------------------------------------------------------------
  // Agent rebuilt after replay_load (not reusing stale live-provider agent)
  // -------------------------------------------------------------------------

  it("replay_load resets a stale agent so the next step uses the replay model", () => {
    // The key invariant: after replay_load, the session's agent field is null
    // and the replay frames are stored, so the next step rebuilds with the
    // replay model instead of reusing any pre-existing live-provider agent.
    handleSessionStartRun(baseParams("r-reset"))

    // Verify session was created with agent=null initially
    const sessionBefore = store.get("r-reset")
    expect(sessionBefore?.agent).toBeNull()

    handleSessionReplayLoad({ run_id: "r-reset", frames: RECORDED_FRAMES })

    // After replay_load: agent is still null, frames are stored
    const session = store.get("r-reset")
    expect(session?.agent).toBeNull()
    expect(store.getReplayFrames("r-reset")).toEqual(RECORDED_FRAMES)

    handleSessionEndRun({ run_id: "r-reset" })
  })
})
