/**
 * kline-anchor — convert candle-array index + price into pixel
 * coordinates inside the candle pane.
 *
 * Two variants:
 *
 * 1. **Pure geometric approximation** (`xForIndex` / `yForPrice` /
 *    `deriveRange` + `DEFAULT_BOUNDS`). Used as a fallback when no
 *    klinecharts instance is available (e.g. chart-lab fixture render
 *    before mount). These helpers are also exercised by unit tests.
 *
 * 2. **Instance-aware anchor** (`createKlineAnchor`). Uses
 *    `chart.convertToPixel` for pixel-perfect x/y, and subscribes to
 *    `onVisibleRangeChange` + a `ResizeObserver` so the overlay
 *    re-anchors on every pan/zoom/resize.
 */
import type { Chart } from "klinecharts";

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

// ─── Instance-aware anchor ────────────────────────────────────────────────────

/**
 * The shape returned by `createKlineAnchor`.
 *
 * - `xForIndex(dataIndex)` → pixel x using `chart.convertToPixel`.
 * - `yForPrice(price)` → pixel y using `chart.convertToPixel`.
 * - `subscribeLayout(cb)` → register a callback fired on pan/zoom/resize;
 *   returns an unsubscribe function.
 *
 * Both coordinate methods return `NaN` when the chart is disposed or
 * the conversion result is missing (e.g. index out of visible range).
 */
export interface KlineAnchor {
  xForIndex: (dataIndex: number) => number;
  yForPrice: (price: number) => number;
  subscribeLayout: (cb: () => void) => () => void;
}

/**
 * Create a pixel-precise anchor tied to a live klinecharts `Chart`
 * instance.
 *
 * Uses `chart.convertToPixel` (KlineCharts v10) with:
 *   - `{ dataIndex }` for x → the chart maps the index to the
 *     candle-bar centre pixel accounting for pan/zoom.
 *   - `{ value }` for y → the chart maps the price to the y-axis pixel.
 *
 * `subscribeLayout` wires `chart.subscribeAction("onVisibleRangeChange")`
 * plus a `ResizeObserver` on the chart's root DOM node so callers
 * re-render after every pan, zoom, or container resize.
 */
export function createKlineAnchor(chart: Chart): KlineAnchor {
  function xForIndexFn(dataIndex: number): number {
    try {
      const result = chart.convertToPixel({ dataIndex });
      const x = (result as { x?: number }).x;
      return typeof x === "number" && Number.isFinite(x) ? x : NaN;
    } catch {
      return NaN;
    }
  }

  function yForPriceFn(price: number): number {
    try {
      const result = chart.convertToPixel({ value: price });
      const y = (result as { y?: number }).y;
      return typeof y === "number" && Number.isFinite(y) ? y : NaN;
    } catch {
      return NaN;
    }
  }

  function subscribeLayout(cb: () => void): () => void {
    chart.subscribeAction("onVisibleRangeChange", cb);

    // Also track resize of the chart's root element so the overlay
    // re-anchors when the container size changes independently of pan/zoom.
    let resizeObs: ResizeObserver | null = null;
    try {
      const el = chart.getDom();
      if (el) {
        resizeObs = new ResizeObserver(cb);
        resizeObs.observe(el);
      }
    } catch {
      // getDom unavailable (e.g. chart disposed before subscribe completes)
    }

    return () => {
      try {
        chart.unsubscribeAction("onVisibleRangeChange", cb);
      } catch {
        // chart may already be disposed
      }
      resizeObs?.disconnect();
    };
  }

  return { xForIndex: xForIndexFn, yForPrice: yForPriceFn, subscribeLayout };
}
