import { useEffect, useMemo, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useCycleEventStream } from "../hooks/useCycleEventStream";
import { useLiveActivity } from "../hooks/useLiveActivity";
import {
  useCycleEvents,
  useCycleRuns,
  useCycleRun,
  autooptimizerKeys,
} from "../api";
import { normalizePersisted } from "../selectors/narrateEvent";
import { boardFromNodes, buildBoardState } from "../selectors/buildBoardState";
import { PhaseRibbon } from "./PhaseRibbon";
import { ExperimentBoard } from "./ExperimentBoard";
import { NarratedFeed } from "./NarratedFeed";
import { formatRelativeTime } from "../utils/time";

/**
 * The optimizer console: one component, three modes.
 *
 * - **Live** — a cycle is running (and no explicit `cycleId` pins us to a
 *   historic one): ribbon/board/feed driven by the SSE stream.
 * - **Replay** — idle, or an explicit `cycleId` (CycleDetail): the persisted
 *   event log of that cycle (or the most recent completed one) replays
 *   through the exact same selectors, ribbon forced "done". If the event log
 *   is unavailable (pruned, pre-persistence cycle, older backend), the board
 *   falls back to the cycle's lineage nodes — never blank.
 * - **Never-ran** — no cycles exist at all: a four-phase explainer with the
 *   launch action. There is no "waiting for…" state by design.
 */
export function ConsoleModule({
  launchAction,
  cycleId,
  defaultOpenHash,
  expandBoard,
  feedMaxItems,
}: {
  launchAction?: ReactNode;
  cycleId?: string;
  /** `?exp=` deep link: open the matching board card on mount. */
  defaultOpenHash?: string;
  /** Open all board cards on mount. */
  expandBoard?: boolean;
  /** Forwarded to NarratedFeed's `maxItems` (defaults to its own cap). */
  feedMaxItems?: number;
}) {
  const stream = useCycleEventStream();
  const activity = useLiveActivity();
  const cycles = useCycleRuns();
  const queryClient = useQueryClient();

  // Live whenever the unified signal says a run is active (and we're not pinned
  // to a historic cycle). Crucially this no longer hinges on the SSE buffer
  // alone: a CLI run with no IPC bridge — or a tab that joined mid-cycle — still
  // reads as live via the server-status / persisted-event signals.
  const live = !cycleId && activity.activity !== "idle";
  const liveCycleId = activity.activeCycleId;

  // explicit cycleId (CycleDetail) > most recent completed cycle (when idle).
  const replayId = cycleId ?? (!live ? (cycles.data?.[0]?.cycle_id ?? null) : null);

  // Persisted log for whichever cycle we're showing. While live we poll it so a
  // run with no live SSE still streams straight from the DB.
  const persisted = useCycleEvents(
    live ? liveCycleId : replayId,
    live ? { pollMs: 4_000 } : undefined,
  );

  // The live SSE buffer is the freshest source ONLY when it holds this cycle
  // from its cycle_started; otherwise we fall back to the polled persisted log.
  const streamHasLiveCycle = useMemo(() => {
    if (!live || !liveCycleId) return false;
    return stream.events.some((e) => {
      const et = e.event_type ?? e.type ?? e.kind ?? "";
      return et === "cycle_started" && e.cycle_id === liveCycleId;
    });
  }, [live, liveCycleId, stream.events]);

  // Persistence-worker race: when cycle_finished arrives on the stream, the
  // backend's event-persister task may not have drained its queue yet — a
  // fetch right now could cache an empty log for 60s. Invalidate the query
  // so it refetches once the worker has landed the rows.
  const lastFinishedCycleId = useMemo(() => {
    for (let i = stream.events.length - 1; i >= 0; i--) {
      const e = stream.events[i];
      const et = e.event_type ?? e.type ?? e.kind ?? "";
      if (et === "cycle_finished") return e.cycle_id ?? null;
    }
    return null;
  }, [stream.events]);
  useEffect(() => {
    if (lastFinishedCycleId) {
      void queryClient.invalidateQueries({
        queryKey: autooptimizerKeys.cycleEvents(lastFinishedCycleId),
      });
    }
  }, [lastFinishedCycleId, queryClient]);

  const persistedEmpty =
    persisted.isError || (persisted.isSuccess && (persisted.data?.length ?? 0) === 0);

  // Same race on the read side: the stream buffer still holds the finished
  // cycle's events. If the persisted fetch comes back empty for that cycle,
  // replay from the buffer instead of degrading to the node-derived fallback.
  const streamReplayEvents = useMemo(() => {
    if (!replayId) return [];
    const start = stream.events.findIndex((e) => {
      const et = e.event_type ?? e.type ?? e.kind ?? "";
      return et === "cycle_started" && e.cycle_id === replayId;
    });
    return start === -1 ? [] : stream.events.slice(start);
  }, [stream.events, replayId]);
  const useStreamBuffer = !live && persistedEmpty && streamReplayEvents.length > 0;

  // Replay edge case: events pruned / pre-persistence cycle / older backend
  // (and nothing usable in the stream buffer either).
  const eventsUnavailable =
    !live && replayId != null && persistedEmpty && !useStreamBuffer;
  // Fetched unconditionally in replay mode: it's one cached query (CycleDetail
  // shares the key) and gives the header its strategy identity; it doubles as
  // the node-derived board fallback when the event log is unavailable.
  const replayRun = useCycleRun(replayId ?? undefined);

  const events = useMemo(() => {
    if (live) {
      return streamHasLiveCycle
        ? stream.events
        : (persisted.data ?? []).map(normalizePersisted);
    }
    if (useStreamBuffer) return streamReplayEvents;
    return (persisted.data ?? []).map(normalizePersisted);
  }, [
    live,
    streamHasLiveCycle,
    useStreamBuffer,
    streamReplayEvents,
    stream.events,
    persisted.data,
  ]);

  const board = useMemo(() => {
    const fromEvents = buildBoardState(events);
    if (eventsUnavailable && replayRun.data) {
      return {
        ...fromEvents,
        cards: boardFromNodes(replayRun.data.nodes),
        cycleId: replayId,
      };
    }
    return fromEvents;
  }, [events, eventsUnavailable, replayRun.data, replayId]);

  // Header strategy identity for replay: the parent strategy is the most
  // common parent_hash among the cycle's lineage nodes.
  const replayStrategyHash = useMemo(() => {
    const counts = new Map<string, number>();
    for (const n of replayRun.data?.nodes ?? []) {
      if (n.parent_hash) counts.set(n.parent_hash, (counts.get(n.parent_hash) ?? 0) + 1);
    }
    let best: string | null = null;
    let bestCount = 0;
    for (const [hash, count] of counts) {
      if (count > bestCount) {
        best = hash;
        bestCount = count;
      }
    }
    return best ? best.slice(0, 8) : null;
  }, [replayRun.data]);

  if (!live && !cycleId && !cycles.isLoading && (cycles.data?.length ?? 0) === 0) {
    return <NeverRanExplainer launchAction={launchAction} />;
  }

  const replaySummary = cycles.data?.find((c) => c.cycle_id === replayId);
  const lastCycleAgo = formatRelativeTime(replaySummary?.last_created_at);
  const headerCycleId = board.cycleId ?? activity.activeCycleId ?? stream.activeCycleId;
  const liveStrategyId = activity.session?.strategy_id;
  const replayId8 = replayId ? replayId.slice(0, 8) : null;

  // The header label tracks the real activity so a paused/cancelling run never
  // reads "Live".
  const liveLabel =
    activity.activity === "paused"
      ? "Paused"
      : activity.activity === "cancelling"
        ? "Cancelling"
        : "Live";
  const liveTone =
    activity.activity === "running"
      ? "text-gold"
      : activity.activity === "paused"
        ? "text-warn"
        : "text-danger";

  return (
    <section
      className={[
        "space-y-4 rounded-md border bg-surface-card p-5 transition-colors",
        live && activity.activity === "running"
          ? "border-gold/30"
          : live && activity.activity === "paused"
            ? "border-warn/30"
            : live
              ? "border-danger/30"
              : "border-border",
      ].join(" ")}
    >
      <div className="flex items-center justify-between gap-3">
        <div className="text-[11px] uppercase tracking-widest text-text-4">
          {live ? (
            <span className={liveTone}>
              {liveLabel} · cycle{" "}
              <span className="font-mono">
                {headerCycleId ? headerCycleId.slice(0, 8) : "…"}
              </span>
              {liveStrategyId ? (
                <>
                  {" · "}
                  <span className="font-mono">{liveStrategyId}</span>
                </>
              ) : null}
            </span>
          ) : (
            <>
              Last cycle
              {replayId8 ? (
                <>
                  {" · "}
                  <span className="font-mono">{replayId8}</span>
                </>
              ) : null}
              {replayStrategyHash ? (
                <>
                  {" · strategy "}
                  <span className="font-mono">{replayStrategyHash}</span>
                </>
              ) : null}
              {lastCycleAgo ? ` · ${lastCycleAgo}` : null}
            </>
          )}
        </div>
        {/* Intentional extension point — CycleDetail / home may populate this in the live/replay header. */}
        {launchAction}
      </div>
      <PhaseRibbon
        phase={live ? board.phase : "done"}
        running={live && activity.activity === "running"}
      />
      <ExperimentBoard
        cards={board.cards}
        defaultOpenHash={defaultOpenHash}
        expandBoard={expandBoard}
      />
      {/* No feed when the event log is unavailable — the node-derived board
          alone is the content; never surface an apology line. */}
      {!eventsUnavailable && <NarratedFeed events={events} maxItems={feedMaxItems} />}
    </section>
  );
}

function NeverRanExplainer({ launchAction }: { launchAction?: ReactNode }) {
  const phases: [string, string][] = [
    ["Propose", "Experiment writers draft variations of your strategy."],
    ["Eval", "Each experiment is backtested across regimes."],
    ["Gate", "A gate compares each result to its parent — honestly."],
    ["Keep", "Winners join the lineage; the rest are recorded and rejected."],
  ];
  return (
    <section className="space-y-4 rounded-md border border-border bg-surface-card p-5">
      <p className="text-[14px] text-text-2">Each cycle runs four phases:</p>
      <div className="grid gap-3 sm:grid-cols-4">
        {phases.map(([t, d]) => (
          <div key={t} className="rounded-sm border border-border-soft p-3">
            <div className="text-[11px] uppercase tracking-widest text-gold">{t}</div>
            <p className="mt-1 text-[12px] text-text-3">{d}</p>
          </div>
        ))}
      </div>
      {launchAction}
    </section>
  );
}
