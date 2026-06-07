// Shows in-flight eval runs (queued | running) with elapsed time, stuck warning, cancel.

import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { cancelRun, evalKeys, listRuns } from "@/api/eval";
import type { RunSummary } from "@/api/types.gen";

const TWO_HOURS_MS = 2 * 60 * 60 * 1000;

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
      return "bg-muted text-muted-foreground";
  }
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
    <div className="flex items-center gap-3 py-2 px-3 rounded-md hover:bg-muted/40 transition-colors min-w-0">
      {/* Strategy name links to run detail */}
      <Link
        to={`/eval-runs/${run.id}`}
        className="text-[13px] font-medium text-foreground hover:underline truncate min-w-0 flex-1"
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
      <span className="shrink-0 text-[12px] text-muted-foreground font-mono tabular-nums">
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
        className="shrink-0 text-[12px] text-muted-foreground hover:text-foreground disabled:opacity-50 px-2 py-0.5 rounded border border-border hover:border-muted-foreground/40 transition-colors"
        aria-label={`Cancel ${strategyName}`}
      >
        {cancel.isPending ? "Cancelling…" : "Cancel"}
      </button>
    </div>
  );
}

export function ActiveTasksStrip() {
  const { data } = useQuery({
    queryKey: evalKeys.runs({ status: "queued,running" }),
    queryFn: () => listRuns({ status: "queued,running" }),
    refetchInterval: 5_000,
  });

  // While loading (data === undefined), skip render entirely
  if (data === undefined) return null;

  // Filter to only inflight runs (defence-in-depth, server should already filter)
  const inflight = data.filter(
    (r) => r.status === "queued" || r.status === "running",
  );

  return (
    <div
      data-testid="active-tasks-strip"
      className="w-full rounded-md border border-border bg-card"
    >
      <div className="flex items-center justify-between px-3 py-2 border-b border-border">
        <span className="text-[12px] font-semibold text-muted-foreground uppercase tracking-wide">
          Active tasks
        </span>
        {inflight.length > 0 && (
          <span className="text-[11px] text-muted-foreground">
            {inflight.length} in flight
          </span>
        )}
      </div>

      {inflight.length === 0 ? (
        <p className="px-3 py-3 text-[13px] text-muted-foreground">
          No active tasks
        </p>
      ) : (
        <div className="divide-y divide-border/50">
          {inflight.map((run) => (
            <RunRow key={run.id} run={run} />
          ))}
        </div>
      )}
    </div>
  );
}
