// frontend/web/src/components/home/PulseEquityChart.tsx
//
// Hero equity chart for the home Pulse band: return-% area line with the
// client-side drawdown band (running max − equity) rendered as a subdued
// red-tinted area below zero with NO stroke (the xvnAreaFill plugin paints
// the band; a visible stroke would read as a duplicate earnings line).
// All gradient/path construction goes through the F8-guarded plugins, so
// empty/non-finite data can never throw `createLinearGradient: non-finite`.

import "uplot/dist/uPlot.min.css";

import { useRef, type ReactElement } from "react";
import uPlot from "uplot";

import {
  buildReturnFillGradient,
  xvnAreaFill,
  xvnLastDot,
  xvnZeroLine,
} from "@/components/chart/v2/adapters/uplot-plugins";
import { themeToUplotOptions } from "@/components/chart/v2/adapters/theme-to-uplot";
import { useChart2Theme } from "@/components/chart/v2/hooks/useChart2Theme";
import { usePlot } from "@/components/chart/v2/primitives/usePlot";
import type { PulseChartSeries } from "@/features/home/pulse";

export interface PulseEquityChartProps {
  series: PulseChartSeries;
  height?: number;
}

export function PulseEquityChart({
  series,
  height = 210,
}: PulseEquityChartProps): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();

  const lastEquity = [...series.equity]
    .reverse()
    .find((v): v is number => v !== null && Number.isFinite(v));
  const equityStroke =
    lastEquity !== undefined && lastEquity < 0
      ? theme.panes.drawdown
      : theme.panes.equity;

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;
  const baseAxes = (baseOpts.axes as uPlot.Axis[] | undefined) ?? [];

  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0,
    height,
    legend: { show: false },
    axes: [
      baseAxes[0] ?? {},
      {
        ...baseAxes[1],
        size: 52,
        values: (_u: uPlot, vals: (number | null)[]) =>
          vals.map((v) => (v != null ? `${v.toFixed(2)}%` : "")),
      },
    ],
    series: [
      {},
      {
        label: "Return %",
        stroke: equityStroke,
        fill: buildReturnFillGradient,
        width: 1.5,
        points: { show: false },
      },
      {
        label: "Drawdown",
        // Band only — the xvnAreaFill plugin paints the underwater tint;
        // a visible stroke here reads as a duplicate earnings line.
        stroke: "transparent",
        width: 0,
        points: { show: false },
      },
    ],
    plugins: [
      xvnAreaFill(2, "rgba(255,77,77,0.16)"),
      xvnZeroLine(),
      xvnLastDot(1, equityStroke, { backgroundFill: theme.surface.bg }),
    ],
  };

  const data = [series.time, series.equity, series.drawdown] as uPlot.AlignedData;
  usePlot(opts, data, hostRef, height);

  return (
    <div
      ref={hostRef}
      data-testid="pulse-equity-chart"
      style={{ width: "100%" }}
    />
  );
}
