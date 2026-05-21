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
import { init, dispose } from "klinecharts";
import type { Chart, KLineData } from "klinecharts";

import {
  type CandleColumns,
  type LineSeries,
  type V2Marker,
  type PositionSpan,
} from "../types";
import { columnarToKLineData } from "../adapters/columnar-to-klinedata";
import { themeToKlinechartsStyles } from "../adapters/theme-to-klinecharts";
import { v2MarkersToKlineOverlay } from "../adapters/markers";
import { useChart2Theme } from "../hooks/useChart2Theme";

export interface KlineCandlePaneProps {
  candles: CandleColumns;
  overlays?: {
    sma20?: LineSeries;
    sma50?: LineSeries;
    sma200?: LineSeries;
    ema20?: LineSeries;
    ema50?: LineSeries;
    bollUpper?: LineSeries;
    bollMiddle?: LineSeries;
    bollLower?: LineSeries;
    donchianUpper?: LineSeries;
    donchianLower?: LineSeries;
  };
  markers?: V2Marker[];
  positions?: PositionSpan[];
  height?: number;
}

export function KlineCandlePane({
  candles,
  overlays,
  markers,
  positions,
  height = 380,
}: KlineCandlePaneProps): React.ReactElement {
  const divRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<Chart | null>(null);
  const theme = useChart2Theme();

  // ── Init / Destroy ─────────────────────────────────────────────────────────
  useEffect(() => {
    const el = divRef.current;
    if (!el) return;

    let chart: Chart | null = null;
    try {
      // init() returns Nullable<Chart>; guard the null case.
      chart = init(el) ?? null;
    } catch (err) {
      console.warn("[KlineCandlePane] init() threw:", err);
      return;
    }
    chartRef.current = chart;

    const obs = new ResizeObserver(() => {
      try {
        chartRef.current?.resize();
      } catch (err) {
        console.warn("[KlineCandlePane] resize() threw:", err);
      }
    });
    obs.observe(el);

    return () => {
      obs.disconnect();
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

      // M0: store overlay/marker data as extData for later wiring.
      // TODO M1: register overlays via chart.createIndicator / chart.createOverlay.
      const _overlayExtData = overlays;
      const _markerExtData = markers
        ? v2MarkersToKlineOverlay(markers, theme)
        : [];
      const _positionExtData = positions;

      // Suppress unused-variable lint for M0 stubs.
      void _overlayExtData;
      void _markerExtData;
      void _positionExtData;
    } catch (err) {
      console.warn("[KlineCandlePane] applyNewData threw:", err);
    }
  }, [candles, overlays, markers, positions, theme]);

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
