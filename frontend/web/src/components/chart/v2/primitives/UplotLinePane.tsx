/**
 * UplotLinePane — generic multi-line pane.
 *
 * Each series can carry an optional color override and a dashed flag.
 * If no color is provided the series falls back to the first compare palette
 * entry at its index.
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

export interface UplotLinePaneSeriesSpec {
  label: string;
  data: LineSeries;
  color?: string;
  dashed?: boolean;
}

export interface UplotLinePaneProps {
  series: UplotLinePaneSeriesSpec[];
  height?: number;
  yLabel?: string;
}

export function UplotLinePane({
  series,
  height = 120,
  yLabel,
}: UplotLinePaneProps): React.ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const syncKey = useSyncKey();

  // Build aligned data from the first series' time axis (all series are
  // assumed to share the same time grid for M0).
  let alignedData: uPlot.AlignedData;
  if (series.length === 0) {
    alignedData = [[], []];
  } else {
    const firstPair = columnarToUplotIndicator(series[0].data);
    const timeAxis = firstPair[0] as number[];
    const valueCols: (number | null | undefined)[][] = [
      firstPair[1] as (number | null | undefined)[],
    ];
    for (let i = 1; i < series.length; i++) {
      const pair = columnarToUplotIndicator(series[i].data);
      valueCols.push(pair[1] as (number | null | undefined)[]);
    }
    alignedData = [timeAxis, ...valueCols] as uPlot.AlignedData;
  }

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;

  const cursorOpts: uPlot.Cursor = {
    ...(baseOpts.cursor as uPlot.Cursor | undefined),
    ...(syncKey != null
      ? { sync: { key: syncKey, setSeries: true } }
      : {}),
  };

  // Build uPlot series descriptors.
  const uplotSeries: uPlot.Series[] = [
    // x-axis time series (required empty placeholder).
    {},
    ...series.map((s, idx) => {
      const color =
        s.color ?? theme.compare.palette[idx % theme.compare.palette.length];
      const seriesOpts: uPlot.Series = {
        label: s.label,
        stroke: color,
        width: 1.5,
        points: { show: false },
      };
      if (s.dashed) {
        seriesOpts.dash = [6, 4];
      }
      return seriesOpts;
    }),
  ];

  // Build axes — optionally include a y-axis label.
  const baseAxes = (baseOpts.axes ?? []) as uPlot.Axis[];
  const axes: uPlot.Axis[] = baseAxes.map((ax, i) =>
    i === 1 && yLabel ? { ...ax, label: yLabel } : ax,
  );

  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0,
    height,
    cursor: cursorOpts,
    axes,
    series: uplotSeries,
  };

  usePlot(opts, alignedData, hostRef, height);

  return <div ref={hostRef} style={{ width: "100%" }} />;
}
