/**
 * UplotHistogramPane — volume or generic histogram bar pane.
 *
 * Accepts either CandleColumns (uses volume column) or a raw { time, value }
 * pair. Per-bar coloring (up/down) is M1 work; M0 uses a single fill color.
 */
import "uplot/dist/uPlot.min.css";

import React, { useRef } from "react";
import uPlot from "uplot";

import { type CandleColumns } from "../types";
import {
  columnarToUplotHistogram,
} from "../adapters/columnar-to-uplot";
import { themeToUplotOptions } from "../adapters/theme-to-uplot";
import { useChart2Theme } from "../hooks/useChart2Theme";
import { useSyncKey } from "./PaneStack";
import { usePlot } from "./usePlot";

export interface UplotHistogramPaneProps {
  /** Supply candles to extract the volume column automatically. */
  candles?: CandleColumns;
  /** Or supply raw time/value arrays directly. */
  bars?: { time: number[]; value: number[] };
  height?: number;
}

export function UplotHistogramPane({
  candles,
  bars,
  height = 80,
}: UplotHistogramPaneProps): React.ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const syncKey = useSyncKey();

  // Build AlignedData from whichever source was provided.
  let data: uPlot.AlignedData;
  if (candles) {
    data = columnarToUplotHistogram(candles);
  } else if (bars) {
    data = [bars.time, bars.value as (number | null | undefined)[]];
  } else {
    // Empty placeholder so uPlot still renders.
    data = [[], []];
  }

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;

  const cursorOpts: uPlot.Cursor = {
    ...(baseOpts.cursor as uPlot.Cursor | undefined),
    ...(syncKey != null
      ? { sync: { key: syncKey, setSeries: true } }
      : {}),
  };

  // uPlot.paths.bars may be undefined if the path-builder bundle is not
  // included. Guard with optional chaining.
  const barsPathBuilder = uPlot.paths.bars?.({ size: [0.6, 100] });

  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0,
    height,
    cursor: cursorOpts,
    series: [
      {},
      {
        label: "Volume",
        // M0: single fill color — per-bar up/down coloring is M1 work.
        fill: theme.panes.volumeUp,
        stroke: theme.panes.volumeUp,
        width: 0,
        // Use the bars path builder when available; fall back to line.
        ...(barsPathBuilder ? { paths: barsPathBuilder } : {}),
        points: { show: false },
      },
    ],
  };

  usePlot(opts, data, hostRef, height);

  return <div ref={hostRef} style={{ width: "100%" }} />;
}
