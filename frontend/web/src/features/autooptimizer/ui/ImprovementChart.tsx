/**
 * ImprovementChart — best Δ untouched-period score per optimizer cycle.
 *
 * Line chart using uPlot, styled to match eval-runs chart conventions.
 * X axis: cycle index (integer). Y axis: best_delta_holdout value.
 */
import "uplot/dist/uPlot.min.css";

import { useRef } from "react";
import uPlot from "uplot";
import type { StatsRow } from "../api";
import { usePlotSimple } from "./usePlotSimple";

export interface ImprovementChartProps {
  rows: StatsRow[];
  sessionId?: string;
  height?: number;
}

export function ImprovementChart({
  rows,
  height = 140,
}: ImprovementChartProps) {
  const hostRef = useRef<HTMLDivElement>(null);

  // Filter to rows that have a meaningful delta value
  const validRows = rows.filter((r) => r.best_delta_holdout != null);

  const hasData = validRows.length > 0;

  const xData = validRows.map((_, i) => i);
  const yData = validRows.map((r) => r.best_delta_holdout as number);

  const opts: uPlot.Options = {
    width: 0,
    height,
    cursor: { show: true },
    legend: { show: false },
    axes: [
      {
        stroke: "var(--text-3)",
        ticks: { stroke: "var(--border)" },
        grid: { stroke: "var(--border)" },
        values: (_u, vals) => vals.map((v) => (v == null ? "" : `#${v + 1}`)),
      },
      {
        stroke: "var(--text-3)",
        ticks: { stroke: "var(--border)" },
        grid: { stroke: "var(--border)" },
        values: (_u, vals) =>
          vals.map((v) =>
            v == null ? "" : (v >= 0 ? "+" : "") + v.toFixed(3),
          ),
      },
    ],
    scales: {
      x: { time: false },
      y: { auto: true },
    },
    series: [
      {},
      {
        label: "Best Δ score",
        stroke: "var(--gold)",
        width: 2,
        points: { show: true, size: 5, fill: "var(--gold)" },
        fill: "rgba(0,230,118,0.08)",
      },
    ],
  };

  usePlotSimple(opts, [xData, yData] as uPlot.AlignedData, hostRef, height, hasData);

  if (!hasData) {
    return (
      <div className="flex items-center justify-center py-8 text-[12px] text-text-3">
        Start an Optimizer run to see improvement over time
      </div>
    );
  }

  return <div data-chart="improvement" ref={hostRef} style={{ width: "100%" }} />;
}
