/**
 * UplotEquityPane — return % curve rendered as a filled area line pane.
 *
 * uPlot CSS loaded here; Vite deduplicates the import across all panes.
 */
import "uplot/dist/uPlot.min.css";

import React, { useRef } from "react";
import uPlot from "uplot";

import { type EquityPoint } from "../types";
import { columnarToUplotEquity } from "../adapters/columnar-to-uplot";
import {
  buildReturnFillGradient,
  xvnZeroLine,
} from "../adapters/uplot-plugins";
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
  label = "Return %",
}: UplotEquityPaneProps): React.ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const syncKey = useSyncKey();

  const data = columnarToUplotEquity(points);
  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;
  const baseAxes = (baseOpts.axes as uPlot.Axis[] | undefined) ?? [];

  const finalValue =
    points.length > 0 ? (points[points.length - 1].value ?? 0) : 0;
  const strokeColor = finalValue >= 0 ? "#00E676" : "#EF4444";

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
      baseAxes[0] ?? {},
      {
        ...baseAxes[1],
        values: (_u: uPlot, vals: number[]) =>
          vals.map((v) => (v != null ? v.toFixed(1) + "%" : "")),
      },
    ],
    series: [
      {},
      {
        label,
        stroke: strokeColor,
        fill: buildReturnFillGradient,
        width: 1.5,
        points: { show: false },
      },
    ],
    plugins: [xvnZeroLine()],
  };

  usePlot(opts, data, hostRef, height);

  return <div ref={hostRef} style={{ width: "100%" }} />;
}
