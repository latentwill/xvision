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

      {/* Live badge */}
      <span className="rounded-full bg-green-500/15 px-2 py-0.5 text-xs font-medium text-green-600 dark:text-green-400">
        Live
      </span>

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
    queryFn: listAgentRuns,
    refetchInterval: 10_000,
  });

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
      ) : !data || data.length === 0 ? (
        <EmptyState />
      ) : (
        <div className="space-y-1">
          {data.map((summary) => (
            <RunRow key={summary.run_id} summary={summary} />
          ))}
        </div>
      )}
    </section>
  );
}
