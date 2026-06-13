// frontend/web/src/components/home/ActiveTasksStrip.tsx
//
// nsk BOUNDARY NOTE (Control Tower s78): this is the SINGLE home for per-run
// live/paper ROWS (CT5 LiveDeploymentSummary contract, beads n0k + awm) — each
// deployment's running P&L / last decision renders as a row in a distinct,
// labeled "Live & paper" group, ALONGSIDE the eval QUEUE (in-flight eval runs +
// the optimizer cycle). The aggregate live COUNT ("N live") lives in
// LiveSummaryStrip — that strip answers "how many are live?", this one answers
// "what is each task doing now?". Do not duplicate the aggregate live count
// here.
//
// HONESTY (CT5 §11): every nullable LiveDeploymentSummary field renders "—" /
// "no data", NEVER a fabricated 0 / $0. Capital / P&L / drawdown have a TWO
// source contract:
//   - the honest 5s POLL (`GET /api/live/deployments`), passed in as the
//     `deployments` prop from the home route, is the list-membership source of
//     truth AND the degrade floor for every capital field;
//   - the per-deployment SSE (`openDeploymentStream`, CT5 §4 / bead s78.1)
//     OVERLAYS the live-ticking capital block (esp. unrealized P&L) on top of
//     the poll value. The streamed numbers are the SAME honest book/execution
//     values; a field with no real data is OMITTED on the wire (stays
//     undefined), so it falls back to the poll value — never a blank / 0 flash.
//     On stream drop / close the overlay clears and the poll value shows again.
// `mode` (paper/live) comes from the deployment's `mode` field, never inferred.
//
// Shows in-flight eval runs (queued | running) with elapsed time, stuck warning, cancel,
// plus the running optimizer cycle (if any) with pause/resume controls (S0 / O2+O3),
// plus the live/paper deployment rows (n0k/awm).

import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { cancelRun, evalKeys, flattenRun, listRuns } from "@/api/eval";
import {
  openDeploymentStream,
  type DeploymentMetricsPatch,
} from "@/api/live-deployments";
import type { LiveDeploymentSummary, RunSummary } from "@/api/types.gen";
import { fmtUsdSigned, pnlTone } from "@/features/live/live-format";
import { formatEta } from "@/features/live/deployment-risk";
import { formatRelativeTime } from "@/features/home/pulse";
import {
  useOptimizerStatus,
  usePauseCycle,
  useResumeCycle,
  type SessionSummary,
} from "@/features/autooptimizer/api";

const TWO_HOURS_MS = 2 * 60 * 60 * 1000;
// awm (b): a LIVE (real-money) deployment still running past this threshold is
// surfaced with a runaway-warning chip. Frontend-only check on `started_at` +
// `mode`, mirroring the eval-queue stuck heuristic above.
const RUNAWAY_THRESHOLD_MS = 24 * 60 * 60 * 1000;

function formatElapsed(startedAt: string | null | undefined): string {
  if (!startedAt) return "—";
  const ms = Date.now() - new Date(startedAt).getTime();
  if (ms < 0) return "—";
  const totalSecs = Math.floor(ms / 1000);
  const hours = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  const secs = totalSecs % 60;
  if (hours > 0) {
    return `${hours}h ${mins}m`;
  }
  return `${mins}m ${secs}s`;
}

function isStuck(run: RunSummary): boolean {
  if (run.status !== "running") return false;
  if (!run.started_at) return false;
  return Date.now() - new Date(run.started_at).getTime() > TWO_HOURS_MS;
}

/// awm (b): a LIVE (real-money) deployment whose `started_at` is older than the
/// runaway threshold. Paper deployments are never flagged — the warning targets
/// real money. `mode` is read off the contract field, never inferred.
function isRunaway(dep: LiveDeploymentSummary): boolean {
  if (dep.mode !== "live") return false;
  if (!dep.started_at) return false;
  const t = new Date(dep.started_at).getTime();
  if (!Number.isFinite(t)) return false;
  return Date.now() - t > RUNAWAY_THRESHOLD_MS;
}

/// awm (a): the Stop control is shown for human-queued AND legacy runs
/// (source absent/undefined), and HIDDEN only on an explicit optimizer source.
/// Optimizer-driven deployments are managed by the cycle, not flattened here.
function canStopDeployment(dep: LiveDeploymentSummary): boolean {
  return dep.source !== "optimizer";
}

function statusPillClass(status: string): string {
  switch (status) {
    case "running":
      return "bg-blue-500/15 text-blue-700 dark:text-blue-300";
    case "queued":
    case "starting":
      return "bg-yellow-500/15 text-yellow-700 dark:text-yellow-300";
    case "paused":
      return "bg-amber-500/15 text-amber-700 dark:text-amber-300";
    case "failed":
      return "bg-red-500/15 text-red-700 dark:text-red-300";
    default:
      return "bg-surface-elev text-text-3";
  }
}

function modeBadgeClass(mode: string): string {
  // LIVE reads as the app's canonical info/blue badge (matches
  // TrajectoryModeBadge); amber stays reserved for WARNING states (the runaway
  // chip + paused/starting pills on this same row), so one color = one meaning.
  return mode === "live"
    ? "bg-blue-500/15 text-blue-700 dark:text-blue-300"
    : "bg-surface-elev text-text-3";
}

function RunRow({ run }: { run: RunSummary }) {
  const queryClient = useQueryClient();

  const cancel = useMutation({
    mutationFn: () => cancelRun(run.id),
    onSettled: () => {
      // Optimistic invalidation: refetch inflight runs
      queryClient.invalidateQueries({ queryKey: evalKeys.runs({ status: "queued,running" }) });
    },
  });

  const strategyName =
    run.strategy?.display_name?.trim() || "Unknown strategy";
  const stuck = isStuck(run);

  return (
    <div className="flex items-center gap-3 py-2 px-3 rounded-md hover:bg-surface-hover transition-colors min-w-0">
      {/* Strategy name links to run detail */}
      <Link
        to={`/eval-runs/${run.id}`}
        className="text-[13px] font-medium text-text hover:underline truncate min-w-0 flex-1"
      >
        {strategyName}
      </Link>

      {/* Status pill */}
      <span
        className={`shrink-0 text-[11px] font-medium px-2 py-0.5 rounded-full ${statusPillClass(run.status)}`}
      >
        {run.status}
      </span>

      {/* Elapsed time */}
      <span className="shrink-0 text-[12px] text-text-3 font-mono tabular-nums">
        {formatElapsed(run.started_at)}
      </span>

      {/* Stuck warning */}
      {stuck && (
        <span className="shrink-0 text-[11px] font-medium px-2 py-0.5 rounded-full bg-amber-500/15 text-amber-700 dark:text-amber-300">
          ⚠ may be stuck
        </span>
      )}

      {/* Cancel button — rendered for all runs */}
      {/* TODO: gate on human-queued only when RunSummary.source field exists */}
      <button
        type="button"
        disabled={cancel.isPending}
        onClick={() => cancel.mutate()}
        className="shrink-0 text-[12px] text-text-3 hover:text-text disabled:opacity-50 px-2 py-0.5 rounded border border-border hover:border-border-strong transition-colors"
        aria-label={`Cancel ${strategyName}`}
      >
        {cancel.isPending ? "Cancelling…" : "Cancel"}
      </button>
    </div>
  );
}

/// s78.1: subscribe to a deployment's SSE and keep the latest live capital
/// patch. Only LIVE / paper deployments that are actually running stream — a
/// stopped row never opens a socket. The patch holds ONLY the capital fields the
/// backend sent on the wire (omitted-null fields stay undefined), so a consumer
/// overlays present fields and falls back to the poll for the rest. On unmount /
/// id change the stream closes (no leak) and the overlay clears, so the row
/// degrades to the poll value with no blank / 0 flash.
function useDeploymentLiveMetrics(
  id: string,
  streaming: boolean,
): DeploymentMetricsPatch | null {
  const [patch, setPatch] = useState<DeploymentMetricsPatch | null>(null);

  useEffect(() => {
    if (!streaming) {
      setPatch(null);
      return;
    }
    // New subscription target: clear any stale overlay so the poll value shows
    // until the first live tick lands (no carry-over from a previous id).
    setPatch(null);
    const close = openDeploymentStream(id, (ev) => {
      if (ev.event === "metrics") {
        // Merge so a later equity-only heartbeat does not wipe the capital
        // fields a prior full tick delivered; present fields overlay, absent
        // ones keep their last live value (still honest — last real number).
        setPatch((prev) => ({ ...(prev ?? {}), ...ev.data }));
      }
    });
    return () => {
      close();
      // Stream torn down — drop the overlay so the row falls back to the poll.
      setPatch(null);
    };
  }, [id, streaming]);

  return patch;
}

/// Pick the honest value to render: the live-streamed field when the SSE
/// delivered one this session, else the poll value. NEVER fabricates a `0` —
/// `null`/`undefined` on both sides stays `null` (rendered "—").
function liveOrPoll(
  live: number | null | undefined,
  poll: number | null,
): number | null {
  return live ?? poll;
}

/**
 * n0k / awm / s78.1: one live/paper deployment row. List membership + the
 * capital floor come from the 5s poll (`LiveDeploymentSummary`); the live
 * unrealized P&L ticks via the per-deployment SSE and overlays the poll value,
 * degrading back to it on stream drop. Capital/P&L are honesty-constrained — a
 * `null` unrealized P&L renders "—", never a fabricated $0.
 */
function DeploymentRow({ dep }: { dep: LiveDeploymentSummary }) {
  const queryClient = useQueryClient();

  const flatten = useMutation({
    mutationFn: () => flattenRun(dep.deployment_id),
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: ["live-deployments"] });
      queryClient.invalidateQueries({ queryKey: evalKeys.runs({ status: "queued,running" }) });
    },
  });

  // Only stream while the deployment is in a live, non-terminal state. A
  // paused/stopped row has no live ticks to overlay; keep it on the poll.
  const streaming = dep.status === "running";
  const live = useDeploymentLiveMetrics(dep.deployment_id, streaming);

  // Live-ticking unrealized P&L overlaid on the poll; "—" when both are null.
  const unrealizedPnl = liveOrPoll(live?.unrealized_pnl_usd, dep.unrealized_pnl_usd);

  const strategyName = dep.strategy_name?.trim() || "Unknown strategy";
  const decisionAgo = formatRelativeTime(dep.last_decision_at);
  const eta = formatEta(dep.stop_at);
  const runaway = isRunaway(dep);
  const showStop =
    canStopDeployment(dep) && dep.status !== "stopped" && dep.status !== "failed";

  return (
    <div
      data-testid={`deployment-row-${dep.deployment_id}`}
      className="flex items-center gap-3 py-2 px-3 rounded-md hover:bg-surface-hover transition-colors min-w-0"
    >
      {/* Mode badge — paper | live, straight off the contract field. */}
      <span
        className={`shrink-0 text-[11px] font-medium px-2 py-0.5 rounded-full ${modeBadgeClass(dep.mode)}`}
      >
        {dep.mode}
      </span>

      {/* Strategy name links to the live run inspector. `deployment_id` is the
          eval_runs.id (CT5 §2.1), so the detail route is /live/runs/:runId. */}
      <Link
        to={`/live/runs/${dep.deployment_id}`}
        className="text-[13px] font-medium text-text hover:underline truncate min-w-0 flex-1"
      >
        {strategyName}
      </Link>

      {/* Status pill */}
      <span
        className={`shrink-0 text-[11px] font-medium px-2 py-0.5 rounded-full ${statusPillClass(dep.status)}`}
      >
        {dep.status}
      </span>

      {/* Unrealized P&L — live-ticking via SSE, overlaid on the poll value;
          "—" when both are null (honesty), signed $ otherwise. */}
      <span
        data-testid="deployment-unrealized-pnl"
        className={`shrink-0 text-[12px] font-mono tabular-nums ${
          unrealizedPnl == null ? "text-text-3" : pnlTone(unrealizedPnl)
        }`}
        title="Unrealized P&L"
      >
        {fmtUsdSigned(unrealizedPnl)}
      </span>

      {/* Last decision, relative time (omitted when no decision recorded). */}
      {decisionAgo && (
        <span className="shrink-0 text-[12px] text-text-3 font-mono tabular-nums">
          {decisionAgo}
        </span>
      )}

      {eta && (
        <span
          data-testid={`deployment-eta-${dep.deployment_id}`}
          className="shrink-0 text-[12px] text-text-3 font-mono tabular-nums"
        >
          {eta}
        </span>
      )}

      {/* awm (b): runaway warning chip for a >24h live deployment. */}
      {runaway && (
        <span
          data-testid="deployment-runaway-chip"
          className="shrink-0 text-[11px] font-medium px-2 py-0.5 rounded-full bg-amber-500/15 text-amber-700 dark:text-amber-300"
        >
          ⚠ running &gt;24h
        </span>
      )}

      {/* awm (a): Stop hidden for optimizer-sourced or terminal deployments. */}
      {showStop && (
        <button
          type="button"
          data-testid={`deployment-stop-${dep.deployment_id}`}
          disabled={flatten.isPending}
          onClick={() => flatten.mutate()}
          className="shrink-0 text-[12px] text-text-3 hover:text-text disabled:opacity-50 px-2 py-0.5 rounded border border-border hover:border-border-strong transition-colors"
          aria-label={`Stop ${strategyName}`}
        >
          {flatten.isPending ? "Stopping…" : "Stop"}
        </button>
      )}
    </div>
  );
}

/**
 * S0 / O2+O3: the running (or paused) optimizer cycle, with pause/resume.
 * Pause/resume target the in-flight cycle via the mounted cycle-level
 * endpoints; controls disable when no cycle id is known yet.
 */
function OptimizerCycleRow({
  session,
  cycleId,
}: {
  session: SessionSummary;
  cycleId: string | null;
}) {
  const pause = usePauseCycle();
  const resume = useResumeCycle();
  const isPaused = session.state === "paused";
  const busy = pause.isPending || resume.isPending;

  return (
    <div
      data-testid="active-optimizer-cycle"
      className="flex items-center gap-2 px-3 py-2"
    >
      <span className="shrink-0 text-[11px] font-medium px-2 py-0.5 rounded-full bg-purple-500/15 text-purple-700 dark:text-purple-300">
        optimizer
      </span>
      <Link
        to={`/optimizer/run/${session.session_id}`}
        className="text-[13px] font-medium text-text hover:underline truncate min-w-0 flex-1"
      >
        {session.strategy_id}
      </Link>
      <span
        className={`shrink-0 text-[11px] font-medium px-2 py-0.5 rounded-full ${statusPillClass(session.state)}`}
      >
        {session.state}
      </span>
      <span className="shrink-0 text-[12px] text-text-3 font-mono tabular-nums">
        {session.cycles_completed} cycles
      </span>
      {isPaused ? (
        <button
          type="button"
          disabled={busy || !cycleId}
          onClick={() => cycleId && resume.mutate(cycleId)}
          className="shrink-0 text-[12px] text-text-3 hover:text-text disabled:opacity-50 px-2 py-0.5 rounded border border-border hover:border-border-strong transition-colors"
          aria-label="Resume optimizer cycle"
        >
          {resume.isPending ? "Resuming…" : "Resume"}
        </button>
      ) : (
        <button
          type="button"
          disabled={busy || !cycleId}
          onClick={() => cycleId && pause.mutate(cycleId)}
          className="shrink-0 text-[12px] text-text-3 hover:text-text disabled:opacity-50 px-2 py-0.5 rounded border border-border hover:border-border-strong transition-colors"
          aria-label="Pause optimizer cycle"
        >
          {pause.isPending ? "Pausing…" : "Pause"}
        </button>
      )}
    </div>
  );
}

export interface ActiveTasksStripProps {
  /** n0k/awm: live & paper deployment rows, sourced from the home route's 5s
   * `listDeployments({status:'running,paused'})` poll (CT5 §3/§9). Each row
   * renders the deployment's mode badge, status, last-decision time, and
   * honest unrealized P&L. Empty/undefined => no live group is rendered
   * (say-nothing-when-empty). */
  deployments?: LiveDeploymentSummary[];
}

export function ActiveTasksStrip({ deployments }: ActiveTasksStripProps = {}) {
  const { data } = useQuery({
    queryKey: evalKeys.runs({ status: "queued,running" }),
    queryFn: () => listRuns({ status: "queued,running" }),
    refetchInterval: 5_000,
  });

  // S0 / O2: the running optimizer cycle belongs in Active tasks too.
  const status = useOptimizerStatus();
  const session = status?.active_session ?? null;
  const showCycle =
    session !== null && (session.state === "running" || session.state === "paused");
  const activeCycleId = status?.active_cycle_id ?? null;

  // Filter to only inflight runs (defence-in-depth, server should already filter)
  const inflight = (data ?? []).filter(
    (r) => r.status === "queued" || r.status === "running",
  );

  // n0k: live/paper deployment rows. Empty => no live group at all.
  const liveDeployments = deployments ?? [];
  const showLive = liveDeployments.length > 0;

  // While the eval list is still loading AND there's no optimizer cycle AND no
  // live deployments to show, skip render entirely. A live cycle or a live
  // deployment should surface even before the eval list loads.
  if (data === undefined && !showCycle && !showLive) return null;

  const total = inflight.length + (showCycle ? 1 : 0) + liveDeployments.length;

  // CT2: never render a permanent empty card above the fold. Once the eval list
  // has loaded and there is nothing in flight (no optimizer cycle, no live
  // deployment), the panel renders nothing at all.
  if (data !== undefined && total === 0) return null;

  return (
    <div data-testid="active-tasks-strip" className="w-full">
      <div className="flex items-center justify-between px-5 pt-2.5 pb-1">
        <span className="caps">Active tasks</span>
        <span className="text-[11px] text-text-4 font-mono tabular-nums">
          {total} in flight
        </span>
      </div>

      {/* n0k/awm: live & paper deployment group — a distinct labeled section,
          rendered only when there is at least one deployment. */}
      {showLive && (
        <div data-testid="live-deployments-group">
          <div className="px-5 pt-1 pb-0.5">
            <span className="caps text-text-4">Live &amp; paper</span>
          </div>
          <div className="divide-y divide-border-soft/60 px-2 pb-1.5">
            {liveDeployments.map((dep) => (
              <DeploymentRow key={dep.deployment_id} dep={dep} />
            ))}
          </div>
        </div>
      )}

      <div className="divide-y divide-border-soft/60 px-2 pb-1.5">
        {showCycle && session !== null && (
          <OptimizerCycleRow session={session} cycleId={activeCycleId} />
        )}
        {inflight.map((run) => (
          <RunRow key={run.id} run={run} />
        ))}
      </div>
    </div>
  );
}
