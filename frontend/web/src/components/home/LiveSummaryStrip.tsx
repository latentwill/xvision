// frontend/web/src/components/home/LiveSummaryStrip.tsx
//
// Compact home-page summary strip for live trading (spec §2.10). Replaces the
// old per-run LiveStrategiesSection list — the full console lives at /live only.
//
// The strip gives an at-a-glance status and a single route into the live page:
//   - count of ACTIVE (running, not paused) live strategies
//   - count of PAUSED strategies (shown only when > 0)
//   - a "Go to Live Trading →" CTA to /live
//
// Aggregate daily PnL is intentionally NOT shown here. The home aggregate
// (`listAgentRuns()` → AgentRunSummary[]) carries only pre-rolled cost/token
// totals, not equity. Deriving daily PnL would require one equity-stream /
// chart fetch PER live run (see features/live/live-account.ts::dailyPnl), which
// is over-fetching for a home strip. The spec qualifies the metric as
// "(if available)" — it is not cheaply available here, so we omit it rather
// than fake a number. PnL lives in the live page at /live.

import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import { agentRunKeys, listAgentRuns } from "@/api/agent-runs";
import { deriveStripStatus, isLiveRun } from "@/features/live/strip-status";

/**
 * Live-trading summary strip — always visible, never returns null (matches the
 * old section's "communicate even when empty" contract). Polls every 10 s so
 * newly-deployed strategies appear without a manual refresh.
 */
export function LiveSummaryStrip() {
  const { data, isPending } = useQuery({
    queryKey: agentRunKeys.list(),
    queryFn: () => listAgentRuns(),
    refetchInterval: 10_000,
  });

  const runs = data ?? [];
  const live = runs.filter(isLiveRun);
  const activeCount = live.filter((r) => deriveStripStatus(r) === "ACTIVE").length;
  const pausedCount = live.filter((r) => deriveStripStatus(r) === "PAUSED").length;
  const hasLive = live.length > 0;

  return (
    <section
      aria-label="live-summary-strip"
      data-testid="live-summary-strip"
      className="flex flex-wrap items-center gap-x-3 gap-y-1 border-l-2 border-info/60 py-2 pl-4 text-sm"
    >
      {/* Label */}
      <span className="font-semibold tracking-tight text-foreground">
        Live trading
      </span>

      {/* Body: counts or empty state */}
      {isPending ? (
        <span
          data-testid="live-summary-loading"
          className="text-xs text-muted-foreground"
          aria-label="Loading live trading status"
        >
          Loading…
        </span>
      ) : hasLive ? (
        <span className="flex items-center gap-x-2 text-muted-foreground">
          <span className="text-foreground">
            <span className="font-semibold tabular-nums text-info">
              {activeCount}
            </span>{" "}
            active
          </span>
          {pausedCount > 0 && (
            <span className="text-foreground">
              <span className="font-semibold tabular-nums text-warn">
                {pausedCount}
              </span>{" "}
              paused
            </span>
          )}
        </span>
      ) : (
        <span className="text-muted-foreground">No live strategies running.</span>
      )}

      {/* CTA — always present. Routes to the live page when there's live
          activity to monitor, or to the strategies list to deploy one when
          there's nothing running yet. */}
      <Link
        to={hasLive ? "/live" : "/strategies"}
        className="ml-auto shrink-0 text-xs underline-offset-2 hover:underline"
      >
        {hasLive ? "Go to Live Trading →" : "Deploy a strategy →"}
      </Link>
    </section>
  );
}
