// frontend/web/src/components/home/LiveSummaryStrip.tsx
//
// nsk BOUNDARY NOTE (Control Tower s78): this is the AGGREGATE live COUNT strip
// — the single at-a-glance "how many are live?" surface ("N live · M paper").
// Per-run live ROWS (each deployment's running P&L / last decision) do NOT
// belong here; they live in ActiveTasksStrip once the CT5 LiveDeploymentSummary
// contract lands (bead n0k). Keep these two surfaces distinct: this answers
// "how many are live?", ActiveTasksStrip answers "what is each one doing now?".
// Do not re-introduce a per-run live list here (that was the deleted
// LiveStrategiesSection).
//
// Compact home-page summary strip for live trading (spec §2.10). Replaces the
// old per-run LiveStrategiesSection list — the full console lives at /live only.
//
// The strip gives an at-a-glance HONEST status and a single route into the
// live page (xvision-9pi — "live" means live money):
//   - count of ACTIVE live-money strategies (parent eval run mode=live,
//     non-terminal; the backend `is_live_money` discriminator)
//   - count of PAUSED live-money strategies (shown only when > 0)
//   - count of non-live running rows (paper/sim/backtest; only when > 0)
//   - count of stale orphans (agent runs stuck in `running` whose parent
//     eval run is terminal; only when > 0 — rendered muted, never as live)
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
import {
  classifyRunLiveness,
  deriveStripStatus,
  isLiveRun,
} from "@/features/live/strip-status";

/**
 * Live-trading summary strip — always visible, never returns null (matches the
 * old section's "communicate even when empty" contract). Polls every 10 s so
 * newly-deployed strategies appear without a manual refresh.
 */
export function LiveSummaryStrip() {
  // Scope the query to the non-terminal population: live/non-live/stale are
  // all derived from `queued`/`running` agent runs, and the default
  // newest-20-of-any-status window would undercount once terminal runs
  // crowd the head of the ledger. Params live in the query key so this
  // cache entry never collides with the live page's unfiltered list.
  const listParams = { status: "running,queued", limit: 100 } as const;
  const { data, isPending } = useQuery({
    queryKey: agentRunKeys.list(listParams),
    queryFn: () => listAgentRuns(listParams),
    refetchInterval: 10_000,
  });

  const runs = data ?? [];
  // Live = live money ONLY (backend is_live_money + non-terminal parent).
  const live = runs.filter(isLiveRun);
  const activeCount = live.filter((r) => deriveStripStatus(r) === "ACTIVE").length;
  const pausedCount = live.filter((r) => deriveStripStatus(r) === "PAUSED").length;
  // Honest companions: running-but-not-live-money, and stale orphans.
  const nonLiveCount = runs.filter((r) => classifyRunLiveness(r) === "paper").length;
  const staleCount = runs.filter((r) => classifyRunLiveness(r) === "stale").length;
  const hasLive = live.length > 0;
  const hasAny = hasLive || nonLiveCount > 0 || staleCount > 0;

  return (
    <section
      aria-label="live-summary-strip"
      data-testid="live-summary-strip"
      className="flex flex-wrap items-center gap-x-3 gap-y-1 px-5 py-2.5 text-[13px]"
    >
      {/* Label */}
      <span className="text-[12px] font-medium text-text">
        Live trading
      </span>

      {/* Body: counts or empty state */}
      {isPending ? (
        <span
          data-testid="live-summary-loading"
          className="text-[12px] text-text-4"
          aria-label="Loading live trading status"
        >
          Loading…
        </span>
      ) : hasAny ? (
        <span className="flex items-center gap-x-2 text-[12px] text-text-3">
          {hasLive && (
            <span data-testid="live-count" className="text-text-2">
              <span className="font-mono font-semibold tabular-nums text-info">
                {activeCount}
              </span>{" "}
              live
              <span className="text-text-4 font-normal"> · simulated</span>
            </span>
          )}
          {pausedCount > 0 && (
            <span data-testid="paused-count" className="text-text-2">
              <span className="font-mono font-semibold tabular-nums text-warn">
                {pausedCount}
              </span>{" "}
              paused
            </span>
          )}
          {nonLiveCount > 0 && (
            <span data-testid="non-live-count" className="text-text-2">
              <span className="font-mono font-semibold tabular-nums">{nonLiveCount}</span>{" "}
              non-live
            </span>
          )}
          {staleCount > 0 && (
            <span
              data-testid="stale-count"
              className="text-text-3"
              title="Agent run stuck in 'running' after parent eval completed"
            >
              <span className="font-mono font-semibold tabular-nums">{staleCount}</span>{" "}
              stale
            </span>
          )}
        </span>
      ) : (
        <span className="text-[12px] text-text-3">No live strategies running.</span>
      )}

      {/* CTA — always present. Routes to the live page when there's activity
          to monitor (live, non-live, or stale rows to clean up), or to the
          strategies list to deploy one when there's nothing running yet. */}
      <Link
        to={hasAny ? "/live" : "/strategies"}
        className="ml-auto shrink-0 text-[12px] text-text-3 hover:text-text underline-offset-2 hover:underline"
      >
        {hasLive ? "Go to Live Trading →" : hasAny ? "View runs →" : "Deploy a strategy →"}
      </Link>
    </section>
  );
}
