import { useEffect, useMemo, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useCycleEventStream } from "../hooks/useCycleEventStream";
import { useCycleEvents, useCycleRuns, useCycleRun, autooptimizerKeys } from "../api";
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
  const cycles = useCycleRuns();
  const queryClient = useQueryClient();

  const live = stream.isRunning && !cycleId;
  // explicit cycleId (CycleDetail) > live cycle > most recent completed cycle
  const replayId =
    cycleId ?? (!stream.isRunning ? (cycles.data?.[0]?.cycle_id ?? null) : null);
  const persisted = useCycleEvents(live ? null : replayId);

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
  const fallbackRun = useCycleRun(eventsUnavailable ? replayId : undefined);

  const events = useMemo(() => {
    if (live) return stream.events;
    if (useStreamBuffer) return streamReplayEvents;
    return (persisted.data ?? []).map(normalizePersisted);
  }, [live, useStreamBuffer, streamReplayEvents, stream.events, persisted.data]);

  const board = useMemo(() => {
    const fromEvents = buildBoardState(events);
    if (eventsUnavailable && fallbackRun.data) {
      return {
        ...fromEvents,
        cards: boardFromNodes(fallbackRun.data.nodes),
        cycleId: replayId,
      };
    }
    return fromEvents;
  }, [events, eventsUnavailable, fallbackRun.data, replayId]);

  if (!live && !cycleId && !cycles.isLoading && (cycles.data?.length ?? 0) === 0) {
    return <NeverRanExplainer launchAction={launchAction} />;
  }

  const replaySummary = cycles.data?.find((c) => c.cycle_id === replayId);
  const lastCycleAgo = formatRelativeTime(replaySummary?.last_created_at);

  return (
    <section className="space-y-4 rounded-md border border-border bg-surface-card p-5">
      <div className="flex items-center justify-between gap-3">
        <div className="text-[11px] uppercase tracking-widest text-text-4">
          {live ? (
            <span className="text-gold">
              Live · cycle {board.cycleId ?? stream.activeCycleId ?? "…"}
            </span>
          ) : (
            <>Last cycle{lastCycleAgo ? ` · ${lastCycleAgo}` : ""}</>
          )}
        </div>
        {/* Intentional extension point — CycleDetail / home may populate this in the live/replay header. */}
        {launchAction}
      </div>
      <PhaseRibbon phase={live ? board.phase : "done"} />
      <ExperimentBoard
        cards={board.cards}
        defaultOpenHash={defaultOpenHash}
        expandBoard={expandBoard}
      />
      {eventsUnavailable ? (
        <p className="font-mono text-[12px] text-text-4">
          Event log unavailable for this cycle.
        </p>
      ) : (
        <NarratedFeed events={events} maxItems={feedMaxItems} />
      )}
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
