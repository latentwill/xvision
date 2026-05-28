import { describe, expect, test } from "vitest";

import {
  decisionCounts,
  fmtStepStamp,
  shortAsset,
  stepOrdinalsByDecision,
  type TimelineDecision,
} from "./decision-view";

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

describe("fmtStepStamp", () => {
  test("renders ISO UTC as YYYY-MM-DD HH:MM:SS (no ms)", () => {
    expect(fmtStepStamp("2024-01-12T21:00:00Z")).toBe("2024-01-12 21:00:00");
    expect(fmtStepStamp("2024-02-09T08:00:00+00:00")).toBe("2024-02-09 08:00:00");
  });

  test("forces UTC regardless of local timezone", () => {
    // Same instant; the renderer must not shift it into the host TZ.
    expect(fmtStepStamp("2024-01-12T21:00:00Z")).toBe("2024-01-12 21:00:00");
  });

  test("drops sub-second precision but keeps the seconds field", () => {
    expect(fmtStepStamp("2024-01-12T21:00:00.500Z")).toBe("2024-01-12 21:00:00");
    expect(fmtStepStamp("2024-01-12T21:00:42.999Z")).toBe("2024-01-12 21:00:42");
  });

  test("returns the raw input on parse failure (matches prior behaviour)", () => {
    expect(fmtStepStamp("not-a-date")).toBe("not-a-date");
    expect(fmtStepStamp("")).toBe("");
  });
});

describe("decisionCounts", () => {
  // Two steps, each fanned out into BTC + ETH (real multi-asset shape).
  const TS_A = "2024-01-01T20:00:00+00:00";
  const TS_B = "2024-01-07T13:00:00+00:00";
  const rows: TimelineDecision[] = [
    td(0, TS_A, "BTC/USD"),
    td(1, TS_A, "ETH/USD"),
    td(2, TS_B, "BTC/USD"),
    td(3, TS_B, "ETH/USD"),
  ];

  test("unfiltered view: steps count distinct timestamps, trader calls count rows", () => {
    const c = decisionCounts(rows, rows);
    expect(c.totalSteps).toBe(2);
    expect(c.viewedSteps).toBe(2);
    expect(c.engagedSteps).toBe(2);
    expect(c.totalTraderCalls).toBe(4);
    expect(c.viewedTraderCalls).toBe(4);
  });

  test("narrowing the view by row narrows steps too", () => {
    // The action-filter pill / search box can shrink the view to a subset of
    // rows; the chip should report the visible step count, not the total.
    const c = decisionCounts([rows[0]!, rows[1]!], rows);
    expect(c.viewedSteps).toBe(1);
    expect(c.totalSteps).toBe(2);
    expect(c.viewedTraderCalls).toBe(2);
    expect(c.totalTraderCalls).toBe(4);
  });

  test("engagedSteps counts steps where at least one visible row is engaged", () => {
    const mixed: TimelineDecision[] = [
      { ...rows[0]!, phase: "engaged" },
      { ...rows[1]!, phase: "filtered" }, // same step as [0] — still engaged at step level
      { ...rows[2]!, phase: "filtered" },
      { ...rows[3]!, phase: "filtered" }, // step B has no engaged row
    ];
    const c = decisionCounts(mixed, mixed);
    expect(c.totalSteps).toBe(2);
    expect(c.engagedSteps).toBe(1);
  });

  test("empty view zeroes the visible counts but preserves totals", () => {
    const c = decisionCounts([], rows);
    expect(c.viewedSteps).toBe(0);
    expect(c.engagedSteps).toBe(0);
    expect(c.viewedTraderCalls).toBe(0);
    expect(c.totalSteps).toBe(2);
    expect(c.totalTraderCalls).toBe(4);
  });
});
