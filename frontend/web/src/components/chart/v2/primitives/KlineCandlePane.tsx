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
import { v2MarkersToKlineOverlay } from "../adapters/markers";
import { overlayLineDescriptors } from "../adapters/overlay-lines";
import { useChart2Theme } from "../hooks/useChart2Theme";
import { CHART_V2_ZOOM_EVENT } from "./ChartFrame";

// ── xvnLine custom overlay ──────────────────────────────────────────────────
// A single line overlay template used to render every precomputed candle-pane
// indicator (SMA / EMA / Bollinger / Donchian). Registered once at module
// scope (KlineCharts keeps a global template registry). Color + dash come from
// the per-overlay extendData carried on each created overlay instance.
let xvnLineRegistered = false;

function registerXvnLineOverlay(): void {
  if (xvnLineRegistered) return;
  xvnLineRegistered = true;
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
}

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
  overlayActive?: Partial<Record<string, boolean>>;
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
  const theme = useChart2Theme();

  // ── Init / Destroy ─────────────────────────────────────────────────────────
  useEffect(() => {
    const el = divRef.current;
    if (!el) return;

    // Register the shared xvnLine overlay template (idempotent / module-scope).
    try {
      registerXvnLineOverlay();
    } catch (err) {
      console.warn("[KlineCandlePane] registerOverlay(xvnLine) threw:", err);
    }

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

    return () => {
      obs.disconnect();
      window.removeEventListener(CHART_V2_ZOOM_EVENT, onZoom);
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

      // TODO (later tasks): render markers + position bands.
      const _markerExtData = markers
        ? v2MarkersToKlineOverlay(markers, theme)
        : [];
      const _positionExtData = positions;

      // Suppress unused-variable lint for not-yet-wired stubs.
      void _markerExtData;
      void _positionExtData;
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
