/**
 * Tests for the pure pickSelectedInOrder helper backing
 * ComparisonABDashboard. (Canvas-heavy render lives at
 * /chart-lab/dashboards/compare for visual review.)
 */
import { describe, it, expect } from "vitest";

import { pickSelectedInOrder } from "./ComparisonABDashboard";
import type { MultiStrategyBundleEntry } from "../types";

function entry(id: string, partial: Partial<MultiStrategyBundleEntry> = {}): MultiStrategyBundleEntry {
  return {
    id,
    name: id.toUpperCase(),
    short: id,
    color: "#000000",
    kind: "Trend",
    equity: [],
    drawdown: [],
    monthly: [],
    metrics: { return: 0, sharpe: 0, mdd: 0, win: 0, pf: 0 },
    ...partial,
  };
}

describe("pickSelectedInOrder", () => {
  const strategies = [entry("fib"), entry("ema"), entry("brk"), entry("msw")];

  it("returns selected entries in the order of selectedIds", () => {
    const out = pickSelectedInOrder(strategies, ["brk", "fib"]);
    expect(out.map((s) => s.id)).toEqual(["brk", "fib"]);
  });

  it("filters out ids not present in the bundle", () => {
    const out = pickSelectedInOrder(strategies, ["fib", "unknown", "ema"]);
    expect(out.map((s) => s.id)).toEqual(["fib", "ema"]);
  });

  it("returns [] for empty selection", () => {
    expect(pickSelectedInOrder(strategies, [])).toEqual([]);
  });

  it("returns [] when bundle is empty", () => {
    expect(pickSelectedInOrder([], ["fib", "ema"])).toEqual([]);
  });
});
