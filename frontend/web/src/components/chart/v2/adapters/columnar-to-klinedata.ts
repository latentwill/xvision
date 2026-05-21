import { type CandleColumns } from "../types";
import { type KLineData } from "klinecharts";

/**
 * Convert columnar candle arrays to KlineCharts KLineData[].
 * Timestamps in CandleColumns are seconds; KlineCharts expects milliseconds.
 */
export function columnarToKLineData(candles: CandleColumns): KLineData[] {
  const { time, open, high, low, close, volume } = candles;
  const len = time.length;
  const result: KLineData[] = new Array(len);
  for (let i = 0; i < len; i++) {
    result[i] = {
      timestamp: time[i] * 1000,
      open: open[i],
      high: high[i],
      low: low[i],
      close: close[i],
      volume: volume[i],
    };
  }
  return result;
}

/**
 * Like columnarToKLineData but returns the same shape with timestamps
 * already in milliseconds — convenience alias for callers that need ms bars
 * without going through KLineData[].
 */
export function columnarToBarsMs(
  candles: CandleColumns,
): { timestamp: number; open: number; high: number; low: number; close: number; volume: number }[] {
  return columnarToKLineData(candles) as {
    timestamp: number;
    open: number;
    high: number;
    low: number;
    close: number;
    volume: number;
  }[];
}
