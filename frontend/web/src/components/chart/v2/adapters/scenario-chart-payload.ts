import type { ScenarioChartPayload } from "@/api/types.gen";
import type {
  IndicatorMap,
  ScenarioChartV2Payload,
} from "../types";

export function scenarioChartPayloadToV2(
  payload: ScenarioChartPayload,
  asset: string,
  granularity: string,
): ScenarioChartV2Payload {
  return {
    kind: "scenario",
    asset,
    granularity,
    candles: {
      time: payload.bars.map((b) => b.time),
      open: payload.bars.map((b) => b.open),
      high: payload.bars.map((b) => b.high),
      low: payload.bars.map((b) => b.low),
      close: payload.bars.map((b) => b.close),
      volume: payload.bars.map((b) => b.volume),
    },
    indicators: indicatorMap(payload),
    equity: [],
    markers: [],
    positions: [],
  };
}

function indicatorMap(payload: ScenarioChartPayload): IndicatorMap {
  const i = payload.indicators;
  return {
    sma20: line(i.sma_20),
    sma30: line(i.sma_30),
    sma50: line(i.sma_50),
    sma60: line(i.sma_60),
    sma90: line(i.sma_90),
    sma200: line(i.sma_200),
    ema20: line(i.ema_20),
    ema30: line(i.ema_30),
    ema50: line(i.ema_50),
    ema60: line(i.ema_60),
    ema90: line(i.ema_90),
    ema200: line(i.ema_200),
    bollUpper: line(i.bollinger.upper),
    bollMiddle: line(i.bollinger.middle),
    bollLower: line(i.bollinger.lower),
    donchianUpper: line(i.donchian.upper),
    donchianLower: line(i.donchian.lower),
    rsi: line(i.rsi_14),
    macdLine: line(i.macd.line),
    macdSignal: line(i.macd.signal),
    macdHist: line(i.macd.histogram),
    atr: line(i.atr_14),
  };
}

function line(points: Array<{ time: number; value: number }> | undefined) {
  const rows = points ?? [];
  return {
    time: rows.map((p) => p.time),
    value: rows.map((p) => p.value),
  };
}
