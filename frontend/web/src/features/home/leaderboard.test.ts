import { describe, expect, it } from "vitest";

import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import type { StrategyEvalCoverage } from "@/features/strategies/coverage";
import { strategyLeaderboard } from "./leaderboard";

function strategy(over: Partial<StrategyListItem>): StrategyListItem {
  return {
    agent_id: "strat-1",
    display_name: "Strategy 1",
    template: "trend_follower",
    decision_cadence_minutes: 60,
    ...over,
  };
}

function run(over: Partial<RunSummary>): RunSummary {
  return {
    id: "run-1",
    agent_id: "strat-1",
    scenario_id: "scn-1",
    strategy: null,
    scenario: null,
    mode: "backtest",
    status: "completed",
    started_at: "2026-06-10T09:00:00Z",
    completed_at: "2026-06-10T10:00:00Z",
    sharpe: 1.0,
    max_drawdown_pct: 0.5,
    total_return_pct: 0.1,
    error: null,
    actual_input_tokens: null,
    actual_output_tokens: null,
    inference_cost_quote_total: null,
    net_return_pct: null,
    filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: null,
    paused: false,
    paused_at: null,
    flatten_requested: false,
    unrealized_pnl_usd: null,
    skipped_dispatches: 0,
    delayed_decisions: 0,
    forced_cancels: 0,
    ...over,
  };
}

function coverage(over: Partial<StrategyEvalCoverage>): StrategyEvalCoverage {
  return {
    strategy: strategy({}),
    evaluated: true,
    origin: "user",
    latestRun: run({}),
    completedRunCount: 3,
    ...over,
  };
}

describe("strategyLeaderboard", () => {
  it("ranks by latest-eval return desc, Sharpe tie-break", () => {
    const items = [
      coverage({
        strategy: strategy({ agent_id: "low" }),
        latestRun: run({ total_return_pct: -0.5 }),
      }),
      coverage({
        strategy: strategy({ agent_id: "high" }),
        latestRun: run({ total_return_pct: 0.5 }),
      }),
      coverage({
        strategy: strategy({ agent_id: "tie-low-sharpe" }),
        latestRun: run({ total_return_pct: 0.2, sharpe: 0.1 }),
      }),
      coverage({
        strategy: strategy({ agent_id: "tie-high-sharpe" }),
        latestRun: run({ total_return_pct: 0.2, sharpe: 2.0 }),
      }),
    ];
    expect(strategyLeaderboard(items).map((e) => e.strategy.agent_id)).toEqual([
      "high",
      "tie-high-sharpe",
      "tie-low-sharpe",
      "low",
    ]);
  });

  it("excludes strategies without a visible latest run", () => {
    const items = [
      coverage({ latestRun: null, completedRunCount: 0 }),
      coverage({ strategy: strategy({ agent_id: "ok" }) }),
    ];
    expect(strategyLeaderboard(items)).toHaveLength(1);
    expect(strategyLeaderboard(items)[0].strategy.agent_id).toBe("ok");
  });

  it("flags low sample sizes and carries origin", () => {
    const items = [
      coverage({
        strategy: strategy({ agent_id: "thin" }),
        completedRunCount: 1,
        origin: "optimizer",
      }),
      coverage({
        strategy: strategy({ agent_id: "thick" }),
        completedRunCount: 5,
        latestRun: run({ total_return_pct: -1 }),
      }),
    ];
    const [thin, thick] = strategyLeaderboard(items);
    expect(thin.lowSample).toBe(true);
    expect(thin.origin).toBe("optimizer");
    expect(thick.lowSample).toBe(false);
  });

  it("ranks null returns below real losses and caps at max", () => {
    const items = [
      coverage({
        strategy: strategy({ agent_id: "null-return" }),
        latestRun: run({ total_return_pct: null, sharpe: null }),
      }),
      coverage({
        strategy: strategy({ agent_id: "loss" }),
        latestRun: run({ total_return_pct: -2 }),
      }),
    ];
    expect(strategyLeaderboard(items, 1).map((e) => e.strategy.agent_id)).toEqual([
      "loss",
    ]);
  });

  it("prefers the server-side last_eval_completed_at for freshness", () => {
    const items = [
      coverage({
        strategy: strategy({ last_eval_completed_at: "2026-06-10T11:00:00Z" }),
        latestRun: run({ completed_at: "2026-06-10T10:00:00Z" }),
      }),
    ];
    expect(strategyLeaderboard(items)[0].lastEvalAt).toBe(
      "2026-06-10T11:00:00Z",
    );
  });
});
