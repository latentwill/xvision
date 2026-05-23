/**
 * kline-anchor — convert candle-array index + price into pixel
 * coordinates inside the candle pane.
 *
 * B3 ships a **geometric approximation** that doesn't reach into the
 * klinecharts instance. It assumes:
 *   - x is uniformly spaced across the host div's content width minus
 *     left + right padding (the price-axis margin).
 *   - y is linearly mapped between min/max of the visible candle prices.
 *
 * This is correct enough for the AnnotationOverlay's connectors to
 * point at the right candles when pan/zoom is disabled (the default
 * for B3). A pixel-perfect upgrade that consults
 * klinecharts' `convertToPixel` / `onVisibleRangeChange` is a
 * follow-up — track via the chart-rework spec §3 B3 verification list.
 */

export interface AnchorBounds {
  /** Width of the candle pane host div, in CSS pixels. */
  width: number;
  /** Height of the candle pane host div, in CSS pixels. */
  height: number;
  /** Left inset before the first candle (px). */
  padLeft: number;
  /** Right inset after the last candle — typically the price-axis margin (px). */
  padRight: number;
  /** Top inset above the highest price (px). */
  padTop: number;
  /** Bottom inset — typically the time-axis margin (px). */
  padBottom: number;
}

export interface PriceRange {
  min: number;
  max: number;
}

export const DEFAULT_BOUNDS: AnchorBounds = {
  width: 0,
  height: 0,
  padLeft: 12,
  padRight: 80,
  padTop: 12,
  padBottom: 32,
};

/**
 * Map a candle index in [0, count) to a pixel x within `bounds`.
 * Returns `NaN` for out-of-range indices.
 */
export function xForIndex(
  index: number,
  candleCount: number,
  bounds: AnchorBounds,
): number {
  if (candleCount <= 0 || index < 0 || index >= candleCount) return NaN;
  const usable = bounds.width - bounds.padLeft - bounds.padRight;
  if (usable <= 0) return NaN;
  if (candleCount === 1) return bounds.padLeft + usable / 2;
  return bounds.padLeft + (index / (candleCount - 1)) * usable;
}

/**
 * Map a price to a pixel y within `bounds`. Higher prices render
 * higher on screen (smaller y). Returns `NaN` if range is degenerate
 * or bounds have no height.
 */
export function yForPrice(
  price: number,
  range: PriceRange,
  bounds: AnchorBounds,
): number {
  const span = range.max - range.min;
  if (span <= 0) return NaN;
  const usable = bounds.height - bounds.padTop - bounds.padBottom;
  if (usable <= 0) return NaN;
  const frac = (price - range.min) / span;
  return bounds.padTop + (1 - frac) * usable;
}

/**
 * Derive {min, max} for the visible candle range, plus a small
 * cosmetic padding above/below so peaks/troughs aren't flush to the
 * edges.
 */
export function deriveRange(
  highs: readonly number[],
  lows: readonly number[],
  paddingFraction = 0.04,
): PriceRange {
  if (highs.length === 0 || lows.length === 0) return { min: 0, max: 1 };
  let min = Infinity;
  let max = -Infinity;
  for (const v of highs) if (v > max) max = v;
  for (const v of lows) if (v < min) min = v;
  if (!Number.isFinite(min) || !Number.isFinite(max) || min === max) {
    return { min: min - 1, max: max + 1 };
  }
  const pad = (max - min) * paddingFraction;
  return { min: min - pad, max: max + pad };
}
