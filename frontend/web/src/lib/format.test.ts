import { describe, expect, it } from "vitest";

import {
  formatCadence,
  formatCostUsd,
  formatCostUsdPrecise,
  formatPercent,
  formatSharpe,
  formatSpendUsd,
} from "./format";

describe("formatCostUsd", () => {
  it("renders zero as $0.00", () => {
    expect(formatCostUsd(0)).toBe("$0.00");
  });

  it("renders the <$0.0001 floor for very small positive values", () => {
    expect(formatCostUsd(1e-7)).toBe("<$0.0001");
    expect(formatCostUsd(1e-5)).toBe("<$0.0001");
    expect(formatCostUsd(0.00009)).toBe("<$0.0001");
  });

  it("renders sub-cent values with six fraction digits", () => {
    expect(formatCostUsd(0.0001)).toBe("$0.000100");
    expect(formatCostUsd(0.001)).toBe("$0.001000");
    expect(formatCostUsd(0.009999)).toBe("$0.009999");
  });

  it("renders sub-dollar values with four fraction digits", () => {
    expect(formatCostUsd(0.01)).toBe("$0.0100");
    expect(formatCostUsd(0.1)).toBe("$0.1000");
    expect(formatCostUsd(0.9999)).toBe("$0.9999");
  });

  it("renders dollar-and-up values with two fraction digits", () => {
    expect(formatCostUsd(1.23)).toBe("$1.23");
    expect(formatCostUsd(123.45)).toBe("$123.45");
  });

  it("renders large values with grouping", () => {
    expect(formatCostUsd(12_345.67)).toBe("$12,345.67");
    expect(formatCostUsd(1_234_567.89)).toBe("$1,234,567.89");
  });

  it("renders negative values with a leading minus", () => {
    expect(formatCostUsd(-1.5)).toBe("-$1.50");
    expect(formatCostUsd(-0.00005)).toBe("<$0.0001");
  });

  it("renders null / undefined / NaN as em-dash", () => {
    expect(formatCostUsd(null)).toBe("—");
    expect(formatCostUsd(undefined)).toBe("—");
    expect(formatCostUsd(Number.NaN)).toBe("—");
    expect(formatCostUsd(Number.POSITIVE_INFINITY)).toBe("—");
  });

  it("does not regress the historical $0.01+ display", () => {
    // Pre-change SpanInspector/RunStatusStrip used toFixed(4) which would
    // render these as $0.0100 / $0.5000 / $1.2300. The new bands keep
    // sub-dollar at four-decimal and ≥$1 at two-decimal — the visible
    // delta on the $1.23 case is intentional (operators prefer fewer
    // trailing zeros once cost is no longer subcent).
    expect(formatCostUsd(0.0123)).toBe("$0.0123");
    expect(formatCostUsd(0.5)).toBe("$0.5000");
    expect(formatCostUsd(0.9999)).toBe("$0.9999");
  });
});

describe("formatSpendUsd", () => {
  it("renders zero / null / negative / non-finite as em-dash (unknown spend)", () => {
    // Unlike formatCostUsd, a zero aggregate means "no priced model call
    // recorded" — spend is unknown, not a genuine $0.00.
    expect(formatSpendUsd(0)).toBe("—");
    expect(formatSpendUsd(null)).toBe("—");
    expect(formatSpendUsd(undefined)).toBe("—");
    expect(formatSpendUsd(-1.5)).toBe("—");
    expect(formatSpendUsd(Number.NaN)).toBe("—");
    expect(formatSpendUsd(Number.POSITIVE_INFINITY)).toBe("—");
  });

  it("delegates positive amounts to formatCostUsd", () => {
    expect(formatSpendUsd(0.18)).toBe(formatCostUsd(0.18));
    expect(formatSpendUsd(0.18)).toBe("$0.1800");
    expect(formatSpendUsd(12.5)).toBe("$12.50");
  });
});

describe("formatCostUsdPrecise", () => {
  it("renders zero as $0.00", () => {
    expect(formatCostUsdPrecise(0)).toBe("$0.00");
  });

  it("renders empty string for null/undefined/non-finite", () => {
    expect(formatCostUsdPrecise(null)).toBe("");
    expect(formatCostUsdPrecise(undefined)).toBe("");
    expect(formatCostUsdPrecise(Number.NaN)).toBe("");
  });

  it("surfaces small values past the display floor", () => {
    expect(formatCostUsdPrecise(0.0000001)).toBe("$0.0000001");
    expect(formatCostUsdPrecise(0.00000123)).toBe("$0.00000123");
    expect(formatCostUsdPrecise(0.00001)).toBe("$0.00001");
  });

  it("trims trailing zeros", () => {
    expect(formatCostUsdPrecise(0.5)).toBe("$0.5");
    expect(formatCostUsdPrecise(1)).toBe("$1");
    expect(formatCostUsdPrecise(12_345.67)).toBe("$12345.67");
  });

  it("preserves negative sign", () => {
    expect(formatCostUsdPrecise(-0.00000123)).toBe("-$0.00000123");
  });
});

describe("formatCadence (smoke)", () => {
  it("formats minutes-only durations", () => {
    expect(formatCadence(15)).toBe("15m");
  });
});

describe("formatPercent", () => {
  it("rounds full-precision floats to 2dp so they fit the stat boxes", () => {
    // The operator-reported overflow: "+0.19077721054834548%".
    expect(formatPercent(0.19077721054834548)).toBe("+0.19%");
  });

  it("strips trailing zeros (47.2 stays 47.2, not 47.20)", () => {
    expect(formatPercent(47.2)).toBe("+47.2%");
    expect(formatPercent(47)).toBe("+47%");
  });

  it("prefixes a + on positive values by default and keeps the native minus", () => {
    expect(formatPercent(12.5)).toBe("+12.5%");
    expect(formatPercent(-8.3)).toBe("-8.3%");
  });

  it("omits the + when signed:false (win rate / drawdown)", () => {
    expect(formatPercent(63.4, { signed: false })).toBe("63.4%");
    expect(formatPercent(-12.55, { signed: false })).toBe("-12.55%");
  });

  it("renders null / undefined / non-finite as em-dash", () => {
    expect(formatPercent(null)).toBe("—");
    expect(formatPercent(undefined)).toBe("—");
    expect(formatPercent(Number.NaN)).toBe("—");
    expect(formatPercent(Number.POSITIVE_INFINITY)).toBe("—");
  });
});

describe("formatSharpe", () => {
  it("rounds full-precision ratios to 2dp (operator overflow case)", () => {
    // The operator-reported overflow: "4.062559453920121".
    expect(formatSharpe(4.062559453920121)).toBe("+4.06");
  });

  it("strips trailing zeros and signs positive ratios", () => {
    expect(formatSharpe(2.1)).toBe("+2.1");
    expect(formatSharpe(-0.5)).toBe("-0.5");
  });

  it("renders null / non-finite as em-dash", () => {
    expect(formatSharpe(null)).toBe("—");
    expect(formatSharpe(Number.NaN)).toBe("—");
  });
});
