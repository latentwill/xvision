import { describe, expect, it } from "vitest";

import { chart2ThemeFor } from "../hooks/useChart2Theme";
import type { IndicatorMap, LineSeries } from "../types";
import { OVERLAY_LINE_KEYS, overlayLineDescriptors } from "./overlay-lines";

const theme = chart2ThemeFor("dark");

function lineFor(times: number[], values: number[]): LineSeries {
  return { time: times, value: values };
}

describe("OVERLAY_LINE_KEYS", () => {
  it("covers the full candle-pane line set and excludes oscillators", () => {
    expect(OVERLAY_LINE_KEYS).toEqual([
      "sma20",
      "sma30",
      "sma50",
      "sma60",
      "sma90",
      "sma200",
      "ema20",
      "ema30",
      "ema50",
      "ema60",
      "ema90",
      "ema200",
      "bollUpper",
      "bollMiddle",
      "bollLower",
      "donchianUpper",
      "donchianLower",
    ]);
    // Oscillators belong to uPlot subpanes, not the candle pane.
    expect(OVERLAY_LINE_KEYS).not.toContain("rsi");
    expect(OVERLAY_LINE_KEYS).not.toContain("macdLine");
    expect(OVERLAY_LINE_KEYS).not.toContain("atr");
  });
});

describe("overlayLineDescriptors", () => {
  it("maps a present line to a xvnLine descriptor with ms timestamps", () => {
    const indicators: IndicatorMap = {
      sma20: lineFor([1_700_000_000, 1_700_000_060], [100, 101]),
    };

    const descriptors = overlayLineDescriptors(indicators, theme, {});
    expect(descriptors).toHaveLength(1);

    const [d] = descriptors;
    expect(d.name).toBe("xvnLine");
    expect(d.points).toEqual([
      { timestamp: 1_700_000_000_000, value: 100 },
      { timestamp: 1_700_000_060_000, value: 101 },
    ]);
    expect(d.extendData.key).toBe("sma20");
    expect(d.extendData.color).toBe(theme.overlay.sma20);
    expect(d.extendData.dashed).toBe(false);
  });

  it("marks EMA lines as dashed and SMA/boll/donchian as solid", () => {
    const indicators: IndicatorMap = {
      ema50: lineFor([1, 2], [10, 11]),
      sma50: lineFor([1, 2], [10, 11]),
      bollUpper: lineFor([1, 2], [10, 11]),
      donchianLower: lineFor([1, 2], [10, 11]),
    };

    const byKey = new Map(
      overlayLineDescriptors(indicators, theme, {}).map((d) => [
        d.extendData.key,
        d,
      ]),
    );

    expect(byKey.get("ema50")?.extendData.dashed).toBe(true);
    expect(byKey.get("sma50")?.extendData.dashed).toBe(false);
    expect(byKey.get("bollUpper")?.extendData.dashed).toBe(false);
    expect(byKey.get("donchianLower")?.extendData.dashed).toBe(false);
  });

  it("skips absent and empty line keys", () => {
    const indicators: IndicatorMap = {
      sma20: lineFor([], []),
      sma50: lineFor([1, 2], [10, 11]),
      // ema20 absent
    };

    const descriptors = overlayLineDescriptors(indicators, theme, {});
    const keys = descriptors.map((d) => d.extendData.key);
    expect(keys).toEqual(["sma50"]);
  });

  it("skips keys explicitly toggled off, keeps undefined/true active", () => {
    const indicators: IndicatorMap = {
      sma20: lineFor([1, 2], [1, 2]),
      sma50: lineFor([1, 2], [1, 2]),
      ema20: lineFor([1, 2], [1, 2]),
    };

    const descriptors = overlayLineDescriptors(indicators, theme, {
      sma20: false,
      sma50: true,
      // ema20 undefined → active
    });
    const keys = descriptors.map((d) => d.extendData.key);
    expect(keys).toEqual(["sma50", "ema20"]);
  });

  it("emits descriptors in OVERLAY_LINE_KEYS order", () => {
    const indicators: IndicatorMap = {
      ema20: lineFor([1], [1]),
      sma20: lineFor([1], [1]),
      donchianUpper: lineFor([1], [1]),
    };

    const keys = overlayLineDescriptors(indicators, theme, {}).map(
      (d) => d.extendData.key,
    );
    expect(keys).toEqual(["sma20", "ema20", "donchianUpper"]);
  });
});
