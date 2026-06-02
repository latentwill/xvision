import { type CandleColumns, type LineSeries } from "../types";
import type uPlot from "uplot";

/**
 * Normalize raw equity_usd points to return % relative to the first point.
 * Returns `{ time, value }` where value = ((equity_usd / base) - 1) * 100.
 */
export function normalizeEquityToReturnPct(
  raw: { time: number; equity_usd: number }[],
): { time: number; value: number }[] {
  if (raw.length === 0) return [];
  const base = raw[0].equity_usd;
  if (base === 0) return raw.map((r) => ({ time: r.time, value: 0 }));
  return raw.map((r) => ({
    time: r.time,
    value: ((r.equity_usd / base) - 1) * 100,
  }));
}

/**
 * Convert equity/drawdown point array to uPlot AlignedData.
 * uPlot x-axis expects timestamps in seconds (default ms multiplier is 1e-3).
 * CandleColumns.time is already in seconds; equity/drawdown points share the
 * same convention.
 */
export function columnarToUplotEquity(
  points: { time: number; value: number }[],
): uPlot.AlignedData {
  const len = points.length;
  const time: number[] = new Array(len);
  const values: (number | null)[] = new Array(len);
  for (let i = 0; i < len; i++) {
    time[i] = points[i].time;
    values[i] = points[i].value;
  }
  return [time, values];
}

/**
 * Convert N compare arms to uPlot AlignedData.
 * First row is the time index from the first arm; subsequent rows are value
 * columns for each arm.  Assumes all arms share the same time index.
 */
export function columnarToUplotCompare(
  arms: { time: number[]; values: number[] }[],
): uPlot.AlignedData {
  if (arms.length === 0) return [[]];
  const timeAxis = arms[0].time;
  const result: (number | null | undefined)[][] = [timeAxis];
  for (const arm of arms) {
    result.push(arm.values as (number | null | undefined)[]);
  }
  return result as uPlot.AlignedData;
}

/**
 * Convert a LineSeries to uPlot AlignedData as [time, value].
 */
export function columnarToUplotIndicator(series: LineSeries): uPlot.AlignedData {
  return [series.time, series.value as (number | null | undefined)[]];
}

/**
 * Convert CandleColumns to uPlot AlignedData for OHLCV rendering.
 * Returns [time, open, high, low, close, volume].
 * Timestamps are in seconds (uPlot default).
 */
export function columnarToUplotCandles(candles: CandleColumns): uPlot.AlignedData {
  return [
    candles.time,
    candles.open as (number | null | undefined)[],
    candles.high as (number | null | undefined)[],
    candles.low as (number | null | undefined)[],
    candles.close as (number | null | undefined)[],
    candles.volume as (number | null | undefined)[],
  ];
}

/**
 * Convert CandleColumns to uPlot AlignedData for volume histogram rendering.
 * Returns [time, volume].
 */
export function columnarToUplotHistogram(candles: CandleColumns): uPlot.AlignedData {
  return [
    candles.time,
    candles.volume as (number | null | undefined)[],
  ];
}
