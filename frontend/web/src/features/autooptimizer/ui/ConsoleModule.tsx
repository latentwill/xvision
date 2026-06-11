import { useMemo, type ReactNode } from "react";
import { useCycleEventStream } from "../hooks/useCycleEventStream";
import { useCycleEvents, useCycleRuns, useCycleRun } from "../api";
import { normalizePersisted } from "../selectors/narrateEvent";
import { boardFromNodes, buildBoardState } from "../selectors/buildBoardState";
import { PhaseRibbon } from "./PhaseRibbon";
import { ExperimentBoard } from "./ExperimentBoard";
import { NarratedFeed } from "./NarratedFeed";

function formatRelativeTime(iso?: string): string {
  if (!iso) return "";
  try {
    const diffMs = Date.now() - new Date(iso).getTime();
    const diffMin = Math.floor(diffMs / 60_000);
    if (diffMin < 1) return "just now";
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr}h ago`;
    return `${Math.floor(diffHr / 24)}d ago`;
  } catch {
    return iso;
  }
}

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
}: {
  launchAction?: ReactNode;
  cycleId?: string;
  /** `?exp=` deep link: open the matching board card on mount. */
  defaultOpenHash?: string;
  /** Open all board cards on mount. */
  expandBoard?: boolean;
}) {
  const stream = useCycleEventStream();
  const cycles = useCycleRuns();

  const live = stream.isRunning && !cycleId;
  // explicit cycleId (CycleDetail) > live cycle > most recent completed cycle
  const replayId =
    cycleId ?? (!stream.isRunning ? (cycles.data?.[0]?.cycle_id ?? null) : null);
  const persisted = useCycleEvents(live ? null : replayId);

  // Replay edge case: events pruned / pre-persistence cycle / older backend.
  const eventsUnavailable =
    !live &&
    replayId != null &&
    (persisted.isError || (persisted.isSuccess && (persisted.data?.length ?? 0) === 0));
  const fallbackRun = useCycleRun(eventsUnavailable ? replayId : undefined);

  const events = useMemo(() => {
    if (live) return stream.events;
    return (persisted.data ?? []).map(normalizePersisted);
  }, [live, stream.events, persisted.data]);

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
        <NarratedFeed events={events} />
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
