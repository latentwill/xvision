/**
 * Integration tests for `session.step` budget enforcement.
 *
 * The happy-path / tool-call behavior is covered by the legacy
 * `test/session-step.test.ts`. This file exclusively exercises the
 * three budget-exhaustion paths (`budget_wall_ms_exceeded`,
 * `budget_input_tokens_exceeded`, `budget_output_tokens_exceeded`) and
 * confirms the under-budget happy path still returns `completed`.
 */
import { describe, it, expect, beforeEach, vi } from "vitest"
import {
  handleSessionStartRun,
  handleSessionStep,
  handleSessionEndRun,
  __setStoreForTesting,
  __setBudgetClockForTesting,
} from "../../src/methods/session.js"
import { createStore } from "../../src/session/store.js"
import { resetRegistry, handleToolRegistrySet } from "../../src/methods/tool-registry.js"
import {
  installMockProvider,
  resetMockScript,
  setMockScript,
} from "../../src/testing/mock-provider.js"

const ECHO_DESC = {
  name: "echo",
  version: "1.0.0",
  description: "echoes",
  input_schema: { type: "object", properties: { msg: { type: "string" } }, required: ["msg"] },
  output_schema: { type: "object" },
  timeout_ms: 5000,
  side_effect_level: "pure",
  requires_approval: false,
}

function paramsWith(limits: { max_input_tokens: number; max_output_tokens: number; max_wall_ms: number }): {
  run_id: string
  provider_id: string
  model_id: string
  api_key: string
  system_prompt: string
  allowed_tools: string[]
  budget_limits: typeof limits
} {
  return {
    run_id: "run-budget",
    provider_id: "xvision-mock",
    model_id: "mock-model",
    api_key: "test",
    system_prompt: "you are a test",
    allowed_tools: ["echo"],
    budget_limits: limits,
  }
}

describe("session.step budget enforcement", () => {
  // Mutable clock: tests advance `nowMs` between operations to drive the
  // store's `now()` (used to stamp `created_at_ms` and to compute
  // elapsed wall-clock time during step enforcement).
  let nowMs = 1_000

  beforeEach(() => {
    installMockProvider()
    resetMockScript()
    resetRegistry()
    handleToolRegistrySet({ tools: [ECHO_DESC] })
    nowMs = 1_000
    __setStoreForTesting(createStore({ now: () => nowMs }))
    __setBudgetClockForTesting({})
    vi.restoreAllMocks()
  })

  it("happy path under budget returns status=completed", async () => {
    setMockScript([{ text: "ok" }])
    handleSessionStartRun(paramsWith({ max_input_tokens: 100, max_output_tokens: 100, max_wall_ms: 30_000 }))
    nowMs = 1_500
    const r = await handleSessionStep({ run_id: "run-budget", prompt: "hi" })
    expect(r.status).toBe("completed")
    expect(r.error).toBeUndefined()
    handleSessionEndRun({ run_id: "run-budget" })
  })

  it("pre-step wall-clock exhaustion short-circuits with budget_wall_ms_exceeded", async () => {
    setMockScript([{ text: "should never run" }])
    handleSessionStartRun(paramsWith({ max_input_tokens: 100, max_output_tokens: 100, max_wall_ms: 500 }))
    // Started at t=1000; advance clock past max_wall_ms so remaining <= 0.
    nowMs = 1_000 + 600

    const r = await handleSessionStep({ run_id: "run-budget", prompt: "hi" })
    expect(r.status).toBe("aborted")
    expect(r.error).toBe("budget_wall_ms_exceeded")
    expect(r.iterations).toBe(0)
    expect(r.usage.input_tokens).toBe(0)
    handleSessionEndRun({ run_id: "run-budget" })
  })

  it("subsequent step short-circuits when cumulative output_tokens already at cap", async () => {
    setMockScript([{ text: "one" }])
    handleSessionStartRun(paramsWith({ max_input_tokens: 100, max_output_tokens: 1, max_wall_ms: 30_000 }))
    nowMs = 1_100

    // First step lands exactly at the cap (1 output token). Per the
    // `>=` semantics in checkTokenCapsBeforeStep, the *next* step is the
    // one that short-circuits — landing-at-the-cap is allowed.
    const r1 = await handleSessionStep({ run_id: "run-budget", prompt: "go" })
    expect(r1.status).toBe("completed")

    setMockScript([{ text: "two" }])
    const r2 = await handleSessionStep({ run_id: "run-budget", prompt: "again" })
    expect(r2.status).toBe("aborted")
    expect(r2.error).toBe("budget_output_tokens_exceeded")
    expect(r2.iterations).toBe(0)
    handleSessionEndRun({ run_id: "run-budget" })
  })

  it("subsequent step short-circuits when cumulative input_tokens already at cap", async () => {
    setMockScript([{ text: "one" }])
    handleSessionStartRun(paramsWith({ max_input_tokens: 1, max_output_tokens: 100, max_wall_ms: 30_000 }))
    nowMs = 1_100

    const r1 = await handleSessionStep({ run_id: "run-budget", prompt: "go" })
    expect(r1.status).toBe("completed")

    setMockScript([{ text: "two" }])
    const r2 = await handleSessionStep({ run_id: "run-budget", prompt: "again" })
    expect(r2.status).toBe("aborted")
    expect(r2.error).toBe("budget_input_tokens_exceeded")
    expect(r2.iterations).toBe(0)
    handleSessionEndRun({ run_id: "run-budget" })
  })

  it("mid-step wall-clock exhaustion fires the timer and aborts via agent.abort", async () => {
    // Use a tool-call to suspend the agent run until we resolve `callRust`.
    // While the agent is awaiting the tool result we advance fake timers
    // past `max_wall_ms`; the armed wall timer calls agent.abort(), the
    // tool-call promise rejects/unwinds, and the run returns aborted.
    const callbackClient = await import("../../src/transport/callback-client.js")
    let resolveCallRust: (v: unknown) => void = () => {}
    const callRustSpy = vi.spyOn(callbackClient, "callRust").mockImplementation(
      () =>
        new Promise<unknown>((resolve) => {
          resolveCallRust = resolve
        }),
    )

    setMockScript([
      { toolCall: { name: "echo", input: { msg: "stall" } } },
      { text: "should never finish" },
    ])
    handleSessionStartRun(paramsWith({ max_input_tokens: 100, max_output_tokens: 100, max_wall_ms: 50 }))
    // Clock stays at start time so remainingWallMs is 50.

    vi.useFakeTimers()
    try {
      const stepPromise = handleSessionStep({ run_id: "run-budget", prompt: "go" })
      // Let the agent dispatch the tool call and suspend on callRust.
      await vi.advanceTimersByTimeAsync(0)
      expect(callRustSpy).toHaveBeenCalled()
      // Fire the wall timer; agent.abort() unwinds the in-flight run.
      await vi.advanceTimersByTimeAsync(60)
      // Resolve callRust so any pending awaits inside the SDK unblock.
      resolveCallRust({ echoed: "stall" })

      const r = await stepPromise
      expect(r.status).toBe("aborted")
      expect(r.error).toBe("budget_wall_ms_exceeded")
    } finally {
      vi.useRealTimers()
    }
    handleSessionEndRun({ run_id: "run-budget" })
  })
})
