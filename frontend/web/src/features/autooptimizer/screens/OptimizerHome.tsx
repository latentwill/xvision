import { useEffect, useState, type ReactNode } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { LaunchPanel } from "../ui/LaunchPanel";
import { RecentCyclesTableBody } from "../panels/RecentCyclesTable";
import { ExperimentWritersPanel } from "../panels/ExperimentWritersPanel";
import { EditorialHeadline } from "../ui/EditorialHeadline";
import { ConsoleModule } from "../ui/ConsoleModule";
import { RunStatusBar } from "../ui/RunStatusBar";
import { LineageRiver } from "../ui/LineageRiver";
import { EdgeVsRandomChart } from "../ui/EdgeVsRandomChart";
import { buildHeadline } from "../selectors/buildHeadline";
import { buildDigest, deriveBestFind } from "../selectors/buildDigest";
import {
  useOptimizerStats,
  useCycleRuns,
  useCycleRun,
  useLineageNodes,
  useSchedule,
  usePauseCycle,
  useResumeCycle,
  useCancelCycle,
  type LineageNode,
} from "../api";
import { useLiveActivity } from "../hooks/useLiveActivity";
import { useCycleEventStream } from "../hooks/useCycleEventStream";
import { OptiCapsuleSlot } from "../OptiCapsuleSlot";
import { formatRelativeTime, formatUntil } from "../utils/time";

/** Distinct cycles that still hold an active (kept) node. */
function countActiveLineages(nodes: LineageNode[]): number {
  const seen = new Set<string>();
  for (const n of nodes) {
    if (n.status === "active" && n.cycle_id) seen.add(n.cycle_id);
  }
  return seen.size;
}

// ─── Session scope chip (?session=) ───────────────────────────────────────────

function SessionScopeBanner({ sessionId }: { sessionId: string }) {
  return (
    <div
      role="status"
      aria-label={`Viewing session ${sessionId}`}
      className="rounded-md border border-info/30 bg-info/[0.06] px-4 py-3 flex items-start justify-between gap-4"
    >
      <div>
        <p className="m-0 text-[12px] font-semibold text-info uppercase tracking-wide">
          Session view
        </p>
        <p className="m-0 mt-0.5 font-mono text-[12px] text-text-2">
          Viewing session{" "}
          <span className="text-text select-all">{sessionId}</span>
        </p>
      </div>
      <Link
        to="/optimizer"
        aria-label="Exit session view"
        className="shrink-0 text-[12px] text-text-3 hover:text-text transition-colors"
      >
        Exit session view →
      </Link>
    </div>
  );
}

// ─── Page root ────────────────────────────────────────────────────────────────

export function OptimizerHome() {
  const [params] = useSearchParams();
  const sessionId = params.get("session");

  // The one truthful run signal — server status, live SSE, or an in-flight
  // cycle's persisted log — shared with the console so they never disagree.
  const live = useLiveActivity();
  const stats = useOptimizerStats(sessionId ? { session_id: sessionId } : undefined);
  const cycles = useCycleRuns();
  const lineage = useLineageNodes({ status: "active" });
  const schedule = useSchedule();

  const session = live.session;
  const state = live.activity;
  const isActive = state !== "idle";

  const cycleRows = cycles.data ?? [];
  const hasHistory = cycleRows.length > 0;
  const lastCycle = cycleRows[0] ?? null; // CycleRunSummary — only last_created_at exists
  const lastCycleDetail = useCycleRun(lastCycle?.cycle_id);

  const headline = buildHeadline({
    state,
    activeLineages: countActiveLineages(lineage.data ?? []),
    lastCycle: lastCycle
      ? { kept: lastCycle.active_count, total: lastCycle.node_count }
      : null,
    lastCycleAgo: lastCycle ? formatRelativeTime(lastCycle.last_created_at) : null,
    bestFind: deriveBestFind(stats.data, lastCycle, lastCycleDetail.data),
  });

  const statsRows = stats.data ?? [];
  const digest = buildDigest(statsRows, cycleRows);
  const newestStatsTs = statsRows.reduce<string | null>(
    (newest, r) => (newest == null || r.ts > newest ? r.ts : newest),
    null,
  );

  // Idle + enabled schedule → "next run <relative time>" in the headline area.
  const nextRun =
    !isActive && schedule.data?.enabled && schedule.data.next_run_at
      ? formatUntil(schedule.data.next_run_at)
      : null;

  // Contextual action: Launch (idle) | Pause+Cancel (running) | Resume+Cancel (paused)
  const [launcherOpen, setLauncherOpen] = useState(false);
  useEffect(() => {
    if (isActive) setLauncherOpen(false);
  }, [isActive]);

  // Resolved across live SSE → server status → latest cycle, so Pause/Resume/
  // Cancel stay wired even after a reload mid-run (empty SSE buffer).
  const activeCycleId = live.activeCycleId;
  // Raw SSE buffer for the OPTI trace capsule. useCycleEventStream is a shared
  // singleton, so this is the SAME socket useLiveActivity reads — no second
  // EventSource.
  const { events: cycleEvents, isRunning: streamRunning } = useCycleEventStream();
  const pauseMutation = usePauseCycle();
  const resumeMutation = useResumeCycle();
  const cancelMutation = useCancelCycle();

  const launchButton: ReactNode = !isActive ? (
    <button
      type="button"
      onClick={() => setLauncherOpen((v) => !v)}
      aria-expanded={launcherOpen}
      className={[
        "rounded px-3 py-1.5 text-[13px] font-medium transition-colors",
        launcherOpen
          ? "bg-surface-panel border border-border text-text-2"
          : "bg-accent text-on-accent hover:opacity-90",
      ].join(" ")}
    >
      {launcherOpen ? "Hide launcher" : "Launch run"}
    </button>
  ) : null;

  const cancelButton =
    session != null && activeCycleId != null ? (
      <button
        type="button"
        onClick={() => cancelMutation.mutate(activeCycleId)}
        disabled={cancelMutation.isPending}
        className="rounded border border-danger/40 px-3 py-1.5 text-[13px] text-danger hover:bg-danger/[0.06] transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
      >
        Cancel
      </button>
    ) : null;

  const action: ReactNode =
    state === "running" && session && activeCycleId ? (
      <>
        <button
          type="button"
          onClick={() => pauseMutation.mutate(activeCycleId)}
          disabled={pauseMutation.isPending}
          className="rounded border border-border px-3 py-1.5 text-[13px] text-text-2 hover:bg-surface-elev/40 transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
        >
          Pause
        </button>
        {cancelButton}
      </>
    ) : state === "paused" && session && activeCycleId ? (
      <>
        <button
          type="button"
          onClick={() => resumeMutation.mutate(activeCycleId)}
          disabled={resumeMutation.isPending}
          className="rounded bg-accent px-3 py-1.5 text-[13px] font-medium text-on-accent hover:opacity-90 transition-opacity disabled:opacity-60 disabled:cursor-not-allowed"
        >
          Resume
        </button>
        {cancelButton}
      </>
    ) : hasHistory ? (
      // Idle with history: show the launch button in the headline.
      // Never-ran: no action here — ConsoleModule's NeverRanExplainer is the
      // single owner of the launch button via the launchAction slot below.
      launchButton
    ) : null;

  // Honesty chips: sample sizes for the charts row.
  const attemptCount = cycleRows.reduce((n, c) => n + c.node_count, 0);
  const cycleCount = cycleRows.length;

  return (
    <>
      <Topbar title="Optimizer" />
      {/* WS-11a: the OPTI trace capsule — the live autooptimizer cycle rendered
          on the trace-dock surface. Fed by THIS screen's single cycle SSE
          subscription (no second EventSource). Fixed-position; renders only
          while a cycle is in flight / freshly finished. */}
      <OptiCapsuleSlot
        events={cycleEvents}
        activeCycleId={activeCycleId}
        isRunning={streamRunning}
      />
      <div className="space-y-5">
        <EditorialHeadline headline={headline} digest={digest}>
          {action}
        </EditorialHeadline>

        {/* The unmissable live indicator — answers "is something running right
            now" before anything else on the page. */}
        {isActive && (
          <RunStatusBar
            activity={live.activity}
            source={live.source}
            cycleId={activeCycleId}
            session={session}
            connected={live.connected}
            startedAtMs={live.startedAtMs}
          />
        )}

        {(newestStatsTs || nextRun) && (
          <div className="flex flex-wrap items-center gap-3 font-mono text-[11px] text-text-4 -mt-2">
            {newestStatsTs && <span>as of {formatRelativeTime(newestStatsTs)}</span>}
            {nextRun && (
              <span className="text-text-3">next run {nextRun}</span>
            )}
          </div>
        )}

        {sessionId && <SessionScopeBanner sessionId={sessionId} />}

        {/* Inline launch panel — the launch form extracted from LiveCycleView */}
        {launcherOpen && !isActive && <LaunchPanel />}

        {/* The headline already carries the contextual action; hand the launch
            button to the console only for the never-ran explainer. */}
        <ConsoleModule launchAction={hasHistory ? undefined : launchButton} />

        {/* Charts row — section header carries the honest sample sizes */}
        <section className="space-y-3">
          <div className="text-[11px] uppercase tracking-widest text-text-4">
            Trajectory
            <span className="ml-2 normal-case tracking-normal text-text-3">
              {attemptCount} attempts · {cycleCount} cycle
              {cycleCount === 1 ? "" : "s"}
            </span>
          </div>
          <div className="grid gap-4 lg:grid-cols-2">
            <LineageRiver hasHistory={hasHistory} />
            <div className="rounded-md border border-border bg-surface-card p-5 space-y-3">
              <div className="text-[11px] uppercase tracking-widest text-text-4">
                Edge vs random
              </div>
              <EdgeVsRandomChart rows={statsRows} />
            </div>
          </div>
        </section>

        <ExperimentWritersPanel />

        {/* Cycle history */}
        <section className="rounded-md border border-border bg-surface-card p-5">
          <h2 className="m-0 mb-3 text-[15px] font-semibold tracking-tight">
            Cycle history
          </h2>
          <RecentCyclesTableBody />
        </section>
      </div>
    </>
  );
}
