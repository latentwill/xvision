// frontend/web/src/features/home/leaderboard.ts
//
// Ranking selector for the home strategy leaderboard. Consumes the
// eval-coverage join (features/strategies/coverage.ts) and orders the
// strategies that have at least one completed eval visible in the supplied
// runs page. Sample sizes ride along so the UI can flag low-n results
// instead of rendering thin data as a verdict.

import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import type {
  StrategyEvalCoverage,
  StrategyOrigin,
} from "@/features/strategies/coverage";

/** Below this many completed evals (within the visible runs page) a row gets
 * an explicit low-sample chip — same threshold the outcomes summary used to
 * gate verdict coloring. */
export const LOW_SAMPLE_THRESHOLD = 3;

export interface LeaderboardEntry {
  strategy: StrategyListItem;
  /** Latest completed eval run (carries return / Sharpe / max DD). */
  run: RunSummary;
  /** Completed evals for this strategy within the supplied runs page. */
  sampleSize: number;
  lowSample: boolean;
  origin: StrategyOrigin;
  /** RFC3339 stamp of the most recent completed eval (server-side field when
   * present, else the latest visible run). */
  lastEvalAt: string | null;
}

function metric(v: number | null): number {
  return v !== null && Number.isFinite(v) ? v : Number.NEGATIVE_INFINITY;
}

/**
 * Rank evaluated strategies by latest-eval return (desc), Sharpe as the
 * tie-break. Only coverage items with a visible latest run qualify — rows
 * without metrics can't be ranked honestly.
 */
export function strategyLeaderboard(
  items: StrategyEvalCoverage[],
  max = 6,
): LeaderboardEntry[] {
  const entries: LeaderboardEntry[] = [];
  for (const item of items) {
    if (item.latestRun === null) continue;
    entries.push({
      strategy: item.strategy,
      run: item.latestRun,
      sampleSize: item.completedRunCount,
      lowSample: item.completedRunCount < LOW_SAMPLE_THRESHOLD,
      origin: item.origin,
      lastEvalAt:
        item.strategy.last_eval_completed_at ??
        item.latestRun.completed_at ??
        null,
    });
  }
  entries.sort((a, b) => {
    const byReturn =
      metric(b.run.total_return_pct) - metric(a.run.total_return_pct);
    if (byReturn !== 0) return byReturn;
    return metric(b.run.sharpe) - metric(a.run.sharpe);
  });
  return entries.slice(0, max);
}
