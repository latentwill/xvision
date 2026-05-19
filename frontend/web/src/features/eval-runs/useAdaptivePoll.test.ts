/**
 * Tests for `useAdaptivePoll` — the status-aware poll-cadence hook for
 * `eval.get_run`. Covers the state machine transitions
 * `queued → running → completed`, the 5-minute idle backoff to 30s, and
 * the terminal-status stop.
 *
 * We exercise both the pure `adaptivePollInterval` calculator (no React
 * needed) and the `useAdaptivePoll` hook (renderHook + a controllable
 * `nowFn` seam — vi.useFakeTimers would also work, but the seam lets the
 * test stay deterministic without coupling to the timer queue).
 */
import { describe, expect, it } from "vitest";
import { renderHook } from "@testing-library/react";

import {
  ADAPTIVE_POLL_CAP_MS,
  POLL_QUEUED_MS,
  POLL_RUNNING_MS,
  STALE_CUTOFF_MS,
  adaptivePollInterval,
  useAdaptivePoll,
} from "./useAdaptivePoll";

describe("adaptivePollInterval (pure)", () => {
  it("returns false for terminal statuses", () => {
    expect(adaptivePollInterval("completed", 0)).toBe(false);
    expect(adaptivePollInterval("failed", 0)).toBe(false);
    expect(adaptivePollInterval("cancelled", 0)).toBe(false);
  });

  it("returns false for missing/unknown-empty status (initial mount)", () => {
    expect(adaptivePollInterval(undefined, 0)).toBe(false);
    expect(adaptivePollInterval(null, 0)).toBe(false);
    expect(adaptivePollInterval("", 0)).toBe(false);
  });

  it("polls running runs at 2s", () => {
    expect(adaptivePollInterval("running", 0)).toBe(POLL_RUNNING_MS);
    expect(adaptivePollInterval("running", 1000)).toBe(POLL_RUNNING_MS);
  });

  it("polls queued runs at 5s", () => {
    expect(adaptivePollInterval("queued", 0)).toBe(POLL_QUEUED_MS);
  });

  it("backs off to 30s after 5 minutes of no status change", () => {
    expect(adaptivePollInterval("queued", STALE_CUTOFF_MS - 1)).toBe(
      POLL_QUEUED_MS,
    );
    expect(adaptivePollInterval("queued", STALE_CUTOFF_MS)).toBe(
      ADAPTIVE_POLL_CAP_MS,
    );
    expect(adaptivePollInterval("running", STALE_CUTOFF_MS + 1)).toBe(
      ADAPTIVE_POLL_CAP_MS,
    );
  });

  it("treats unknown non-terminal status as queued (forward-compat)", () => {
    expect(adaptivePollInterval("paused", 0)).toBe(POLL_QUEUED_MS);
  });
});

describe("useAdaptivePoll (hook)", () => {
  /**
   * Build a controllable clock so we can drive "wall-clock time" forward
   * without waiting on real timers. The function returned by the hook
   * reads `nowFn()` each invocation, so bumping `now` and then calling
   * the returned function deterministically simulates time passing
   * between scheduled refetches.
   */
  function makeClock(start = 1_000_000) {
    let now = start;
    return {
      advance(ms: number) {
        now += ms;
      },
      now: () => now,
    };
  }

  it("walks queued → running → completed and stops polling on terminal", () => {
    const clock = makeClock();
    const { result } = renderHook(() => useAdaptivePoll("run-1", clock.now));

    // First call: status=queued, no time has passed since "creation".
    expect(result.current("queued")).toBe(POLL_QUEUED_MS);

    // Status flips to running — the hook resets its change-timestamp
    // and the cadence drops to 2s.
    clock.advance(3000);
    expect(result.current("running")).toBe(POLL_RUNNING_MS);

    // Running for a bit longer, still well under the stale cutoff.
    clock.advance(10_000);
    expect(result.current("running")).toBe(POLL_RUNNING_MS);

    // Terminal status — the final read returns `false` so tanstack-query
    // stops scheduling further refetches. The contract's "one final
    // read" is the read that delivered this status; no extra fetch.
    expect(result.current("completed")).toBe(false);
  });

  it("backs off to 30s after 5min of no status change", () => {
    const clock = makeClock();
    const { result } = renderHook(() => useAdaptivePoll("run-1", clock.now));

    // Seed at queued. The hook captures the current clock as the
    // "last change at" baseline on its first invocation.
    expect(result.current("queued")).toBe(POLL_QUEUED_MS);

    // Jump to just under 5 minutes — still polling at the queued
    // cadence, no backoff yet.
    clock.advance(STALE_CUTOFF_MS - 1);
    expect(result.current("queued")).toBe(POLL_QUEUED_MS);

    // Cross the 5-minute boundary — the cap kicks in.
    clock.advance(2);
    expect(result.current("queued")).toBe(ADAPTIVE_POLL_CAP_MS);

    // A status change resets the timer back to the normal cadence.
    expect(result.current("running")).toBe(POLL_RUNNING_MS);
  });

  it("resets the staleness timer on runId change (rerun navigation)", () => {
    const clock = makeClock();
    const { result, rerender } = renderHook(
      ({ runId }: { runId: string }) => useAdaptivePoll(runId, clock.now),
      { initialProps: { runId: "run-1" } },
    );

    // Drive run-1 well past the staleness cutoff so the cap is active.
    expect(result.current("queued")).toBe(POLL_QUEUED_MS);
    clock.advance(STALE_CUTOFF_MS + 5000);
    expect(result.current("queued")).toBe(ADAPTIVE_POLL_CAP_MS);

    // Operator clicked Rerun, the route swapped :runId to run-2. The
    // hook must NOT inherit run-1's "5 min idle" wall-clock; the new
    // queued run should poll at the normal 5s cadence again.
    rerender({ runId: "run-2" });
    expect(result.current("queued")).toBe(POLL_QUEUED_MS);
  });

  it("returns false immediately when status is undefined (initial mount)", () => {
    const clock = makeClock();
    const { result } = renderHook(() => useAdaptivePoll("run-1", clock.now));
    expect(result.current(undefined)).toBe(false);
  });
});
