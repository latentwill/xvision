/**
 * UplotDrawdownPane - drawdown curve rendered as a red underwater area
 * hanging from the zero line, with a depth gradient (strong at the surface,
 * fading at max depth) and a dashed zero baseline.
 *
 * Sign convention: the pane plots UNDERWATER (≤ 0) values, but accepts
 * either convention - the chart-v2 fixtures store drawdown as <= 0 while the
 * server's `RunChartPayload.drawdown[].drawdown_pct` is a positive depth.
 * `toUnderwaterDrawdown` normalizes both to <= 0 so the y-range pin to
 * [min, 0] always brackets the data (positive input used to land above the
 * ceiling and render as a flat line).
 */
import "uplot/dist/uPlot.min.css";

import React, { useRef } from "react";
import uPlot from "uplot";

import { type DrawdownPoint } from "../types";
import { columnarToUplotEquity } from "../adapters/columnar-to-uplot";
import {
  buildDrawdownFillGradient,
  xvnZeroLine,
} from "../adapters/uplot-plugins";
import { themeToUplotOptions } from "../adapters/theme-to-uplot";
import { useChart2Theme } from "../hooks/useChart2Theme";
import { useSyncKey } from "./PaneStack";
import { usePlot } from "./usePlot";

/** Normalize drawdown samples to the underwater (<= 0) convention. */
export function toUnderwaterDrawdown(points: DrawdownPoint[]): DrawdownPoint[] {
  return points.map((p) =>
    p.value > 0 ? { time: p.time, value: -p.value } : p,
  );
}

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

  const underwater = toUnderwaterDrawdown(points);

  // Reuse the equity adapter — DrawdownPoint and EquityPoint share the same
  // { time, value } shape.
  const data = columnarToUplotEquity(underwater);

  // Compute the y-axis floor from the data (underwater values are <= 0).
  const values = underwater.map((p) => p.value);
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
          vals.map((v) => (v != null ? v.toFixed(1) + "%" : "")),
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
        fill: buildDrawdownFillGradient,
        width: 1.5,
        points: { show: false },
      },
    ],
    plugins: [xvnZeroLine()],
  };

  usePlot(opts, data, hostRef, height);

  return <div ref={hostRef} style={{ width: "100%" }} />;
}
