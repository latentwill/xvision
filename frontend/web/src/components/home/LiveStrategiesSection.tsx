// frontend/web/src/components/home/LiveStrategiesSection.tsx
//
// Operator's primary real-money visibility surface. Always renders — even
// when empty it communicates "no live money deployed." In dev/test it shows
// the mock MOCK_RUN_LIVE fixture; in prod it is an empty list until S2-W1
// ships the real GET /api/agent-runs endpoint.

import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import { agentRunKeys, listAgentRuns } from "@/api/agent-runs";
import type { AgentRunSummary } from "@/api/types-agent-runs";
import { isInflightRunStatus } from "@/lib/run-status";

// Cap the rows so the section can't grow unbounded (the endpoint returns every
// agent run ever recorded, not just live deployments — see header comment).
const MAX_ROWS = 8;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function relativeTime(isoString: string): string {
  const diff = Date.now() - new Date(isoString).getTime();
  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function LoadingSkeleton() {
  return (
    <div
      data-testid="live-strategies-loading"
      className="animate-pulse space-y-2"
      aria-label="Loading live strategies"
    >
      <div className="h-8 w-48 rounded bg-muted" />
      <div className="h-10 w-full rounded bg-muted" />
    </div>
  );
}

function EmptyState() {
  return (
    <p className="text-sm text-muted-foreground">
      No active live deployments.{" "}
      <span className="text-foreground">Live strategies trade real capital.</span>{" "}
      <Link
        to="/settings/brokers"
        className="underline underline-offset-2 hover:text-foreground"
      >
        Configure brokers
      </Link>
    </p>
  );
}

function StatusBadge({ status }: { status: string }) {
  // Reflect the run's ACTUAL status. The previous hardcoded green "Live" badge
  // mislabelled every row (including backtests) as live real money — dangerous
  // on a real-money surface. Distinguishing live-money from backtest needs a
  // backend signal the AgentRunSummary doesn't carry yet (see header comment).
  const running = status === "running";
  return (
    <span
      className={
        running
          ? "rounded-full bg-green-500/15 px-2 py-0.5 text-xs font-medium text-green-600 dark:text-green-400"
          : "rounded-full bg-amber-500/15 px-2 py-0.5 text-xs font-medium text-amber-600 dark:text-amber-400"
      }
    >
      {running ? "Running" : "Queued"}
    </span>
  );
}

function RunRow({ summary }: { summary: AgentRunSummary }) {
  const shortId = summary.run_id.slice(0, 8);
  const startedRelative = relativeTime(summary.started_at);

  return (
    <Link
      to={`/live/${summary.run_id}`}
      className="flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors hover:bg-muted/50"
      aria-label={shortId}
    >
      {/* Run ID */}
      <span className="font-mono text-xs text-muted-foreground">{shortId}</span>

      {/* Actual status (not a hardcoded "Live") */}
      <StatusBadge status={summary.status} />

      {/* Started */}
      <span className="ml-auto text-xs text-muted-foreground">{startedRelative}</span>
    </Link>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

/**
 * Live strategies section — always visible, never returns null.
 *
 * Polls every 10 s so the operator sees newly-deployed strategies without
 * a manual refresh. Renders a loading skeleton while the first fetch is
 * in-flight, an empty-state CTA when there are no runs, and a compact row
 * per live run otherwise.
 */
export function LiveStrategiesSection() {
  const { data, isPending } = useQuery({
    queryKey: agentRunKeys.list(),
    queryFn: () => listAgentRuns(),
    refetchInterval: 10_000,
  });

  // Only runs in flight (queued/running) are "live strategies running now" —
  // completed/historical agent runs (backtests, finished evals) are not. This
  // also collapses the unbounded list to the handful actually executing.
  const inflight = (data ?? []).filter((r) => isInflightRunStatus(r.status));
  const shown = inflight.slice(0, MAX_ROWS);
  const overflow = inflight.length - shown.length;

  return (
    <section
      aria-label="live-strategies-section"
      data-testid="live-strategies-section"
      className="border-l-2 border-green-500 pl-4"
    >
      {/* Header */}
      <div className="mb-3 flex items-baseline gap-2">
        <h2 className="text-sm font-semibold tracking-tight">Live strategies</h2>
        <span className="text-xs text-muted-foreground">· Real money</span>
      </div>

      {/* Body */}
      {isPending ? (
        <LoadingSkeleton />
      ) : inflight.length === 0 ? (
        <EmptyState />
      ) : (
        <div className="space-y-1">
          {shown.map((summary) => (
            <RunRow key={summary.run_id} summary={summary} />
          ))}
          {overflow > 0 && (
            <Link
              to="/live"
              className="block px-3 py-1.5 text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
            >
              View all {inflight.length} running →
            </Link>
          )}
        </div>
      )}
    </section>
  );
}
