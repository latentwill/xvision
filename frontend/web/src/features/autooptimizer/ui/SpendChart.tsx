/**
 * SpendChart — cumulative cost-USD over optimizer cycles.
 *
 * Line chart showing cum_cost_usd per cycle.
 * Optional budgetCap adds a horizontal dashed reference line.
 */
import "uplot/dist/uPlot.min.css";

import { useRef } from "react";
import uPlot from "uplot";
import type { StatsRow } from "../api";
import { usePlotSimple } from "./usePlotSimple";

export interface SpendChartProps {
  rows: StatsRow[];
  budgetCap?: number;
  height?: number;
}

function buildGuidePlugin(cap: number): uPlot.Plugin {
  return {
    hooks: {
      draw: [
        (u: uPlot) => {
          const ctx = u.ctx;
          const { left, width } = u.bbox;
          const yPx = u.valToPos(cap, "y", true);
          if (!isFinite(yPx)) return;
          ctx.save();
          ctx.strokeStyle = "rgba(239,68,68,0.7)";
          ctx.lineWidth = 1.5;
          ctx.setLineDash([6, 4]);
          ctx.beginPath();
          ctx.moveTo(left, yPx);
          ctx.lineTo(left + width, yPx);
          ctx.stroke();
          ctx.restore();
        },
      ],
    },
  };
}

export function SpendChart({ rows, budgetCap, height = 120 }: SpendChartProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const hasData = rows.length > 0;

  const xData = rows.map((_, i) => i);
  const yData = rows.map((r) => r.cum_cost_usd);

  const plugins: uPlot.Plugin[] = budgetCap != null ? [buildGuidePlugin(budgetCap)] : [];

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
          vals.map((v) => (v == null ? "" : `$${v.toFixed(2)}`)),
      },
    ],
    scales: {
      x: { time: false },
      y: { auto: true },
    },
    series: [
      {},
      {
        label: "Cumulative cost",
        stroke: "#60A5FA",
        width: 2,
        fill: "rgba(96,165,250,0.1)",
        points: { show: false },
      },
    ],
    plugins,
  };

  usePlotSimple(opts, [xData, yData] as uPlot.AlignedData, hostRef, height, hasData);

  if (!hasData) {
    return (
      <div className="flex items-center justify-center py-6 text-[12px] text-text-3">
        No cost data yet
      </div>
    );
  }

  return (
    <div>
      {budgetCap != null && (
        <div className="mb-1 flex items-center gap-2 text-[11px] text-text-3">
          <span
            className="inline-block h-0 w-5 border-b border-dashed border-danger/60"
            aria-hidden="true"
          />
          <span>Budget cap ${budgetCap.toFixed(2)}</span>
        </div>
      )}
      <div data-chart="spend" ref={hostRef} style={{ width: "100%" }} />
    </div>
  );
}
