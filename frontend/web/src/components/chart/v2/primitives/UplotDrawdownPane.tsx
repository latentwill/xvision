/**
 * UplotDrawdownPane — drawdown curve rendered as a filled area pane.
 *
 * y-axis range is pinned to [min, 0] because drawdown values are ≤ 0.
 */
import "uplot/dist/uPlot.min.css";

import React, { useRef } from "react";
import uPlot from "uplot";

import { type DrawdownPoint } from "../types";
import { columnarToUplotEquity } from "../adapters/columnar-to-uplot";
import { themeToUplotOptions } from "../adapters/theme-to-uplot";
import { useChart2Theme } from "../hooks/useChart2Theme";
import { useSyncKey } from "./PaneStack";
import { usePlot } from "./usePlot";

export interface UplotDrawdownPaneProps {
  points: DrawdownPoint[];
  height?: number;
  label?: string;
}

export function UplotDrawdownPane({
  points,
  height = 120,
  label = "Drawdown",
}: UplotDrawdownPaneProps): React.ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const syncKey = useSyncKey();

  // Reuse the equity adapter — DrawdownPoint and EquityPoint share the same
  // { time, value } shape.
  const data = columnarToUplotEquity(points);

  // Compute the y-axis floor from the data (drawdown is always ≤ 0).
  const values = points.map((p) => p.value);
  const minVal = values.length > 0 ? values.reduce((a, b) => (b < a ? b : a), 0) : -0.01;
  // Pad 5 % so the fill area is visible.
  const paddedMin = minVal === 0 ? -0.01 : minVal * 1.05;

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;
  const [xAxis = {}, yAxisBase = {}] = (baseOpts as any).axes ?? [];

  const cursorOpts: uPlot.Cursor = {
    ...(baseOpts.cursor as uPlot.Cursor | undefined),
    ...(syncKey != null
      ? { sync: { key: syncKey, setSeries: true } }
      : {}),
  };

  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0,
    height,
    cursor: cursorOpts,
    axes: [
      xAxis,
      {
        ...yAxisBase,
        size: 56,
        values: (_u: uPlot, vals: (number | null)[]) =>
          vals.map((v) => (v != null ? (v * 100).toFixed(1) + "%" : "")),
      },
    ],
    scales: {
      y: {
        // Negative-only; pin ceiling at 0.
        range: [paddedMin, 0],
      },
    },
    series: [
      {},
      {
        label,
        stroke: theme.panes.drawdown,
        fill: theme.panes.drawdownFillTop,
        width: 1.5,
        points: { show: false },
      },
    ],
  };

  usePlot(opts, data, hostRef, height);

  return <div ref={hostRef} style={{ width: "100%" }} />;
}
