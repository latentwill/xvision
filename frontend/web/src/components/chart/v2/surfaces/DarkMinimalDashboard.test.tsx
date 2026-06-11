/**
 * Tests for the pure helpers backing DarkMinimalDashboard. The
 * full surface render exercises uPlot inside MultiStrategyEquityPane,
 * which doesn't play well in jsdom (canvas + matchMedia); the helpers
 * carry the load and we keep canvas-render fidelity for the
 * /chart-lab/dashboards/overview manual surface review.
 */
import { describe, it, expect } from "vitest";

// matchMedia is polyfilled in src/test-setup.ts (uPlot calls it at
// module-load); DarkMinimalDashboard transitively imports uPlot via
// MultiStrategyEquityPane.

import {
  pickLead,
  toEquitySeries,
  deriveDrawdownStats,
  fixtureLabel,
} from "./DarkMinimalDashboard";
import type { MultiStrategyEquityBundle } from "../types";

function makeBundle(
  overrides: Partial<MultiStrategyEquityBundle> = {},
): MultiStrategyEquityBundle {
  return {
    kind: "multi_strategy_equity",
    generatedAt: 0,
    granularity: "1d",
    time: [1, 2, 3, 4],
    strategies: [
      {
        id: "fib",
        name: "Fibonacci GC",
        short: "Fib · GC",
        color: "#D4A547",
        kind: "Trend",
        equity: [0, 1, 2, 3],
        drawdown: [0, -0.5, -1.0, 0],
        monthly: [],
        metrics: { return: 82.4, sharpe: 1.9, mdd: -18.7, win: 58.6, pf: 1.81 },
      },
      {
        id: "ema",
        name: "EMA Pullback",
        short: "EMA",
        color: "#E8DCB0",
        kind: "Trend",
        equity: [0, 0.5, 1, 1.5],
        drawdown: [0, 0, -0.2, -0.1],
        monthly: [],
        metrics: { return: 46.3, sharpe: 1.4, mdd: -14.4, win: 54.1, pf: 1.46 },
      },
    ],
    ...overrides,
  };
}

describe("pickLead", () => {
  it("returns the strategy whose id matches bundle.lead", () => {
    const b = makeBundle({ lead: "ema" });
    expect(pickLead(b)?.id).toBe("ema");
  });
  it("falls back to strategies[0] when lead is missing", () => {
    const b = makeBundle({ lead: "unknown" });
    expect(pickLead(b)?.id).toBe("fib");
  });
  it("falls back to strategies[0] when lead is undefined", () => {
    const b = makeBundle();
    expect(pickLead(b)?.id).toBe("fib");
  });
  it("returns undefined for empty bundle", () => {
    const b = makeBundle({ strategies: [], lead: undefined });
    expect(pickLead(b)).toBeUndefined();
  });
});

describe("fixtureLabel", () => {
  it("returns 'Sample data' when the bundle is the fixture stub", () => {
    const b = makeBundle({ isFixture: true });
    expect(fixtureLabel(b)).toBe("Sample data");
  });
  it("returns undefined for real builder output (isFixture unset)", () => {
    const b = makeBundle();
    expect(fixtureLabel(b)).toBeUndefined();
  });
  it("returns undefined when isFixture is explicitly false", () => {
    const b = makeBundle({ isFixture: false });
    expect(fixtureLabel(b)).toBeUndefined();
  });
});

describe("toEquitySeries", () => {
  it("maps bundle strategies to MultiStrategyEquityPane series", () => {
    const b = makeBundle();
    const s = toEquitySeries(b.strategies);
    expect(s).toHaveLength(2);
    expect(s[0]).toMatchObject({
      id: "fib",
      label: "Fib · GC",
      color: "#D4A547",
      values: [0, 1, 2, 3],
    });
    expect(s[0].dashed).toBeUndefined();
  });
  it("propagates the dashed flag for benchmark strategies", () => {
    const b = makeBundle({
      strategies: [
        {
          id: "btc",
          name: "BTC HOLD",
          short: "BTC",
          color: "#6B6553",
          kind: "Bench",
          dashed: true,
          equity: [0, -1, -2, -3],
          drawdown: [0, -1, -2, -3],
          monthly: [],
          metrics: { return: -3.2, sharpe: 0.2, mdd: -26.8, win: 43.1, pf: 0.89 },
        },
      ],
    });
    const s = toEquitySeries(b.strategies);
    expect(s[0].dashed).toBe(true);
  });
});

describe("deriveDrawdownStats", () => {
  it("computes max, avg, duration, and recovery on a recovered curve", () => {
    // drawdown: 0, -1, -3, -2, 0, -1, 0
    // - max = -3 at idx 2
    // - longest negative run = 3 (idx 1..3)
    // - recovery from idx 2 to next ≥ 0: idx 4 ⇒ 2 steps
    const stats = deriveDrawdownStats([0, -1, -3, -2, 0, -1, 0]);
    expect(stats.maxDrawdownPct).toBe(-3);
    expect(stats.avgDrawdownPct).toBeCloseTo(-1.0, 6);
    expect(stats.durationDays).toBe(3);
    expect(stats.recoveryDays).toBe(2);
  });
  it("returns null recovery when still underwater at the end", () => {
    const stats = deriveDrawdownStats([0, -1, -2, -3]);
    expect(stats.maxDrawdownPct).toBe(-3);
    expect(stats.recoveryDays).toBeNull();
    expect(stats.durationDays).toBe(3);
  });
  it("handles an empty drawdown column without crashing", () => {
    const stats = deriveDrawdownStats([]);
    expect(stats.maxDrawdownPct).toBe(0);
    expect(stats.avgDrawdownPct).toBe(0);
    expect(stats.durationDays).toBe(0);
    expect(stats.recoveryDays).toBe(0);
  });
  it("treats a flat-at-zero curve as no drawdown", () => {
    const stats = deriveDrawdownStats([0, 0, 0, 0]);
    expect(stats.maxDrawdownPct).toBe(0);
    expect(stats.avgDrawdownPct).toBe(0);
    expect(stats.durationDays).toBe(0);
    expect(stats.recoveryDays).toBe(0);
  });
});
