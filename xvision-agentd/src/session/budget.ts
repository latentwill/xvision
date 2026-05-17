/**
 * Per-run budget enforcement helpers for `xvision-agentd`.
 *
 * Two enforcement vectors:
 *
 * 1. Wall-clock: each `agent.run` / `agent.continue` invocation gets an
 *    abortable timeout derived from `max_wall_ms` minus elapsed time since
 *    the run started. On timeout we abort the underlying SDK call and
 *    surface the result as `status: "aborted"` with reason
 *    `budget_wall_ms_exceeded`.
 *
 * 2. Token caps: cumulative `input_tokens` / `output_tokens` across all
 *    steps in a run. Caps are checked twice — pre-step (if a previous step
 *    already crossed the threshold, we short-circuit without invoking the
 *    agent) and post-step (if the step that just ran pushed us over). On
 *    overshoot we emit `status: "aborted"` with reason
 *    `budget_input_tokens_exceeded` or `budget_output_tokens_exceeded`.
 *
 * The helpers are intentionally side-effect-free aside from arming and
 * disarming the wall-clock timer, so `session.ts` stays thin and the
 * tests can drive each path deterministically.
 */
import type { BudgetLimits } from "./store.js"

/**
 * Typed reason codes surfaced via the JSON-RPC `error` field on an
 * aborted step result. The Rust client (`xvision-agent-client`) treats
 * these as opaque strings, but matching on them lets callers distinguish
 * budget exhaustion from other failures.
 */
export type BudgetAbortReason =
  | "budget_wall_ms_exceeded"
  | "budget_input_tokens_exceeded"
  | "budget_output_tokens_exceeded"

export interface CumulativeUsage {
  input_tokens: number
  output_tokens: number
}

export function emptyUsage(): CumulativeUsage {
  return { input_tokens: 0, output_tokens: 0 }
}

/**
 * Pre-step check: if a *previous* step's accumulated usage already
 * exceeded a cap, refuse to invoke the agent again. Returns the reason
 * code on overshoot, otherwise `null`.
 */
export function checkTokenCapsBeforeStep(
  cumulative: CumulativeUsage,
  limits: BudgetLimits,
): BudgetAbortReason | null {
  if (cumulative.input_tokens >= limits.max_input_tokens) {
    return "budget_input_tokens_exceeded"
  }
  if (cumulative.output_tokens >= limits.max_output_tokens) {
    return "budget_output_tokens_exceeded"
  }
  return null
}

/**
 * Post-step check: after a step completes (or was aborted by the SDK),
 * check whether the just-observed cumulative totals crossed a cap.
 * Returns the reason code on overshoot, otherwise `null`.
 *
 * `checkTokenCapsBeforeStep` covers the *next* step's short-circuit;
 * this covers the in-flight step's terminal status.
 */
export function checkTokenCapsAfterStep(
  cumulative: CumulativeUsage,
  limits: BudgetLimits,
): BudgetAbortReason | null {
  if (cumulative.input_tokens > limits.max_input_tokens) {
    return "budget_input_tokens_exceeded"
  }
  if (cumulative.output_tokens > limits.max_output_tokens) {
    return "budget_output_tokens_exceeded"
  }
  return null
}

/**
 * Remaining wall-clock budget for this run, in milliseconds. Negative or
 * zero means the wall budget is already exhausted before the step starts.
 */
export function remainingWallMs(
  startedAtMs: number,
  limits: BudgetLimits,
  nowMs: number,
): number {
  return limits.max_wall_ms - (nowMs - startedAtMs)
}

export interface WallTimer {
  /** Triggered when the timer fires; abort the in-flight agent run. */
  signal: AbortSignal
  /** Whether the timer fired (vs. was cleared by a completing run). */
  fired: () => boolean
  /** Stop the timer; safe to call multiple times. */
  clear: () => void
}

/**
 * Arm a wall-clock timer. After `wallMs` has elapsed, the returned
 * `AbortSignal` is aborted; callers pass this signal into the SDK so the
 * in-flight `agent.run` / `agent.continue` unwinds cleanly.
 *
 * If `wallMs <= 0`, the signal is aborted synchronously (the wall budget
 * was already exhausted before this step started).
 *
 * The default scheduler uses `setTimeout`/`clearTimeout`. Tests inject
 * their own to drive deterministic timing.
 */
export function armWallTimer(
  wallMs: number,
  opts: { schedule?: typeof setTimeout; cancel?: typeof clearTimeout } = {},
): WallTimer {
  const controller = new AbortController()
  let fired = false
  const schedule = opts.schedule ?? setTimeout
  const cancel = opts.cancel ?? clearTimeout

  if (wallMs <= 0) {
    fired = true
    controller.abort(new Error("budget_wall_ms_exceeded"))
    return {
      signal: controller.signal,
      fired: () => fired,
      clear: () => {},
    }
  }

  const handle = schedule(() => {
    fired = true
    controller.abort(new Error("budget_wall_ms_exceeded"))
  }, wallMs)
  // Best-effort: node's Timeout has `.unref` so this timer never keeps
  // the event loop alive on its own.
  if (typeof (handle as { unref?: () => void }).unref === "function") {
    ;(handle as { unref: () => void }).unref()
  }

  let cleared = false
  return {
    signal: controller.signal,
    fired: () => fired,
    clear: () => {
      if (cleared) return
      cleared = true
      cancel(handle as Parameters<typeof clearTimeout>[0])
    },
  }
}
