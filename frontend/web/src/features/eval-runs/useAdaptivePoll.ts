/**
 * Status-aware adaptive polling for `eval.get_run` (and any other view
 * that watches a per-run status).
 *
 * Background — 2026-05-19 `api_audit` logged 890 `eval.get_run` calls
 * against 64 `eval.start` calls in the audit window. The detail view's
 * previous fixed 2s tick (running OR queued) burned ~14× the reads a
 * typical "operator-watching-a-backtest" workload actually needs. This
 * hook replaces that with a status-aware schedule:
 *
 *   - `running`    → 2s   (active progress; the chart and decisions tail
 *                          are also live, so frequent refresh is justified)
 *   - `queued`     → 5s   (waiting on the executor; no UI to update)
 *   - terminal     → false (stop polling; tanstack-query will not refetch)
 *   - 5min no-change → 30s (operator left the tab open on a stale queue;
 *                            don't keep hitting the DB)
 *
 * Floor is 1s — never schedule a faster tick than that.
 *
 * Usage:
 *
 *   const pollFor = useAdaptivePoll(runId);
 *   useQuery({
 *     ...
 *     refetchInterval: (query) => pollFor(query.state.data?.summary.status),
 *   });
 *
 * `pollFor` has a stable identity across renders, so the query option
 * doesn't churn. Internally it tracks "ms since the last status
 * transition" via refs, so the 5-min staleness backoff still fires when
 * no re-render happens (a queued run sitting idle overnight).
 */
import { useCallback, useRef } from "react";

const QUEUED_STATUSES = new Set(["queued"]);
const RUNNING_STATUSES = new Set(["running"]);
const TERMINAL_STATUSES = new Set(["completed", "failed", "cancelled"]);

/** Floor (ms). Never tick faster than this. */
export const ADAPTIVE_POLL_FLOOR_MS = 1000;
/** Cap (ms). After `STALE_CUTOFF_MS` of no state change, slow to this. */
export const ADAPTIVE_POLL_CAP_MS = 30_000;
/** Time of no-status-change after which we back off to the cap. */
export const STALE_CUTOFF_MS = 5 * 60 * 1000;

export const POLL_RUNNING_MS = 2000;
export const POLL_QUEUED_MS = 5000;

export type RunStatusInput = string | null | undefined;

/**
 * Pure interval calculator — the heart of the hook. Lifted out so it can
 * be unit-tested without React/Date plumbing.
 *
 * Returns `false` for terminal status (stops polling). Returns a number
 * of milliseconds otherwise, clamped to `[ADAPTIVE_POLL_FLOOR_MS,
 * ADAPTIVE_POLL_CAP_MS]`.
 */
export function adaptivePollInterval(
  status: RunStatusInput,
  msSinceLastStatusChange: number,
): number | false {
  if (!status) {
    // Unknown status — no detail loaded yet. Don't schedule a tick;
    // the initial fetch is what tanstack-query fires on mount, and
    // once that lands the caller's query state-data picks up the real
    // status and the next tick is scheduled.
    return false;
  }
  if (TERMINAL_STATUSES.has(status)) {
    return false;
  }

  if (msSinceLastStatusChange >= STALE_CUTOFF_MS) {
    return ADAPTIVE_POLL_CAP_MS;
  }

  let base: number;
  if (RUNNING_STATUSES.has(status)) {
    base = POLL_RUNNING_MS;
  } else if (QUEUED_STATUSES.has(status)) {
    base = POLL_QUEUED_MS;
  } else {
    // Unknown but non-terminal — be conservative and poll at the queued
    // cadence so the UI still updates if the engine adds a new
    // non-terminal phase (e.g. `paused`) without crashing the hook.
    base = POLL_QUEUED_MS;
  }

  return Math.max(ADAPTIVE_POLL_FLOOR_MS, Math.min(ADAPTIVE_POLL_CAP_MS, base));
}

/**
 * Reusable hook returning a stable `(status) => interval | false`
 * function for use as a tanstack-query `refetchInterval` callback.
 *
 * Tracks the last-seen status and the wall-clock time of the last
 * transition via refs, so the 5-minute staleness cap kicks in even
 * while the React tree is idle (no re-renders).
 *
 * `runId` is taken so the hook resets its staleness timer when the
 * route navigates to a different run without unmounting (e.g. the
 * "rerun" flow that swaps `:runId` in place). Pass `null` / `""` when
 * no run is selected.
 *
 * `nowFn` is a test seam — defaults to `Date.now`.
 */
export function useAdaptivePoll(
  runId: string | null | undefined,
  nowFn: () => number = Date.now,
): (status: RunStatusInput) => number | false {
  const lastStatusRef = useRef<RunStatusInput>(undefined);
  const lastChangeAtRef = useRef<number>(nowFn());
  const lastRunIdRef = useRef<string | null | undefined>(runId);
  const nowFnRef = useRef(nowFn);
  nowFnRef.current = nowFn;

  return useCallback(
    (status: RunStatusInput) => {
      const now = nowFnRef.current();
      if (lastRunIdRef.current !== runId) {
        // Navigated to a new run — reset the staleness timer so we don't
        // inherit the previous run's "5 min idle" wall-clock.
        lastRunIdRef.current = runId;
        lastStatusRef.current = status;
        lastChangeAtRef.current = now;
      } else if (status !== lastStatusRef.current) {
        lastStatusRef.current = status;
        lastChangeAtRef.current = now;
      }
      const elapsed = now - lastChangeAtRef.current;
      return adaptivePollInterval(status, elapsed);
    },
    [runId],
  );
}
