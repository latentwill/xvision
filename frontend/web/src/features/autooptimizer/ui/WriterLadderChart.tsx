/**
 * WriterLadderChart — per-writer accept rate bar chart.
 *
 * Shows accepted / proposals (pass-rate) for each experiment writer
 * as a horizontal bar chart. Each writer gets one bar so the ranking
 * is immediately readable. A legend shows writer model labels.
 */
import "uplot/dist/uPlot.min.css";

import { useRef } from "react";
import uPlot from "uplot";
import type { MutatorScore } from "../api";
import { usePlotSimple } from "./usePlotSimple";

export interface WriterLadderChartProps {
  rows: MutatorScore[];
  height?: number;
}

// Palette for up to 8 writers — cycles after that
const WRITER_COLORS = [
  "#00E676",
  "#60A5FA",
  "#F59E0B",
  "#A78BFA",
  "#F472B6",
  "#34D399",
  "#FB923C",
  "#94A3B8",
];

export function WriterLadderChart({ rows, height = 120 }: WriterLadderChartProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const hasData = rows.length > 0;

  // X axis: writer index (0-based)
  const xData = rows.map((_, i) => i);
  // Y axis: accept rate [0–1]
  const yData = rows.map((r) => (r.proposals > 0 ? r.accepted / r.proposals : 0));

  const barsPathBuilder = uPlot.paths?.bars?.({ size: [0.7, 100] });

  const opts: uPlot.Options = {
    width: 0,
    height,
    cursor: { show: true },
    legend: { show: false },
    axes: [
      {
        stroke: "var(--text-3)",
        ticks: { stroke: "var(--border)" },
        grid: { show: false },
        values: (_u, vals) =>
          vals.map((v) => {
            if (v == null) return "";
            const idx = Math.round(v);
            const row = rows[idx];
            return row ? row.model.slice(0, 14) : "";
          }),
      },
      {
        stroke: "var(--text-3)",
        ticks: { stroke: "var(--border)" },
        grid: { stroke: "var(--border)" },
        values: (_u, vals) =>
          vals.map((v) => (v == null ? "" : `${Math.round(v * 100)}%`)),
      },
    ],
    scales: {
      x: { time: false },
      y: { auto: true, range: [0, 1] },
    },
    series: [
      {},
      {
        label: "Accept rate",
        stroke: "#00E676",
        fill: "rgba(0,230,118,0.5)",
        width: 0,
        ...(barsPathBuilder ? { paths: barsPathBuilder } : {}),
        points: { show: false },
      },
    ],
  };

  usePlotSimple(opts, [xData, yData] as uPlot.AlignedData, hostRef, height, hasData);

  if (!hasData) {
    return (
      <div className="flex items-center justify-center py-6 text-[12px] text-text-3">
        No writer data yet
      </div>
    );
  }

  return (
    <div>
      {/* Legend */}
      <div className="mb-2 flex flex-wrap gap-3 text-[11px] text-text-3">
        {rows.map((r, i) => (
          <span key={`${r.provider}/${r.model}`} className="flex items-center gap-1">
            <span
              className="inline-block h-2 w-3 rounded-sm"
              style={{ background: WRITER_COLORS[i % WRITER_COLORS.length] }}
            />
            {r.model}
          </span>
        ))}
      </div>
      <div data-chart="writer-ladder" ref={hostRef} style={{ width: "100%" }} />
    </div>
  );
}
