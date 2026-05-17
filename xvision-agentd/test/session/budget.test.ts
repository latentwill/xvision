import { describe, it, expect, vi } from "vitest"
import {
  armWallTimer,
  checkTokenCapsAfterStep,
  checkTokenCapsBeforeStep,
  emptyUsage,
  remainingWallMs,
} from "../../src/session/budget.js"

const LIMITS = {
  max_input_tokens: 100,
  max_output_tokens: 50,
  max_wall_ms: 1000,
}

describe("budget.checkTokenCapsBeforeStep", () => {
  it("returns null when usage is well under the caps", () => {
    expect(checkTokenCapsBeforeStep({ input_tokens: 10, output_tokens: 5 }, LIMITS)).toBeNull()
  })

  it("signals input-token exhaustion at the cap boundary", () => {
    expect(checkTokenCapsBeforeStep({ input_tokens: 100, output_tokens: 0 }, LIMITS)).toBe(
      "budget_input_tokens_exceeded",
    )
  })

  it("signals output-token exhaustion at the cap boundary", () => {
    expect(checkTokenCapsBeforeStep({ input_tokens: 0, output_tokens: 50 }, LIMITS)).toBe(
      "budget_output_tokens_exceeded",
    )
  })

  it("prioritizes input over output when both are exhausted", () => {
    expect(checkTokenCapsBeforeStep({ input_tokens: 100, output_tokens: 50 }, LIMITS)).toBe(
      "budget_input_tokens_exceeded",
    )
  })
})

describe("budget.checkTokenCapsAfterStep", () => {
  it("returns null when usage is exactly at the cap", () => {
    // Pre-step uses `>=`; post-step uses `>` so a step that lands exactly
    // on the limit doesn't double-fault — the *next* step short-circuits.
    expect(checkTokenCapsAfterStep({ input_tokens: 100, output_tokens: 50 }, LIMITS)).toBeNull()
  })

  it("signals exhaustion only when usage strictly exceeds the cap", () => {
    expect(checkTokenCapsAfterStep({ input_tokens: 101, output_tokens: 0 }, LIMITS)).toBe(
      "budget_input_tokens_exceeded",
    )
    expect(checkTokenCapsAfterStep({ input_tokens: 0, output_tokens: 51 }, LIMITS)).toBe(
      "budget_output_tokens_exceeded",
    )
  })
})

describe("budget.remainingWallMs", () => {
  it("subtracts elapsed time from the wall budget", () => {
    expect(remainingWallMs(1000, LIMITS, 1250)).toBe(750)
  })

  it("goes negative once the wall budget is exhausted", () => {
    expect(remainingWallMs(1000, LIMITS, 2500)).toBe(-500)
  })
})

describe("budget.armWallTimer", () => {
  it("fires after the configured duration and aborts the signal", () => {
    vi.useFakeTimers()
    try {
      const timer = armWallTimer(100)
      expect(timer.signal.aborted).toBe(false)
      expect(timer.fired()).toBe(false)
      vi.advanceTimersByTime(99)
      expect(timer.signal.aborted).toBe(false)
      vi.advanceTimersByTime(1)
      expect(timer.signal.aborted).toBe(true)
      expect(timer.fired()).toBe(true)
    } finally {
      vi.useRealTimers()
    }
  })

  it("aborts synchronously when the wall budget is already exhausted", () => {
    const timer = armWallTimer(0)
    expect(timer.signal.aborted).toBe(true)
    expect(timer.fired()).toBe(true)
  })

  it("does not fire after clear()", () => {
    vi.useFakeTimers()
    try {
      const timer = armWallTimer(100)
      timer.clear()
      vi.advanceTimersByTime(500)
      expect(timer.signal.aborted).toBe(false)
      expect(timer.fired()).toBe(false)
    } finally {
      vi.useRealTimers()
    }
  })
})

describe("budget.emptyUsage", () => {
  it("returns a fresh zeroed counter each call", () => {
    const a = emptyUsage()
    a.input_tokens = 42
    const b = emptyUsage()
    expect(b.input_tokens).toBe(0)
  })
})
