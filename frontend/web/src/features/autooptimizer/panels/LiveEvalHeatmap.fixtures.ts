// Fixtures for LiveEvalHeatmap — shared by the unit test and the dev preview
// harness. These mirror the real CycleNodeDetail / RegimeResult wire shapes.

import type { CycleNodeDetail, RegimeMetrics, RegimeResult } from "../api";

function metrics(sharpe: number): RegimeMetrics {
  return {
    total_return_pct: sharpe * 4,
    sharpe,
    max_drawdown_pct: -6.2,
    win_rate: 0.55,
    n_trades: 42,
  };
}

function regime(
  label: string,
  side: RegimeResult["side"],
  deltaSharpe: number,
): RegimeResult {
  return {
    regime_label: label,
    side,
    delta_sharpe: deltaSharpe,
    verdict: "Pass",
    metrics_day: metrics(1.6 + deltaSharpe),
    metrics_untouched: metrics(1.4 + deltaSharpe),
  };
}

const REGIMES: Array<[string, RegimeResult["side"]]> = [
  ["bull-q1", "bull"],
  ["flash-24", "bear_or_shock"],
  ["chop-q2", "chop"],
  ["bull-q3", "bull"],
  ["bear-q4", "bear_or_shock"],
];

function node(hash: string, completedRegimes: number): CycleNodeDetail {
  return {
    bundle_hash: hash,
    parent_hash: "0xparent",
    gate_verdict: "Pass",
    status: "active",
    cycle_id: "cyc-fixture",
    created_at: "2026-06-08T00:00:00Z",
    diversity_score: 0.5,
    regime_results: REGIMES.slice(0, completedRegimes).map(([label, side], i) =>
      regime(label, side, 0.3 - i * 0.15),
    ),
  };
}

/** Running cycle: a mix of done + in-flight cells (the showcase state). */
export const HEATMAP_RUNNING: CycleNodeDetail[] = [
  node("0xaaaa1111", 5), // fully done
  node("0xbbbb2222", 3), // partial → remaining cells "testing"
  node("0xcccc3333", 1), // mostly testing
  node("0xdddd4444", 0), // all testing
];

/** Idle cycle with partial results: remaining cells render "queued". */
export const HEATMAP_IDLE: CycleNodeDetail[] = [
  node("0xaaaa1111", 5),
  node("0xbbbb2222", 4),
  node("0xcccc3333", 2),
];

/** Empty — no regime data yet. */
export const HEATMAP_EMPTY: CycleNodeDetail[] = [];
