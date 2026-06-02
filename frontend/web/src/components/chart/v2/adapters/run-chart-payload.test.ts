import { describe, expect, it } from "vitest";

import type { RunChartPayload } from "@/api/types.gen";
import { runChartPayloadToV2 } from "./run-chart-payload";

function payload(overrides: Partial<RunChartPayload> = {}): RunChartPayload {
  return {
    run_id: "run_1",
    scenario_id: "scenario_1",
    asset: "BTC",
    granularity: "1h",
    time_window: {} as RunChartPayload["time_window"],
    bars: [
      { time: 100, open: 10, high: 12, low: 9, close: 11, volume: 1000 },
      { time: 200, open: 11, high: 13, low: 10, close: 12, volume: 1001 },
    ],
    indicators: {
      sma_20: [],
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
    },
    equity: [],
    drawdown: [],
    position: [],
    markers: { trades: [], vetoes: [], holds: [] },
    ...overrides,
  };
}

describe("runChartPayloadToV2", () => {
  it("extends candles to the final eval equity timestamp", () => {
    const v2 = runChartPayloadToV2(
      payload({
        equity: [{ time: 260, equity_usd: 10_125 }],
      }),
    );

    expect(v2.candles.time).toEqual([100, 200, 260]);
    expect(v2.candles.open.at(-1)).toBe(12);
    expect(v2.candles.high.at(-1)).toBe(12);
    expect(v2.candles.low.at(-1)).toBe(12);
    expect(v2.candles.close.at(-1)).toBe(12);
    expect(v2.candles.volume.at(-1)).toBe(0);
  });

  it("includes a final trade price in the synthetic completion candle range", () => {
    const v2 = runChartPayloadToV2(
      payload({
        markers: {
          trades: [
            {
              time: 260,
              side: "Buy",
              price: 15,
              size: 1,
              fee: 0,
              pnl_realized: null,
              decision_index: 2,
              justification: null,
            },
          ],
          vetoes: [],
          holds: [],
        },
      }),
    );

    expect(v2.candles.time.at(-1)).toBe(260);
    expect(v2.candles.high.at(-1)).toBe(15);
    expect(v2.candles.low.at(-1)).toBe(12);
  });

  it("does not append a completion candle when the final event is already covered", () => {
    const v2 = runChartPayloadToV2(
      payload({
        equity: [{ time: 200, equity_usd: 10_125 }],
      }),
    );

    expect(v2.candles.time).toEqual([100, 200]);
  });
});
