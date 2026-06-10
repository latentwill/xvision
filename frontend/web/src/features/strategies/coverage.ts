// frontend/web/src/features/strategies/coverage.ts
//
// Per-strategy eval-coverage selector (beads xvision-eb5). Answers, for every
// strategy: "has this ever had a completed eval?" and "did a human make it,
// or the optimizer?" — correctly across the three id shapes an eval run can
// carry:
//
//   - `run.strategy.id`      enriched payloads (run-detail endpoint)
//   - `run.agent_id`         strategy workspace ULID (dashboard-launched runs)
//   - `run.agent_id`         strategy bundle hash (older CLI-launched runs;
//                            matched against `strategy.bundle_hash`)
//
// Because list pages are truncated (the home page sees one runs page), the
// selector also trusts the server-computed `evaluated` /
// `last_eval_completed_at` fields on `StrategyListItem` when present — those
// are derived from the full `eval_runs` table, not a page of it.
//
// Consumers: StrategyOutcomesSummary today; the home-redesign surfaces next.

import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";

/** Where a strategy came from. Mirrors the server's `StrategyOrigin`:
 * `optimizer` when the strategy's bundle hash appears in the autooptimizer
 * lineage (`lineage_nodes`) — such strategies are evaluated inside optimizer
 * cycles and must not be nagged about missing eval runs. */
export type StrategyOrigin = "user" | "optimizer";

/** Eval coverage for one strategy. */
export interface StrategyEvalCoverage {
  strategy: StrategyListItem;
  /** True when at least one completed eval references this strategy —
   * combining the server-side flag (full `eval_runs` table) with the
   * client-side join over the supplied runs page. */
  evaluated: boolean;
  origin: StrategyOrigin;
  /** Most recent completed run *within the supplied runs page*; `null` when
   * the strategy's evals are only known server-side (or it has none).
   * Carries the last-eval metrics (return / sharpe / drawdown) when set. */
  latestRun: RunSummary | null;
  /** Completed runs for this strategy among the supplied runs. */
  completedRunCount: number;
}

/** Aggregate counts for summary lines. The two segment counts partition the
 * NOT-evaluated strategies by origin. */
export interface CoverageCounts {
  total: number;
  evaluated: number;
  /** User-created strategies with no completed eval anywhere. */
  userAwaitingFirstEval: number;
  /** Optimizer-origin strategies with no *direct* eval run — they were
   * evaluated inside optimizer cycles (lineage), so they are informational,
   * not actionable. */
  optimizerLineage: number;
}

/** The id an eval run is keyed by on list payloads: enriched strategy id
 * when present, else the raw `agent_id` (ULID or bundle hash). */
function runStrategyKey(run: RunSummary): string | null {
  return run.strategy?.id ?? (run.agent_id || null);
}

/**
 * Join strategies to eval runs and compute per-strategy coverage.
 * Order of the result mirrors the input `strategies` order.
 */
export function strategyEvalCoverage(
  strategies: StrategyListItem[],
  runs: RunSummary[],
): StrategyEvalCoverage[] {
  // Group completed runs by their strategy key (ULID or bundle hash).
  const byKey = new Map<string, { latest: RunSummary; count: number }>();
  for (const run of runs) {
    if (run.status !== "completed") continue;
    const key = runStrategyKey(run);
    if (!key) continue;
    const entry = byKey.get(key);
    if (!entry) {
      byKey.set(key, { latest: run, count: 1 });
    } else {
      entry.count += 1;
      const incoming = run.completed_at ?? "";
      const existing = entry.latest.completed_at ?? "";
      if (incoming.localeCompare(existing) > 0) entry.latest = run;
    }
  }

  return strategies.map((strategy) => {
    const byUlid = byKey.get(strategy.agent_id);
    const byHash = strategy.bundle_hash
      ? byKey.get(strategy.bundle_hash)
      : undefined;

    let latest: RunSummary | null = null;
    let count = 0;
    for (const entry of [byUlid, byHash]) {
      if (!entry) continue;
      count += entry.count;
      if (
        latest === null ||
        (entry.latest.completed_at ?? "").localeCompare(
          latest.completed_at ?? "",
        ) > 0
      ) {
        latest = entry.latest;
      }
    }

    return {
      strategy,
      evaluated: strategy.evaluated === true || count > 0,
      origin: strategy.origin === "optimizer" ? "optimizer" : "user",
      latestRun: latest,
      completedRunCount: count,
    };
  });
}

/** Roll a coverage list up into the counts the summary line renders. */
export function coverageCounts(
  items: StrategyEvalCoverage[],
): CoverageCounts {
  let evaluated = 0;
  let userAwaitingFirstEval = 0;
  let optimizerLineage = 0;
  for (const item of items) {
    if (item.evaluated) {
      evaluated += 1;
    } else if (item.origin === "optimizer") {
      optimizerLineage += 1;
    } else {
      userAwaitingFirstEval += 1;
    }
  }
  return {
    total: items.length,
    evaluated,
    userAwaitingFirstEval,
    optimizerLineage,
  };
}
