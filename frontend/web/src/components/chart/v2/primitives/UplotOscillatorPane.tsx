/**
 * UplotOscillatorPane — RSI, MACD, or ATR oscillator pane.
 *
 * RSI: one series + horizontal guide lines drawn via a custom hooks.draw
 *      callback (uPlot has no built-in priceLine).
 * MACD: three series — macdLine, signal line, histogram bars.
 * ATR: one series.
 */
import "uplot/dist/uPlot.min.css";

import React, { useRef } from "react";
import uPlot from "uplot";

import { type LineSeries } from "../types";
import { columnarToUplotIndicator } from "../adapters/columnar-to-uplot";
import { themeToUplotOptions } from "../adapters/theme-to-uplot";
import { useChart2Theme } from "../hooks/useChart2Theme";
import { useSyncKey } from "./PaneStack";
import { usePlot } from "./usePlot";

export interface UplotOscillatorPaneProps {
  kind: "rsi" | "macd" | "atr";
  series: {
    primary: LineSeries;
    signal?: LineSeries;     // macd signal line
    histogram?: LineSeries;  // macd histogram bars
  };
  /** Horizontal guide lines, e.g. [30, 70] for RSI. */
  guides?: number[];
  height?: number;
}

/**
 * Build a uPlot plugin that draws horizontal guide lines.
 */
function guidePlugin(
  guides: number[],
  color: string,
): uPlot.Plugin {
  return {
    hooks: {
      draw: [
        (u: uPlot) => {
          const ctx = u.ctx;
          const { left, top, width, height } = u.bbox;

          ctx.save();
          ctx.strokeStyle = color;
          ctx.lineWidth = 1;
          ctx.setLineDash([4, 4]);

          for (const val of guides) {
            const yPx = u.valToPos(val, "y", true);
            if (yPx < top || yPx > top + height) continue;
            ctx.beginPath();
            ctx.moveTo(left, yPx);
            ctx.lineTo(left + width, yPx);
            ctx.stroke();
          }

          ctx.restore();
        },
      ],
    },
  };
}

export function UplotOscillatorPane({
  kind,
  series,
  guides,
  height = 100,
}: UplotOscillatorPaneProps): React.ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const syncKey = useSyncKey();

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;

  const cursorOpts: uPlot.Cursor = {
    ...(baseOpts.cursor as uPlot.Cursor | undefined),
    ...(syncKey != null
      ? { sync: { key: syncKey, setSeries: true } }
      : {}),
  };

  // ── Build data + series config depending on kind ──────────────────────────
  let data: uPlot.AlignedData;
  let uplotSeries: uPlot.Series[];
  let plugins: uPlot.Plugin[] = [];

  if (kind === "rsi") {
    data = columnarToUplotIndicator(series.primary);
    uplotSeries = [
      {},
      {
        label: "RSI",
        stroke: theme.panes.rsi,
        width: 1.5,
        points: { show: false },
      },
    ];
    if (guides && guides.length > 0) {
      plugins = [guidePlugin(guides, theme.panes.rsiGuide)];
    }
  } else if (kind === "macd") {
    const primaryData = columnarToUplotIndicator(series.primary);
    const timeAxis = primaryData[0] as number[];

    // Build aligned data: [time, macdLine, signal?, histogram?]
    const macdLine = primaryData[1] as (number | null | undefined)[];
    const signalVals = series.signal
      ? (columnarToUplotIndicator(series.signal)[1] as (number | null | undefined)[])
      : null;
    const histVals = series.histogram
      ? (columnarToUplotIndicator(series.histogram)[1] as (number | null | undefined)[])
      : null;

    const dataRows: (number | null | undefined)[][] = [timeAxis, macdLine];
    if (signalVals) dataRows.push(signalVals);
    if (histVals) dataRows.push(histVals);
    data = dataRows as uPlot.AlignedData;

    const barsPathBuilder = uPlot.paths.bars?.({ size: [0.6, 100] });

    uplotSeries = [
      {},
      {
        label: "MACD",
        stroke: theme.panes.macdLine,
        width: 1.5,
        points: { show: false },
      },
    ];
    if (signalVals) {
      uplotSeries.push({
        label: "Signal",
        stroke: theme.panes.macdSignal,
        width: 1.5,
        points: { show: false },
      });
    }
    if (histVals) {
      uplotSeries.push({
        label: "Hist",
        stroke: theme.panes.macdHist,
        fill: theme.panes.macdHist,
        width: 0,
        ...(barsPathBuilder ? { paths: barsPathBuilder } : {}),
        points: { show: false },
      });
    }
  } else {
    // ATR
    data = columnarToUplotIndicator(series.primary);
    uplotSeries = [
      {},
      {
        label: "ATR",
        stroke: theme.panes.atr,
        width: 1.5,
        points: { show: false },
      },
    ];
  }

  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0,
    height,
    cursor: cursorOpts,
    series: uplotSeries,
    plugins,
  };

  usePlot(opts, data, hostRef, height);

  return <div ref={hostRef} style={{ width: "100%" }} />;
}
