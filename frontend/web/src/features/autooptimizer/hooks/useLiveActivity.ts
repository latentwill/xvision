import { useMemo } from "react";
import {
  useCycleEvents,
  useCycleRuns,
  useOptimizerStatus,
  type CycleProgressEvent,
  type SessionSummary,
} from "../api";
import { buildBoardState } from "../selectors/buildBoardState";
import { normalizePersisted } from "../selectors/narrateEvent";
import {
  deriveActivity,
  RUNNING_STALE_MS,
  type Activity,
  type ActivitySource,
} from "../selectors/deriveActivity";
import { useCycleEventStream } from "./useCycleEventStream";

/** Poll cadence for the persisted-events catch-up while a run is plausibly live. */
const LIVE_POLL_MS = 4_000;

export interface LiveActivity {
  /** The one truthful verdict, shared across every optimizer surface. */
  activity: Activity;
  /** Which signal proved it (see {@link ActivitySource}). */
  source: ActivitySource;
  /** Best-effort id of the in-flight cycle, resolved across all three signals. */
  activeCycleId: string | null;
  /** The controllable session row — present only when `status` knows the run.
   *  Pause/Resume/Cancel act on this; absent for an inferred (`events`) run. */
  session: SessionSummary | null;
  /** SSE connected → events push live; otherwise the console polls the DB. */
  connected: boolean;
  /** Epoch ms the active cycle started, if derivable from telemetry (for elapsed). */
  startedAtMs: number | null;
}

function kindOf(e: CycleProgressEvent): string {
  return (e.type ?? e.event_type ?? e.kind ?? "") as string;
}

/**
 * The single source of truth for "what is the optimizer doing right now".
 *
 * Collapses three signals into one verdict so no two surfaces can disagree:
 *  1. the DB session row (`/status`) — authoritative + controllable, survives
 *     reloads, cross-process (dashboard *and* `xvn optimize run` both write it);
 *  2. the live SSE buffer — instant, but empty for a tab that joined mid-run;
 *  3. the most recent cycle's *persisted* event log — the resilient fallback
 *     that catches a CLI run with no IPC bridge (the case the operator hit).
 *
 * The persisted log is also the telemetry the console replays when the live SSE
 * stream is empty, so it's polled while a run is plausibly active.
 */
export function useLiveActivity(): LiveActivity {
  const status = useOptimizerStatus();
  const stream = useCycleEventStream();
  const cycles = useCycleRuns();
  const latest = cycles.data?.[0] ?? null;
  const latestCycleId = latest?.cycle_id ?? null;

  const sessionState = status?.active_session?.state ?? null;
  const statusActive =
    sessionState === "running" ||
    sessionState === "paused" ||
    sessionState === "cancelling";

  // Gate the events poll so an idle page with old history doesn't poll forever:
  // poll only when status/stream prove a run is active, OR the latest cycle was
  // touched recently enough to plausibly still be in flight (cheap recency check
  // off the cycle summary — no event fetch required to decide). When idle with a
  // stale last cycle we fetch the log once (to confirm "done") and stop.
  const latestTouchedMs = latest?.last_created_at
    ? Date.now() - new Date(latest.last_created_at).getTime()
    : null;
  const latestIsRecent = latestTouchedMs != null && latestTouchedMs <= RUNNING_STALE_MS;
  const shouldPoll = statusActive || stream.isRunning || latestIsRecent;
  const latestEvents = useCycleEvents(
    latestCycleId,
    shouldPoll ? { pollMs: LIVE_POLL_MS } : undefined,
  );

  const normalized = useMemo(
    () => (latestEvents.data ?? []).map(normalizePersisted),
    [latestEvents.data],
  );

  const { phase, hasStarted, startedAtMs, newestTs } = useMemo(() => {
    const board = buildBoardState(normalized);
    let started: number | null = null;
    let newest: string | null = null;
    let seenStart = false;
    for (const e of normalized) {
      if (kindOf(e) === "cycle_started") {
        seenStart = true;
        const t = e.ts ? new Date(e.ts).getTime() : NaN;
        if (Number.isFinite(t)) started = t;
      }
      if (e.ts && (newest == null || e.ts > newest)) newest = e.ts;
    }
    return {
      phase: board.phase,
      hasStarted: seenStart,
      startedAtMs: started,
      newestTs: newest,
    };
  }, [normalized]);

  const latestAgeMs = newestTs ? Date.now() - new Date(newestTs).getTime() : null;

  const { activity, source } = deriveActivity({
    sessionState,
    streamRunning: stream.isRunning,
    latestPhase: phase,
    latestHasStarted: hasStarted,
    latestAgeMs,
  });

  const activeCycleId =
    stream.activeCycleId ??
    status?.active_cycle_id ??
    (activity !== "idle" ? latestCycleId : null);

  return {
    activity,
    source,
    activeCycleId,
    session: status?.active_session ?? null,
    connected: stream.connected,
    startedAtMs: activity !== "idle" ? startedAtMs : null,
  };
}
