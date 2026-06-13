// frontend/web/src/features/home/capital-risk.test.ts
//
// Spec for the capital-risk aggregate selector (bead 8s4, CT5 §9.3). The
// HONESTY MANDATE (§8.1/§8.9 — "non-negotiable for live money") is the core
// invariant under test: every aggregate is taken ONLY over non-null source
// values, a fully-null field aggregates to `null` (NEVER a fabricated 0), and
// the data floor (no deployments OR all-null fields) reports hasData=false so
// the strip can render an honest "insufficient data" state rather than a calm
// green zero.

import { describe, expect, it } from "vitest";

import { aggregateCapitalRisk, bufferTone } from "./capital-risk";
import type { LiveDeploymentSummary } from "@/api/types.gen";

// Minimal honest deployment; tests override the capital-risk fields. Defaults
// mirror the CT5 wire shape (all capital fields null until a fill lands).
function dep(over: Partial<LiveDeploymentSummary>): LiveDeploymentSummary {
  return {
    deployment_id: "dep-1",
    strategy_id: "strat-1",
    strategy_name: "S1",
    mode: "paper",
    status: "running",
    started_at: "2026-06-13T00:00:00Z",
    last_decision_at: null,
    venue: "alpaca-paper",
    venue_connected: true,
    deployed_capital_usd: null,
    realized_pnl_usd: null,
    unrealized_pnl_usd: null,
    drawdown_pct: null,
    daily_loss_limit_remaining_usd: null,
    daily_loss_budget_usd: null,
    stop_at: null,
    risk_veto_count_since_last_visit: null,
    paused: false,
    flatten_requested: false,
    global_safety_paused: false,
    source: "human",
    unavailable_reason: null,
    ...over,
  };
}

describe("aggregateCapitalRisk", () => {
  it("reports the data floor (hasData=false) for an empty deployment list", () => {
    const agg = aggregateCapitalRisk([]);
    expect(agg.hasData).toBe(false);
    expect(agg.liveCount).toBe(0);
    expect(agg.deployedCapitalUsd).toBeNull();
    expect(agg.worstDrawdownPct).toBeNull();
    expect(agg.tightestDailyLossBufferUsd).toBeNull();
    expect(agg.tightestDailyLossBudgetUsd).toBeNull();
  });

  it("reports the data floor when deployments exist but every capital field is null", () => {
    // Two live rows, but no fills yet → every capital-risk field null.
    const agg = aggregateCapitalRisk([dep({}), dep({ deployment_id: "dep-2" })]);
    expect(agg.liveCount).toBe(2);
    expect(agg.hasData).toBe(false);
    expect(agg.deployedCapitalUsd).toBeNull();
    expect(agg.worstDrawdownPct).toBeNull();
    expect(agg.tightestDailyLossBufferUsd).toBeNull();
    expect(agg.tightestDailyLossBudgetUsd).toBeNull();
  });

  it("SUMS deployed capital only over non-null values", () => {
    const agg = aggregateCapitalRisk([
      dep({ deployment_id: "a", deployed_capital_usd: 1000 }),
      dep({ deployment_id: "b", deployed_capital_usd: 250.5 }),
      dep({ deployment_id: "c", deployed_capital_usd: null }), // ignored, NOT 0
    ]);
    expect(agg.deployedCapitalUsd).toBe(1250.5);
    expect(agg.hasData).toBe(true);
    expect(agg.liveCount).toBe(3);
  });

  it("never coerces a null deployed_capital_usd to 0 in the sum", () => {
    // A single all-null row contributes nothing; the field stays null, not 0.
    const agg = aggregateCapitalRisk([dep({ deployed_capital_usd: null })]);
    expect(agg.deployedCapitalUsd).toBeNull();
    expect(agg.deployedCapitalUsd).not.toBe(0);
  });

  it("returns deployedCapitalUsd null when NONE are non-null even with multiple rows", () => {
    const agg = aggregateCapitalRisk([
      dep({ deployment_id: "a", deployed_capital_usd: null, drawdown_pct: 3 }),
      dep({ deployment_id: "b", deployed_capital_usd: null, drawdown_pct: 5 }),
    ]);
    expect(agg.deployedCapitalUsd).toBeNull();
    // …but other fields with data still surface, and hasData is true.
    expect(agg.worstDrawdownPct).toBe(5);
    expect(agg.hasData).toBe(true);
  });

  it("picks the WORST (max) drawdown over non-null values", () => {
    const agg = aggregateCapitalRisk([
      dep({ deployment_id: "a", drawdown_pct: 2.5 }),
      dep({ deployment_id: "b", drawdown_pct: 11.0 }),
      dep({ deployment_id: "c", drawdown_pct: null }), // ignored
    ]);
    expect(agg.worstDrawdownPct).toBe(11.0);
  });

  it("picks the TIGHTEST (min) daily-loss buffer and its paired budget", () => {
    const agg = aggregateCapitalRisk([
      dep({ deployment_id: "a", daily_loss_limit_remaining_usd: 500, daily_loss_budget_usd: 900 }),
      dep({ deployment_id: "b", daily_loss_limit_remaining_usd: 42, daily_loss_budget_usd: 100 }),
      dep({ deployment_id: "c", daily_loss_limit_remaining_usd: null }), // ignored
    ]);
    expect(agg.tightestDailyLossBufferUsd).toBe(42);
    expect(agg.tightestDailyLossBudgetUsd).toBe(100);
  });

  it("keeps a tightest buffer that has gone negative (over the kill line)", () => {
    const agg = aggregateCapitalRisk([
      dep({ deployment_id: "a", daily_loss_limit_remaining_usd: 100 }),
      dep({ deployment_id: "b", daily_loss_limit_remaining_usd: -25 }),
    ]);
    expect(agg.tightestDailyLossBufferUsd).toBe(-25);
  });

  it("aggregates each field independently — a row missing one field still contributes others", () => {
    const agg = aggregateCapitalRisk([
      dep({ deployment_id: "a", deployed_capital_usd: 800, drawdown_pct: null, daily_loss_limit_remaining_usd: 300 }),
      dep({ deployment_id: "b", deployed_capital_usd: null, drawdown_pct: 6, daily_loss_limit_remaining_usd: null }),
    ]);
    expect(agg.deployedCapitalUsd).toBe(800);
    expect(agg.worstDrawdownPct).toBe(6);
    expect(agg.tightestDailyLossBufferUsd).toBe(300);
    expect(agg.hasData).toBe(true);
  });

  it("treats a real 0 deployed_capital_usd as data (not the same as null)", () => {
    // A broker-sourced 0 (flat book) is honest data and must set hasData,
    // distinct from a null (no snapshot).
    const agg = aggregateCapitalRisk([dep({ deployed_capital_usd: 0 })]);
    expect(agg.deployedCapitalUsd).toBe(0);
    expect(agg.hasData).toBe(true);
  });
});

// bead s78.2: the risk-veto count is a REAL count of recorded risk-veto
// supervisor notes since the operator's last visit. The per-deployment field is
// null when NO `?since` boundary was supplied (can't count "since an unknown
// time"); a real count INCLUDING 0 is honest ("0 vetoes since you were last
// here"). The aggregate SUMS the non-null per-deployment counts; null only when
// EVERY per-deployment count is null.
describe("aggregateCapitalRisk — risk-veto count", () => {
  it("is null when every per-deployment veto count is null (no boundary)", () => {
    const agg = aggregateCapitalRisk([
      dep({ deployment_id: "a", risk_veto_count_since_last_visit: null }),
      dep({ deployment_id: "b", risk_veto_count_since_last_visit: null }),
    ]);
    expect(agg.riskVetoCount).toBeNull();
  });

  it("is null for an empty deployment list", () => {
    expect(aggregateCapitalRisk([]).riskVetoCount).toBeNull();
  });

  it("SUMS the non-null per-deployment veto counts", () => {
    const agg = aggregateCapitalRisk([
      dep({ deployment_id: "a", risk_veto_count_since_last_visit: 3 }),
      dep({ deployment_id: "b", risk_veto_count_since_last_visit: 5 }),
    ]);
    expect(agg.riskVetoCount).toBe(8);
  });

  it("ignores a null per-deployment count in the sum (never coerced to 0)", () => {
    const agg = aggregateCapitalRisk([
      dep({ deployment_id: "a", risk_veto_count_since_last_visit: 2 }),
      dep({ deployment_id: "b", risk_veto_count_since_last_visit: null }),
    ]);
    expect(agg.riskVetoCount).toBe(2);
  });

  it("keeps an honest summed 0 (a real 'no vetoes since last visit')", () => {
    // A boundary WAS supplied (counts are non-null), and the real count is 0.
    // That is an honest fact, distinct from null (unknown boundary).
    const agg = aggregateCapitalRisk([
      dep({ deployment_id: "a", risk_veto_count_since_last_visit: 0 }),
      dep({ deployment_id: "b", risk_veto_count_since_last_visit: 0 }),
    ]);
    expect(agg.riskVetoCount).toBe(0);
    expect(agg.riskVetoCount).not.toBeNull();
  });

  it("sums a mix of a real 0 and a real positive count", () => {
    const agg = aggregateCapitalRisk([
      dep({ deployment_id: "a", risk_veto_count_since_last_visit: 0 }),
      dep({ deployment_id: "b", risk_veto_count_since_last_visit: 4 }),
    ]);
    expect(agg.riskVetoCount).toBe(4);
  });
});

describe("bufferTone", () => {
  it("is healthy when the buffer is comfortably large relative to budget", () => {
    // >50% of budget remaining → healthy.
    expect(bufferTone(600, 1000)).toBe("healthy");
  });

  it("returns null in the middle decay band", () => {
    // 40% remaining is not warning yet and should not fabricate healthy.
    expect(bufferTone(400, 1000)).toBeNull();
  });

  it("warns as the buffer shrinks toward the kill line", () => {
    // 7% of budget remaining → warn band.
    expect(bufferTone(70, 1000)).toBe("warn");
  });

  it("is danger once the buffer has gone non-positive (kill line crossed)", () => {
    expect(bufferTone(0, 1000)).toBe("danger");
    expect(bufferTone(-50, 1000)).toBe("danger");
  });

  it("returns null tone when the buffer is null (no data → no color)", () => {
    expect(bufferTone(null, 1000)).toBeNull();
  });

  it("returns null tone when budget is null or zero (no ratio to compute)", () => {
    expect(bufferTone(100, null)).toBeNull();
    expect(bufferTone(100, 0)).toBeNull();
  });

  it("classifies monotonically as the buffer ratio falls", () => {
    const healthy = bufferTone(900, 1000);
    const neutral = bufferTone(400, 1000);
    const warn = bufferTone(80, 1000);
    const danger = bufferTone(0, 1000);
    expect(healthy).toBe("healthy");
    expect(neutral).toBeNull();
    expect(warn).toBe("warn");
    expect(danger).toBe("danger");
  });
});
