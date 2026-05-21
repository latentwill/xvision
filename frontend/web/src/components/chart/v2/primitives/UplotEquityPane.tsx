/**
 * UplotEquityPane — equity curve rendered as a filled area line pane.
 *
 * uPlot CSS loaded here; Vite deduplicates the import across all panes.
 */
import "uplot/dist/uPlot.min.css";

import React, { useRef } from "react";
import uPlot from "uplot";

import { type EquityPoint } from "../types";
import { columnarToUplotEquity } from "../adapters/columnar-to-uplot";
import { themeToUplotOptions } from "../adapters/theme-to-uplot";
import { useChart2Theme } from "../hooks/useChart2Theme";
import { useSyncKey } from "./PaneStack";
import { usePlot } from "./usePlot";

export interface UplotEquityPaneProps {
  points: EquityPoint[];
  height?: number;
  label?: string;
}

export function UplotEquityPane({
  points,
  height = 120,
  label = "Equity",
}: UplotEquityPaneProps): React.ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const syncKey = useSyncKey();

  const data = columnarToUplotEquity(points);

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;

  const cursorOpts: uPlot.Cursor = {
    ...(baseOpts.cursor as uPlot.Cursor | undefined),
    ...(syncKey != null
      ? { sync: { key: syncKey, setSeries: true } }
      : {}),
  };

  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0, // set by usePlot / ResizeObserver
    height,
    cursor: cursorOpts,
    series: [
      {},
      {
        label,
        stroke: theme.panes.equity,
        fill: theme.panes.equityFillTop,
        width: 1.5,
        points: { show: false },
      },
    ],
  };

  usePlot(opts, data, hostRef, height);

  return <div ref={hostRef} style={{ width: "100%" }} />;
}
