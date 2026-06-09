// Transport optimistic-cache helpers (Task B-III / spec §2.7).
//
// The strategy strip derives each pill's status from `AgentRunSummary` rows
// served by `listAgentRuns()` (query key `agentRunKeys.list()`), polled every
// 10s. The transport mutations (pause/resume/flatten/cancel) hit the eval
// endpoints and return the AUTHORITATIVE eval `RunSummary`. To avoid the strip
// lagging by up to one poll, we optimistically patch the cached
// `AgentRunSummary[]` the instant a mutation fires, then reconcile from the
// returned `RunSummary` on success (and revert on error).
//
// These are pure functions so the flip/merge logic is unit-tested without a
// React tree or a live QueryClient.

import type { AgentRunSummary } from "@/api/types-agent-runs";
import type { RunSummary } from "@/api/types.gen";

/** The optimistic field patch a transport action applies to a cached run. */
export type TransportPatch = Partial<
  Pick<AgentRunSummary, "status" | "paused" | "flatten_requested">
>;

/** Optimistic patches per transport action (applied before the server responds). */
export const OPTIMISTIC_PATCH = {
  pause: { paused: true } as TransportPatch,
  resume: { paused: false } as TransportPatch,
  // Flatten does NOT change paused/terminal status — the run stays paused and
  // alive; only the pending-flatten flag flips so the UI can show "flattening…".
  flatten: { flatten_requested: true } as TransportPatch,
  // Cancel/stop is terminal. We optimistically mark the run cancelled so the
  // pill flips to STOPPED immediately; the poll reconciles the real status.
  stop: { status: "cancelled" } as TransportPatch,
} as const;

export type TransportAction = keyof typeof OPTIMISTIC_PATCH;

/**
 * Apply a field patch to the matching run in a cached `AgentRunSummary[]`,
 * returning a NEW array (referentially fresh so TanStack Query / React
 * re-render). Non-matching rows are returned untouched. A `null`/`undefined`
 * cache (query not yet populated) yields the same value back so the optimistic
 * update is a safe no-op until the first poll lands.
 */
export function patchRunInList(
  list: AgentRunSummary[] | undefined,
  runId: string,
  patch: TransportPatch,
): AgentRunSummary[] | undefined {
  if (!list) return list;
  let changed = false;
  const next = list.map((r) => {
    if (r.run_id !== runId) return r;
    changed = true;
    return { ...r, ...patch };
  });
  return changed ? next : list;
}

/**
 * Restore a single run's row to a prior snapshot, returning a NEW array if the
 * run is present. Used for error rollback: instead of clobbering the whole
 * cached list (which would wipe a CONCURRENT optimistic patch on a different
 * run), we re-apply only the failing run's pre-mutation row. If `priorRow` is
 * `undefined` (the run wasn't in the cache when the mutation fired), the list
 * is returned untouched — there is nothing to restore. Non-matching rows are
 * left as-is so other runs' in-flight optimistic patches survive.
 */
export function restoreRunInList(
  list: AgentRunSummary[] | undefined,
  runId: string,
  priorRow: AgentRunSummary | undefined,
): AgentRunSummary[] | undefined {
  if (!list) return list;
  if (priorRow === undefined) return list;
  let changed = false;
  const next = list.map((r) => {
    if (r.run_id !== runId) return r;
    changed = true;
    return priorRow;
  });
  return changed ? next : list;
}

/**
 * Reconcile a cached `AgentRunSummary` from the authoritative eval
 * `RunSummary` the mutation returned. The eval row is the source of truth for
 * `status` / `paused` / `flatten_requested`; everything else on the agent-run
 * summary (span counts, tokens, objective, …) is preserved. The eval
 * `RunSummary.id` maps to `AgentRunSummary.run_id`.
 */
export function reconcileFromRunSummary(
  list: AgentRunSummary[] | undefined,
  authoritative: RunSummary,
): AgentRunSummary[] | undefined {
  if (!list) return list;
  let changed = false;
  const next = list.map((r) => {
    if (r.run_id !== authoritative.id) return r;
    changed = true;
    return {
      ...r,
      status: authoritative.status as AgentRunSummary["status"],
      paused: authoritative.paused,
      flatten_requested: authoritative.flatten_requested,
    };
  });
  return changed ? next : list;
}
