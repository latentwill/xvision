import type { Phase } from "./buildBoardState";

/** The unified "what is the optimizer doing right now" verdict. */
export type Activity = "running" | "paused" | "cancelling" | "idle";

/**
 * Which signal proved the verdict — ordered by authority/controllability:
 * - `status`  the DB session row (`autooptimizer_session_state`). Authoritative
 *             and controllable (Pause/Cancel act on it). Survives reloads and is
 *             cross-process — set by both the dashboard Run button and `xvn
 *             optimize run`.
 * - `stream`  the live SSE buffer saw a cycle_started without a later finish.
 * - `events`  the most recent cycle's *persisted* event log is in-flight
 *             (cycle_started, no cycle_finished) and fresh. This is the resilient
 *             fallback for a CLI run with no IPC bridge, a tab that joined
 *             mid-cycle, or an older CLI that doesn't write the session row.
 * - `none`    idle.
 */
export type ActivitySource = "status" | "stream" | "events" | "none";

/**
 * A run whose only evidence is an unfinished cycle's event log is treated as
 * *stalled* (idle), not running, once its newest telemetry is older than this.
 * Generous on purpose: a single cycle's LLM proposals + multi-regime backtests
 * routinely take minutes. The authoritative `status` signal is never subject to
 * this bound — only the inferred `events` fallback.
 */
export const RUNNING_STALE_MS = 20 * 60_000;

export interface DeriveActivityInput {
  /** `status.active_session.state`, if any. */
  sessionState?: string | null;
  /** SSE buffer shows a cycle_started with no later cycle_finished. */
  streamRunning: boolean;
  /** Phase derived from the most recent cycle's persisted event log. */
  latestPhase?: Phase;
  /** That log contains a `cycle_started`. */
  latestHasStarted?: boolean;
  /** ms since that log's newest event; `null`/absent when unknown. */
  latestAgeMs?: number | null;
}

const CONTROLLABLE = new Set<Activity>(["running", "paused", "cancelling"]);

/**
 * Collapse the three running signals into one truthful verdict. Pure: callers
 * supply already-fetched signals so this stays trivially testable.
 */
export function deriveActivity(i: DeriveActivityInput): {
  activity: Activity;
  source: ActivitySource;
} {
  const state = (i.sessionState ?? "") as Activity;
  if (CONTROLLABLE.has(state)) return { activity: state, source: "status" };

  if (i.streamRunning) return { activity: "running", source: "stream" };

  // Inferred fallback: an unfinished latest cycle with *recent* telemetry. We
  // require a real age (never fabricate "running" from an unknown age) and we
  // bound staleness so a crashed run that never emitted cycle_finished doesn't
  // pin the UI to "running" forever.
  const unfinished =
    i.latestHasStarted === true && i.latestPhase != null && i.latestPhase !== "done";
  const fresh = i.latestAgeMs != null && i.latestAgeMs <= RUNNING_STALE_MS;
  if (unfinished && fresh) return { activity: "running", source: "events" };

  return { activity: "idle", source: "none" };
}
