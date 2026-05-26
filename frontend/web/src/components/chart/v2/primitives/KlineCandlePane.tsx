/**
 * KlineCandlePane — KlineCharts-powered OHLCV candle pane.
 *
 * Wraps klinecharts v10-beta2. The library's `init()` function returns
 * `Nullable<Chart>` so every call to the chart instance is guarded.
 *
 * M0 wire-up: overlay/marker data is stored in extData for later wiring.
 * TODO M1: register overlays as KlineCharts indicators and markers via
 *          chart.createIndicator / chart.createOverlay.
 */
import React, { useEffect, useRef } from "react";
import { init, dispose, registerOverlay } from "klinecharts";
import type { Chart, KLineData } from "klinecharts";

import {
  type CandleColumns,
  type IndicatorMap,
  type V2Marker,
  type PositionSpan,
} from "../types";
import { columnarToKLineData } from "../adapters/columnar-to-klinedata";
import { themeToKlinechartsStyles } from "../adapters/theme-to-klinecharts";
import {
  overlayLineDescriptors,
  type OverlayLineKey,
} from "../adapters/overlay-lines";
import { useChart2Theme } from "../hooks/useChart2Theme";
import { CHART_V2_RANGE_EVENT, CHART_V2_ZOOM_EVENT } from "./ChartFrame";
import { rangeWindowSeconds } from "./range-window";

// ── xvnLine custom overlay ──────────────────────────────────────────────────
// A single line overlay template used to render every precomputed candle-pane
// indicator (SMA / EMA / Bollinger / Donchian). Registered exactly once at
// module scope — KlineCharts keeps a global template registry, so importing
// this module installs the template before any chart is created. Color + dash
// come from the per-overlay extendData carried on each created overlay instance.
//
// No try/catch here on purpose: a `registerOverlay` failure is a genuine bug
// (bad template shape, library API drift) and must surface, not be swallowed.
registerOverlay({
  name: "xvnLine",
  totalStep: 1,
  needDefaultPointFigure: false,
  needDefaultXAxisFigure: false,
  needDefaultYAxisFigure: false,
  createPointFigures: ({ coordinates, overlay }) => {
    if (coordinates.length < 2) return [];
    const ext = (overlay.extendData ?? {}) as {
      color?: string;
      dashed?: boolean;
    };
    return {
      type: "line",
      attrs: { coordinates },
      styles: {
        color: ext.color ?? "#888888",
        size: 1,
        style: ext.dashed ? "dashed" : "solid",
      },
      ignoreEvent: true,
    };
  },
});

// ── xvnMarker custom overlay ─────────────────────────────────────────────────
// One overlay template for every trade/veto/hold marker pinned to a single
// (timestamp, price) point on the candle pane. Like xvnLine it is registered
// once at module scope and styled from per-instance extendData. Shape by kind:
//   buy  → up-arrow polygon anchored below the bar
//   sell → down-arrow polygon anchored above the bar
//   veto → circle dot
//   hold → circle dot
// An optional text label is drawn beside the glyph. No try/catch on purpose
// (see xvnLine note above).
registerOverlay({
  name: "xvnMarker",
  totalStep: 1,
  needDefaultPointFigure: false,
  needDefaultXAxisFigure: false,
  needDefaultYAxisFigure: false,
  createPointFigures: ({ coordinates, overlay }) => {
    const ext = (overlay.extendData ?? {}) as {
      kind: string;
      text: string;
      color: string;
    };
    const c = coordinates?.[0];
    if (!c) return [];
    const isArrow = ext.kind === "buy" || ext.kind === "sell";
    const up = ext.kind === "buy";
    const yOff = up ? 14 : -14;
    const figs: unknown[] = [];
    if (isArrow) {
      figs.push({
        type: "polygon",
        attrs: {
          coordinates: [
            { x: c.x, y: c.y + (up ? 8 : -8) },
            { x: c.x - 5, y: c.y + yOff },
            { x: c.x + 5, y: c.y + yOff },
          ],
        },
        styles: { style: "fill", color: ext.color },
        ignoreEvent: true,
      });
    } else {
      figs.push({
        type: "circle",
        attrs: { x: c.x, y: c.y, r: 4 },
        styles: { style: "fill", color: ext.color },
        ignoreEvent: true,
      });
    }
    if (ext.text) {
      figs.push({
        type: "text",
        attrs: { x: c.x + 6, y: c.y + yOff, text: ext.text },
        styles: { color: ext.color, size: 10 },
        ignoreEvent: true,
      });
    }
    return figs as never;
  },
});

// ── xvnPositionBand custom overlay ───────────────────────────────────────────
// A shaded full-pane-height rectangle marking a held long/short position from
// `start` to `end`. The two anchor points carry an arbitrary value (0); only
// the x-coordinates and the pane `bounding.height` matter, so the band fills
// the whole candle pane vertically regardless of price. Registered once at
// module scope and tinted from per-instance extendData (a low-opacity band
// fill). No try/catch on purpose (see xvnLine note above).
registerOverlay({
  name: "xvnPositionBand",
  totalStep: 1,
  needDefaultPointFigure: false,
  needDefaultXAxisFigure: false,
  needDefaultYAxisFigure: false,
  createPointFigures: ({ coordinates, bounding, overlay }) => {
    const ext = (overlay.extendData ?? {}) as { color: string };
    if (!coordinates || coordinates.length < 2) return [];
    const x0 = coordinates[0].x;
    const x1 = coordinates[1].x;
    return [
      {
        type: "rect",
        attrs: {
          x: Math.min(x0, x1),
          y: 0,
          width: Math.abs(x1 - x0),
          height: bounding.height,
        },
        styles: { style: "fill", color: ext.color },
        ignoreEvent: true,
      },
    ] as never;
  },
});

export interface KlineCandlePaneProps {
  candles: CandleColumns;
  /**
   * Precomputed indicator line series. Candle-pane lines (SMA / EMA /
   * Bollinger / Donchian) render as `xvnLine` overlays; oscillator keys
   * (rsi / macd* / atr) are ignored here — they belong to uPlot subpanes.
   */
  overlays?: Partial<IndicatorMap>;
  markers?: V2Marker[];
  positions?: PositionSpan[];
  /**
   * Per-overlay-line on/off map keyed by IndicatorMap line key
   * (e.g. `sma20`, `ema50`). A line renders when its entry is `true` or
   * absent; `false` hides it. Defaults to all-present-lines-active.
   */
  overlayActive?: Partial<Record<OverlayLineKey, boolean>>;
  height?: number;
  /**
   * Called once with the live `Chart` instance after `init()` succeeds,
   * and once with `null` on unmount/cleanup. Consumers may use this to
   * drive pixel-precise annotation anchoring via `createKlineAnchor`.
   */
  onReady?: (chart: Chart | null) => void;
}

export function KlineCandlePane({
  candles,
  overlays,
  markers,
  positions,
  overlayActive,
  height = 380,
  onReady,
}: KlineCandlePaneProps): React.ReactElement {
  const divRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<Chart | null>(null);
  // Keep a stable ref to onReady so the init effect doesn't re-run when
  // the callback identity changes between renders.
  const onReadyRef = useRef<((chart: Chart | null) => void) | undefined>(onReady);
  useEffect(() => {
    onReadyRef.current = onReady;
  });
  // Keep a stable ref to the latest candles so the range-event listener
  // (registered once in the init effect) always reads the current series
  // without re-running init when candle data updates.
  const candlesRef = useRef(candles);
  useEffect(() => {
    candlesRef.current = candles;
  });
  const theme = useChart2Theme();

  // ── Init / Destroy ─────────────────────────────────────────────────────────
  useEffect(() => {
    const el = divRef.current;
    if (!el) return;

    // The shared xvnLine overlay template is registered once at module scope
    // (see top of file) — nothing to do here.

    let chart: Chart | null = null;
    try {
      // init() returns Nullable<Chart>; guard the null case.
      chart = init(el) ?? null;
    } catch (err) {
      console.warn("[KlineCandlePane] init() threw:", err);
      return;
    }
    if (!chart) return;
    chartRef.current = chart;
    // KlineCharts v10 only invokes DataLoader#getBars once symbol and period
    // are both configured. M0 fixtures are already normalized, so a generic
    // chart-v2 identity is enough until API-backed symbols arrive in M1.
    chart.setSymbol({ ticker: "chart-v2", pricePrecision: 4, volumePrecision: 2 });
    chart.setPeriod({ type: "minute", span: 1 });

    // Notify the consumer that the chart is ready.
    onReadyRef.current?.(chart);

    const obs = new ResizeObserver(() => {
      try {
        chartRef.current?.resize();
      } catch (err) {
        console.warn("[KlineCandlePane] resize() threw:", err);
      }
    });
    obs.observe(el);

    const onZoom = (event: Event) => {
      const detail = (event as CustomEvent<"in" | "out">).detail;
      const current = chartRef.current;
      if (!current || (detail !== "in" && detail !== "out")) return;
      const chartAny = current as unknown as {
        zoomAtCoordinate?: (
          scale: number,
          coordinate: { x: number; y: number },
          animationDuration?: number,
        ) => void;
      };
      try {
        chartAny.zoomAtCoordinate?.(
          detail === "in" ? 1.18 : 0.84,
          { x: el.clientWidth / 2, y: height / 2 },
          160,
        );
      } catch (err) {
        console.warn("[KlineCandlePane] zoomAtCoordinate threw:", err);
      }
    };
    window.addEventListener(CHART_V2_ZOOM_EVENT, onZoom);

    const onRange = (event: Event) => {
      const preset = (event as CustomEvent).detail;
      const ch = chartRef.current;
      if (!ch) return;
      const t = candlesRef.current.time;
      if (t.length < 2) return;
      const win = rangeWindowSeconds(preset);
      const chAny = ch as unknown as {
        setBarSpace?: (n: number) => void;
        scrollToRealTime?: (ms?: number) => void;
        getDom?: (paneId?: string) => HTMLElement | null;
      };
      const dom = chAny.getDom?.();
      const width = dom?.clientWidth ?? 600;
      try {
        if (win == null) {
          chAny.setBarSpace?.(Math.max(1, width / t.length));
          chAny.scrollToRealTime?.();
          return;
        }
        const intervalSec = Math.max(1, t[t.length - 1] - t[t.length - 2]);
        const count = Math.max(1, Math.ceil(win / intervalSec));
        chAny.setBarSpace?.(Math.max(1, width / count));
        chAny.scrollToRealTime?.();
      } catch (err) {
        console.warn("[KlineCandlePane] range apply threw:", err);
      }
    };
    window.addEventListener(CHART_V2_RANGE_EVENT, onRange);

    return () => {
      obs.disconnect();
      window.removeEventListener(CHART_V2_ZOOM_EVENT, onZoom);
      window.removeEventListener(CHART_V2_RANGE_EVENT, onRange);
      // Notify the consumer that the chart is being destroyed.
      onReadyRef.current?.(null);
      try {
        dispose(el);
      } catch (err) {
        console.warn("[KlineCandlePane] dispose() threw:", err);
      }
      chartRef.current = null;
    };
  // Re-init when candle column identity changes (topology change).
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Data ───────────────────────────────────────────────────────────────────
  useEffect(() => {
    const chart = chartRef.current;
    if (!chart) return;
    try {
      // klinecharts v10-beta2 uses a DataLoader pattern — there is no
      // applyNewData(). We provide a one-shot DataLoader that returns the
      // pre-computed bars and signals no more data (more = false).
      const klineData: KLineData[] = columnarToKLineData(candles);
      chart.setDataLoader({
        getBars: ({ callback }) => {
          callback(klineData, false);
        },
      });

      // Render precomputed candle-pane indicator lines as xvnLine overlays.
      const descriptors = overlayLineDescriptors(
        overlays ?? {},
        theme,
        overlayActive ?? {},
      );
      for (const d of descriptors) {
        chart.createOverlay({
          name: d.name,
          points: d.points,
          extendData: d.extendData,
        });
      }

      // Render trade/veto/hold markers as xvnMarker overlays, one per priced
      // marker. Markers without a price have no candle-pane anchor, so skip
      // them (they still surface in the MarkerDock list).
      for (const m of markers ?? []) {
        if (m.price == null) continue;
        chart.createOverlay({
          name: "xvnMarker",
          points: [{ timestamp: m.time * 1000, value: m.price }],
          extendData: { kind: m.kind, text: m.text ?? "", color: theme.marker[m.kind] },
        });
      }

      // Render held long/short position spans as full-height xvnPositionBand
      // overlays, one per span. The band is tinted with the theme's
      // low-opacity position fill so it shades the pane without overwhelming
      // the candles underneath.
      for (const p of positions ?? []) {
        chart.createOverlay({
          name: "xvnPositionBand",
          points: [
            { timestamp: p.start * 1000, value: 0 },
            { timestamp: p.end * 1000, value: 0 },
          ],
          extendData: {
            color:
              p.side === "long"
                ? theme.position.longBand
                : theme.position.shortBand,
          },
        });
      }
    } catch (err) {
      console.warn("[KlineCandlePane] applyNewData threw:", err);
    }
  }, [candles, overlays, markers, positions, overlayActive, theme]);

  // ── Theme ──────────────────────────────────────────────────────────────────
  useEffect(() => {
    const chart = chartRef.current;
    if (!chart) return;
    try {
      chart.setStyles(themeToKlinechartsStyles(theme));
    } catch (err) {
      console.warn("[KlineCandlePane] setStyles threw:", err);
    }
  }, [theme]);

  return (
    <div
      ref={divRef}
      style={{ width: "100%", height: `${height}px` }}
    />
  );
}
