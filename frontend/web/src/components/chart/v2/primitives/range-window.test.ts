import { describe, expect, it } from "vitest";

import { granularitySeconds, rangeWindowSeconds } from "./range-window";

describe("rangeWindowSeconds", () => {
  it("maps finite presets to their second-counts", () => {
    expect(rangeWindowSeconds("1h")).toBe(3_600);
    expect(rangeWindowSeconds("4h")).toBe(4 * 3_600);
    expect(rangeWindowSeconds("6h")).toBe(6 * 3_600);
    expect(rangeWindowSeconds("12h")).toBe(12 * 3_600);
    expect(rangeWindowSeconds("1d")).toBe(86_400);
    expect(rangeWindowSeconds("1w")).toBe(7 * 86_400);
  });

  it("maps All to null", () => {
    expect(rangeWindowSeconds("All")).toBeNull();
  });
});

describe("granularitySeconds", () => {
  it("parses minute/hour/day/week/month units", () => {
    expect(granularitySeconds("15m")).toBe(900);
    expect(granularitySeconds("1h")).toBe(3600);
    expect(granularitySeconds("1d")).toBe(86_400);
    expect(granularitySeconds("1w")).toBe(7 * 86_400);
    expect(granularitySeconds("1M")).toBe(30 * 86_400);
  });

  it("tolerates surrounding whitespace and internal space", () => {
    expect(granularitySeconds("  4h ")).toBe(4 * 3600);
    expect(granularitySeconds("30 m")).toBe(1800);
  });

  it("returns null for unparseable input", () => {
    expect(granularitySeconds("bogus")).toBeNull();
    expect(granularitySeconds("")).toBeNull();
    expect(granularitySeconds("1y")).toBeNull();
    expect(granularitySeconds("h")).toBeNull();
  });
});
