import { describe, expect, it } from "vitest";

import type { ScenarioChartPayload } from "@/api/types.gen";
import { scenarioChartPayloadToV2 } from "./scenario-chart-payload";

function payload(): ScenarioChartPayload {
  return {
    scenario: { id: "s1", granularity: "1h" } as never,
    preview_asset: "BTC",
    bars: [
      { time: 1, open: 1, high: 2, low: 0.5, close: 1.5, volume: 100 },
      { time: 2, open: 1.5, high: 2.5, low: 1, close: 2, volume: 120 },
    ],
    indicators: {
      sma_20: [{ time: 1, value: 1.2 }],
      sma_30: [],
      sma_50: [],
      sma_60: [],
      sma_90: [],
      sma_200: [],
      ema_20: [],
      ema_30: [],
      ema_50: [],
      ema_60: [],
      ema_90: [],
      ema_200: [],
      bollinger: { upper: [], middle: [], lower: [] },
      donchian: { upper: [], lower: [] },
      rsi_14: [],
      macd: { line: [], signal: [], histogram: [] },
      atr_14: [],
    } as never,
    cache_status: { type: "FullyCached", bar_count: 2 } as never,
  } as ScenarioChartPayload;
}

describe("scenarioChartPayloadToV2", () => {
  it("maps bars to columnar candles with correct kind/asset/granularity from args", () => {
    const result = scenarioChartPayloadToV2(payload(), "ETH", "4h");

    expect(result.kind).toBe("scenario");
    expect(result.asset).toBe("ETH");
    expect(result.granularity).toBe("4h");
    expect(result.candles.time).toEqual([1, 2]);
    expect(result.candles.close).toEqual([1.5, 2]);
    expect(result.candles.open).toEqual([1, 1.5]);
    expect(result.candles.high).toEqual([2, 2.5]);
    expect(result.candles.low).toEqual([0.5, 1]);
    expect(result.candles.volume).toEqual([100, 120]);
  });

  it("maps sma_20 to indicators.sma20 and defaults equity/markers/positions to []", () => {
    const result = scenarioChartPayloadToV2(payload(), "BTC", "1h");

    expect(result.indicators.sma20).toEqual({ time: [1], value: [1.2] });
    expect(result.equity).toEqual([]);
    expect(result.markers).toEqual([]);
    expect(result.positions).toEqual([]);
  });

  it("handles empty bars — all candle arrays are empty", () => {
    const empty: ScenarioChartPayload = {
      ...payload(),
      bars: [],
    };
    const result = scenarioChartPayloadToV2(empty, "SOL", "15m");

    expect(result.candles.time).toEqual([]);
    expect(result.candles.open).toEqual([]);
    expect(result.candles.high).toEqual([]);
    expect(result.candles.low).toEqual([]);
    expect(result.candles.close).toEqual([]);
    expect(result.candles.volume).toEqual([]);
  });
});
