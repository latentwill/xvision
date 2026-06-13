// frontend/web/src/components/home/ActiveTasksStrip.tsx
//
// nsk BOUNDARY NOTE (Control Tower s78): this is the SINGLE home for per-run
// live/paper ROWS once the CT5 LiveDeploymentSummary contract lands (bead n0k)
// — each deployment's running P&L / last decision will render as rows here.
// Today it shows only the eval QUEUE (in-flight eval runs + the optimizer
// cycle); no live rows exist pre-CT5. The aggregate live COUNT ("N live")
// lives in LiveSummaryStrip — that strip answers "how many are live?", this
// one answers "what is each task doing now?". Do not duplicate the aggregate
// live count here.
//
// Shows in-flight eval runs (queued | running) with elapsed time, stuck warning, cancel,
// plus the running optimizer cycle (if any) with pause/resume controls (S0 / O2+O3).

import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { cancelRun, evalKeys, flattenRun, listRuns } from "@/api/eval";
import { liveDeploymentKeys, listLiveDeployments } from "@/api/live-deployments";
import type { RunSummary } from "@/api/types.gen";
import type { LiveDeploymentSummary } from "@/api/types.gen/LiveDeploymentSummary";
import type { VenueLabel } from "@/api/safety";
import { VenueBadge } from "@/components/primitives/VenueBadge";
import {
  runningPnl,
  formatUsd,
  formatEta,
  type RiskTone,
} from "@/features/live/deployment-risk";
import {
  useOptimizerStatus,
  usePauseCycle,
  useResumeCycle,
  type SessionSummary,
} from "@/features/autooptimizer/api";

const TWO_HOURS_MS = 2 * 60 * 60 * 1000;
const RUNAWAY_MS = 24 * 60 * 60 * 1000;

function toneClass(tone: RiskTone): string {
  switch (tone) {
    case "gold":    return "text-gold";
    case "warn":    return "text-warn";
    case "danger":  return "text-danger";
    case "neutral": return "";
  }
}

/** Format a last_decision_at timestamp as "decided N ago" (e.g. "decided 3m ago"). */
function formatDecisionAgo(lastDecisionAt: string | null): string {
  if (!lastDecisionAt) return "no decisions yet";
  const ms = Date.now() - new Date(lastDecisionAt).getTime();
  if (ms < 0) return "decided just now";
  const totalSecs = Math.floor(ms / 1000);
  const hours = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  const secs = totalSecs % 60;
  if (hours > 0) {
    return `decided ${hours}h ${mins}m ago`;
  }
  if (mins > 0) {
    return `decided ${mins}m ago`;
  }
  return `decided ${secs}s ago`;
}

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

function statusPillClass(status: string): string {
  switch (status) {
    case "running":
      return "bg-blue-500/15 text-blue-700 dark:text-blue-300";
    case "queued":
      return "bg-yellow-500/15 text-yellow-700 dark:text-yellow-300";
    default:
      return "bg-surface-elev text-text-3";
  }
}

/**
 * awm (S3): one live (paper/testnet) deployment row in the Active tasks strip.
 * Shows: strategy name link → VenueBadge → last-decision relative time → running P&L
 *        → ETA (when stop_at is set) → runaway >24h warning (if applicable)
 *        → Stop button (if non-terminal).
 * HONESTY: VenueBadge always present (paper/testnet — never live money).
 *          P&L is simulated; null P&L → "—".
 */
function LiveDeploymentRow({ dep }: { dep: LiveDeploymentSummary }) {
  const queryClient = useQueryClient();

  // awm: Stop = flatten the underlying eval run. deployment_id IS the eval run id.
  const flatten = useMutation({
    mutationFn: () => flattenRun(dep.deployment_id),
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: liveDeploymentKeys.all });
    },
  });

  const name = dep.strategy_name ?? "—";
  const decisionText = formatDecisionAgo(dep.last_decision_at);
  const p = runningPnl(dep);
  const pnlText = p.value !== null ? `${p.glyph} ${formatUsd(p.value)}` : "—";
  const eta = formatEta(dep.stop_at);

  // Terminal statuses: Stop not applicable.
  const isTerminal =
    dep.status === "completed" ||
    dep.status === "failed" ||
    dep.status === "cancelled";

  // awm: runaway if running for more than 24h.
  const isRunaway =
    dep.status === "running" &&
    Date.now() - new Date(dep.started_at).getTime() > RUNAWAY_MS;

  return (
    <div
      data-testid={`live-deployment-row-${dep.deployment_id}`}
      className="flex items-center gap-3 py-2 px-3 rounded-md hover:bg-surface-hover transition-colors min-w-0"
    >
      {/* Strategy name → eval run detail (deployment_id maps to eval run id) */}
      <Link
        to={`/eval-runs/${dep.deployment_id}`}
        className="text-[13px] font-medium text-text hover:underline truncate min-w-0 flex-1"
      >
        {name}
      </Link>

      {/* Venue badge — always paper or testnet (honesty mandate) */}
      <VenueBadge label={dep.venue_label as VenueLabel} />

      {/* Last decision time */}
      <span className="shrink-0 text-[12px] text-text-3">
        {decisionText}
      </span>

      {/* Running P&L — glyph + sign + value, colored by tone. Never color alone. */}
      <span
        key={String(p.value)}
        className={`xvn-num-pop shrink-0 text-[12px] font-mono font-semibold tabular-nums ${toneClass(p.tone)}`}
      >
        {pnlText}
      </span>

      {/* awm ETA — only rendered when stop_at is a real wall-clock deadline */}
      {eta !== null && (
        <span
          data-testid={`live-eta-${dep.deployment_id}`}
          className="shrink-0 text-[12px] text-text-3"
        >
          {eta}
        </span>
      )}

      {/* awm: runaway >24h warning pill — mirrors RunRow's ">2h stuck" idiom */}
      {isRunaway && (
        <span
          data-testid={`live-runaway-${dep.deployment_id}`}
          className="shrink-0 text-[11px] font-medium px-2 py-0.5 rounded-full bg-amber-500/15 text-amber-700 dark:text-amber-300"
        >
          ⚠ running &gt;24h
        </span>
      )}

      {/* awm: Stop (flatten) — direct action, no confirm dialog (no-popups rule).
          Hidden on terminal statuses. Mirrors RunRow's Cancel button exactly. */}
      {!isTerminal && (
        <button
          type="button"
          data-testid={`live-stop-${dep.deployment_id}`}
          disabled={flatten.isPending}
          onClick={() => flatten.mutate()}
          className="shrink-0 text-[12px] text-text-3 hover:text-text disabled:opacity-50 px-2 py-0.5 rounded border border-border hover:border-border-strong transition-colors"
          aria-label={`Stop ${name}`}
        >
          {flatten.isPending ? "Stopping…" : "Stop"}
        </button>
      )}
    </div>
  );
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

export function ActiveTasksStrip() {
  const { data } = useQuery({
    queryKey: evalKeys.runs({ status: "queued,running" }),
    queryFn: () => listRuns({ status: "queued,running" }),
    refetchInterval: 5_000,
  });

  // n0k (CT5): running live/paper deployments — each renders as a LiveDeploymentRow.
  const liveDeployments = useQuery({
    queryKey: liveDeploymentKeys.list({ status: "running" }),
    queryFn: () => listLiveDeployments({ status: "running" }),
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

  const liveRows = liveDeployments.data ?? [];

  // While the eval list is still loading AND there's no optimizer cycle or live
  // deployments to show, skip render entirely. Live rows should surface even
  // before evals load.
  if (data === undefined && !showCycle && liveRows.length === 0) return null;

  const total = inflight.length + liveRows.length + (showCycle ? 1 : 0);

  // CT2: never render a permanent empty card above the fold. Once the eval list
  // has loaded and there is nothing in flight (and no optimizer cycle, no live
  // deployments), the panel renders nothing at all.
  if (data !== undefined && total === 0) return null;

  return (
    <div data-testid="active-tasks-strip" className="w-full">
      <div className="flex items-center justify-between px-5 pt-2.5 pb-1">
        <span className="caps">Active tasks</span>
        <span className="text-[11px] text-text-4 font-mono tabular-nums">
          {total} in flight
        </span>
      </div>

      <div className="divide-y divide-border-soft/60 px-2 pb-1.5">
        {showCycle && session !== null && (
          <OptimizerCycleRow session={session} cycleId={activeCycleId} />
        )}
        {inflight.map((run) => (
          <RunRow key={run.id} run={run} />
        ))}
        {liveRows.map((dep) => (
          <LiveDeploymentRow key={dep.deployment_id} dep={dep} />
        ))}
      </div>
    </div>
  );
}
