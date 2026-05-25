/**
 * Tests for Gap #2: terminal `event.run_finished` with correct status.
 *
 * Asserts that the observability stream is always complete and correctly
 * statused — no open / mis-statused run rows in the recorder:
 *
 *   1. Normal completion → exactly one run_finished{status:"completed"}
 *   2. SDK throw mid-step → run_finished{status:"failed"} (not just event.error)
 *   3. Budget/abort (step returns aborted) + end_run{status:cancelled}
 *        → run_finished{status:"cancelled"} (not "completed")
 *   4. Double-emit guard: catch path emits run_finished, end_run must NOT
 *        emit a second one
 *   5. end_run with explicit status (backward-compatible default "completed")
 *   6. end_run rejects unknown status strings
 */
import { describe, it, expect, beforeEach, vi, afterEach } from "vitest"
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
  setMockScript,
  resetMockScript,
} from "../../src/testing/mock-provider.js"
import * as eventClient from "../../src/transport/event-client.js"
import { NOTIFY } from "../../src/session/emit.js"

// ── helpers ──────────────────────────────────────────────────────────────────

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

const BASE_LIMITS = { max_input_tokens: 10_000, max_output_tokens: 10_000, max_wall_ms: 30_000 }

function makeParams(run_id: string) {
  return {
    run_id,
    provider_id: "xvision-mock",
    model_id: "mock-model",
    api_key: "test",
    system_prompt: "you are a test",
    allowed_tools: ["echo"],
    budget_limits: BASE_LIMITS,
  }
}

/**
 * Filter captured emitNotification calls to those for method `method`,
 * returning the params objects.
 */
function capturedForMethod(
  spy: ReturnType<typeof vi.spyOn>,
  method: string,
): unknown[] {
  return (spy.mock.calls as [string, unknown][])
    .filter(([m]) => m === method)
    .map(([, params]) => params)
}

// ── suite ────────────────────────────────────────────────────────────────────

describe("run_finished terminal status (Gap #2)", () => {
  let emitSpy: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    installMockProvider()
    resetMockScript()
    resetRegistry()
    handleToolRegistrySet({ tools: [ECHO_DESC] })
    __setStoreForTesting(createStore({ now: () => Date.now() }))
    __setBudgetClockForTesting({})
    // Intercept emitNotification so we can inspect what went on the wire
    // without needing a live UDS socket.
    emitSpy = vi.spyOn(eventClient, "emitNotification").mockResolvedValue(undefined)
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  // ── 1. Normal completion ────────────────────────────────────────────────────
  it("normal completion → exactly one run_finished{status:completed}", async () => {
    setMockScript([{ text: "all good" }])
    handleSessionStartRun(makeParams("run-gap2-ok"))
    const r = await handleSessionStep({ run_id: "run-gap2-ok", prompt: "go" })
    expect(r.status).toBe("completed")

    // end_run with no explicit status → defaults to "completed"
    handleSessionEndRun({ run_id: "run-gap2-ok" })

    const finisheds = capturedForMethod(emitSpy, NOTIFY.RunFinished)
    expect(finisheds).toHaveLength(1)
    const f = finisheds[0] as Record<string, unknown>
    expect(f.status).toBe("completed")
    expect(f.run_id).toBe("run-gap2-ok")
    expect(typeof f.finished_at_ms).toBe("number")
  })

  // ── 2. SDK throw mid-step → run_finished{failed}, NOT just event.error ──────
  it("SDK throw mid-step → run_finished{status:failed} emitted before re-throw", async () => {
    // Make buildAgent return an agent whose `run()` throws — this is the path
    // that exercises the catch block in handleSessionStep. We spy on
    // build-agent.ts directly, injecting a stub that throws on `run`.
    const buildAgentMod = await import("../../src/session/build-agent.js")
    vi.spyOn(buildAgentMod, "buildAgent").mockReturnValue({
      hasRun: false,
      run: () => Promise.reject(new Error("sdk boom")),
      continue: () => Promise.reject(new Error("sdk boom")),
      abort: () => {},
    } as unknown as ReturnType<typeof buildAgentMod.buildAgent>)

    handleSessionStartRun(makeParams("run-gap2-fail"))

    // handleSessionStep must re-throw — wrap in try/catch.
    let threw = false
    try {
      await handleSessionStep({ run_id: "run-gap2-fail", prompt: "go" })
    } catch {
      threw = true
    }
    expect(threw).toBe(true)

    // There must be a terminal run_finished{status:failed}.
    const finisheds = capturedForMethod(emitSpy, NOTIFY.RunFinished)
    expect(finisheds.length).toBeGreaterThanOrEqual(1)
    const f = finisheds[0] as Record<string, unknown>
    expect(f.status).toBe("failed")
    expect(f.run_id).toBe("run-gap2-fail")
    // The error field must be present (carries the exception message).
    expect(typeof f.error).toBe("string")
    expect((f.error as string).length).toBeGreaterThan(0)
  })

  // ── 3. end_run with status:cancelled for an aborted run ────────────────────
  it("end_run{status:cancelled} emits run_finished{status:cancelled}", async () => {
    // A normal successful step (Rust caller decides to abort after inspecting
    // the step result, then calls end_run with status:cancelled).
    setMockScript([{ text: "ok" }])
    handleSessionStartRun(makeParams("run-gap2-cancel"))
    await handleSessionStep({ run_id: "run-gap2-cancel", prompt: "go" })

    // Rust caller signals "this run was cancelled/budget-killed"
    handleSessionEndRun({ run_id: "run-gap2-cancel", status: "cancelled" })

    const finisheds = capturedForMethod(emitSpy, NOTIFY.RunFinished)
    expect(finisheds).toHaveLength(1)
    const f = finisheds[0] as Record<string, unknown>
    expect(f.status).toBe("cancelled")
    expect(f.run_id).toBe("run-gap2-cancel")
  })

  // ── 4. Double-emit guard: catch path + end_run must produce exactly ONE run_finished ──
  it("after SDK throw, end_run does NOT double-emit run_finished", async () => {
    const buildAgentMod = await import("../../src/session/build-agent.js")
    vi.spyOn(buildAgentMod, "buildAgent").mockReturnValue({
      hasRun: false,
      run: () => Promise.reject(new Error("oops")),
      continue: () => Promise.reject(new Error("oops")),
      abort: () => {},
    } as unknown as ReturnType<typeof buildAgentMod.buildAgent>)

    handleSessionStartRun(makeParams("run-gap2-dedup"))

    try {
      await handleSessionStep({ run_id: "run-gap2-dedup", prompt: "go" })
    } catch {
      // expected
    }

    // Rust calls end_run after catching the JSON-RPC error — guard must fire
    // and suppress the second emit.
    handleSessionEndRun({ run_id: "run-gap2-dedup" })

    const finisheds = capturedForMethod(emitSpy, NOTIFY.RunFinished)
    expect(finisheds).toHaveLength(1)
    expect((finisheds[0] as Record<string, unknown>).status).toBe("failed")
  })

  // ── 5. end_run defaults to "completed" when no status provided (backward-compat) ──
  it("end_run without status field defaults to completed", () => {
    handleSessionStartRun(makeParams("run-gap2-compat"))
    handleSessionEndRun({ run_id: "run-gap2-compat" })

    const finisheds = capturedForMethod(emitSpy, NOTIFY.RunFinished)
    expect(finisheds).toHaveLength(1)
    expect((finisheds[0] as Record<string, unknown>).status).toBe("completed")
  })

  // ── 6. end_run rejects unknown status values ────────────────────────────────
  it("end_run rejects an invalid status value", () => {
    handleSessionStartRun(makeParams("run-gap2-bad-status"))
    expect(() =>
      handleSessionEndRun({ run_id: "run-gap2-bad-status", status: "aborted" }),
    ).toThrow(TypeError)
    // Clean up (no run_finished was emitted).
    handleSessionEndRun({ run_id: "run-gap2-bad-status" })
  })

  // ── 7. Budget wall abort: step returns aborted, caller passes cancelled ──────
  it("wall-budget step abort + end_run{status:cancelled} → run_finished{cancelled}", async () => {
    // Pre-step wall-budget exhaustion short-circuits without invoking the agent.
    let nowMs = 1_000
    __setStoreForTesting(createStore({ now: () => nowMs }))
    handleSessionStartRun({
      run_id: "run-gap2-wall",
      provider_id: "xvision-mock",
      model_id: "mock-model",
      api_key: "test",
      system_prompt: "you are a test",
      allowed_tools: ["echo"],
      budget_limits: { max_input_tokens: 100, max_output_tokens: 100, max_wall_ms: 500 },
    })
    // Advance clock past the wall budget.
    nowMs = 1_000 + 600

    const r = await handleSessionStep({ run_id: "run-gap2-wall", prompt: "go" })
    expect(r.status).toBe("aborted")
    expect(r.error).toBe("budget_wall_ms_exceeded")

    // Rust caller maps step "aborted" → end_run{status:cancelled}.
    handleSessionEndRun({ run_id: "run-gap2-wall", status: "cancelled" })

    const finisheds = capturedForMethod(emitSpy, NOTIFY.RunFinished)
    expect(finisheds).toHaveLength(1)
    const f = finisheds[0] as Record<string, unknown>
    expect(f.status).toBe("cancelled")
    expect(f.run_id).toBe("run-gap2-wall")
  })
})
