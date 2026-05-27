import { describe, expect, test } from "vitest";

import { shortAsset, stepOrdinalsByDecision, type TimelineDecision } from "./decision-view";

function td(i: number, t: string, asset = "BTC/USD"): TimelineDecision {
  return { i, t, phase: "engaged", asset };
}

describe("shortAsset", () => {
  test("strips the quote currency from a pair", () => {
    expect(shortAsset("BTC/USD")).toBe("BTC");
    expect(shortAsset("ETH/USD")).toBe("ETH");
  });
  test("passes a bare symbol and empty string through", () => {
    expect(shortAsset("BTC")).toBe("BTC");
    expect(shortAsset("")).toBe("");
  });
});

describe("stepOrdinalsByDecision", () => {
  test("two assets sharing a timestamp collapse to the same 1-based step", () => {
    // Mirrors a real multi-asset run: each step fans out into BTC + ETH rows
    // at one identical timestamp (decision_index 0/1, 2/3, …).
    const rows = [
      td(0, "2024-01-01T20:00:00+00:00", "BTC/USD"),
      td(1, "2024-01-01T20:00:00+00:00", "ETH/USD"),
      td(2, "2024-01-07T13:00:00+00:00", "BTC/USD"),
      td(3, "2024-01-07T13:00:00+00:00", "ETH/USD"),
    ];
    const m = stepOrdinalsByDecision(rows);
    expect(m.get(0)).toBe(1);
    expect(m.get(1)).toBe(1);
    expect(m.get(2)).toBe(2);
    expect(m.get(3)).toBe(2);
  });

  test("single-asset run numbers each decision sequentially", () => {
    const rows = [
      td(0, "2024-01-01T20:00:00+00:00"),
      td(1, "2024-01-02T20:00:00+00:00"),
      td(2, "2024-01-03T20:00:00+00:00"),
    ];
    const m = stepOrdinalsByDecision(rows);
    expect([m.get(0), m.get(1), m.get(2)]).toEqual([1, 2, 3]);
  });

  test("ranks by chronological time, not input order", () => {
    const rows = [
      td(0, "2024-01-07T13:00:00+00:00"),
      td(1, "2024-01-01T20:00:00+00:00"),
    ];
    const m = stepOrdinalsByDecision(rows);
    expect(m.get(1)).toBe(1); // earlier timestamp ⇒ step 1
    expect(m.get(0)).toBe(2);
  });
});
