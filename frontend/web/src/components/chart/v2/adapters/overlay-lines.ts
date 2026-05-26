/**
 * overlay-lines — map precomputed candle-pane indicator line series
 * (SMA / EMA / Bollinger / Donchian) into KlineCharts custom-overlay
 * descriptors. The descriptors are consumed by KlineCandlePane, which
 * registers a single `"xvnLine"` overlay template and calls
 * `chart.createOverlay(...)` for each descriptor.
 *
 * Oscillators (rsi / macd* / atr) are NOT included here — they live in
 * uPlot subpanes, not the candle pane.
 */
import type { Chart2ThemeDefinition } from "@/theme/themes";

import type { IndicatorMap } from "../types";

export type OverlayLineKey = keyof Chart2ThemeDefinition["overlay"];

/**
 * The candle-pane line set, in render order. Each key is both an
 * `IndicatorMap` series key and a `theme.overlay.*` color key.
 */
export const OVERLAY_LINE_KEYS: OverlayLineKey[] = [
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
];

const DASHED = new Set<OverlayLineKey>([
  "ema20",
  "ema30",
  "ema50",
  "ema60",
  "ema90",
  "ema200",
]);

export type OverlayLineDescriptor = {
  name: "xvnLine";
  points: { timestamp: number; value: number }[];
  extendData: { key: OverlayLineKey; color: string; dashed: boolean };
};

function colorFor(theme: Chart2ThemeDefinition, key: OverlayLineKey): string {
  return theme.overlay[key] ?? "#888888";
}

/**
 * Build one descriptor per present (non-empty) candle-pane line series that
 * is not explicitly toggled off in `active`. A key is active when
 * `active[key] !== false` (undefined → active). Timestamps are converted from
 * unix seconds to milliseconds (KlineCharts uses ms).
 */
export function overlayLineDescriptors(
  indicators: IndicatorMap,
  theme: Chart2ThemeDefinition,
  active: Partial<Record<string, boolean>>,
): OverlayLineDescriptor[] {
  const descriptors: OverlayLineDescriptor[] = [];

  for (const key of OVERLAY_LINE_KEYS) {
    if (active[key] === false) continue;

    const series = indicators[key];
    if (!series || series.time.length === 0) continue;

    const points: { timestamp: number; value: number }[] = [];
    const n = Math.min(series.time.length, series.value.length);
    for (let i = 0; i < n; i += 1) {
      points.push({ timestamp: series.time[i] * 1000, value: series.value[i] });
    }
    if (points.length === 0) continue;

    descriptors.push({
      name: "xvnLine",
      points,
      extendData: {
        key,
        color: colorFor(theme, key),
        dashed: DASHED.has(key),
      },
    });
  }

  return descriptors;
}
