/**
 * EdgeVsRandomChart — edge over a fixed-seed random baseline per optimizer cycle.
 *
 * Two uPlot lines + a dashed zero reference (the random/noise floor):
 *   - Parent edge   (parent_score − random)  → lineage health across generations.
 *     Trending toward 0 = the accepted lineage is decaying toward noise even if
 *     it still beats its immediate predecessor.
 *   - Candidate edge (best child_score − random) → the best candidate's edge that
 *     cycle.
 * Above the dashed 0 line = beating a no-intelligence random agent.
 * X axis: cycle index. Y axis: edge (oriented objective units, e.g. Sharpe).
 */
import "uplot/dist/uPlot.min.css";

import { useRef } from "react";
import uPlot from "uplot";
import type { StatsRow } from "../api";
import { usePlotSimple } from "./usePlotSimple";

export interface EdgeVsRandomChartProps {
  rows: StatsRow[];
  height?: number;
}

export function EdgeVsRandomChart({ rows, height = 140 }: EdgeVsRandomChartProps) {
  const hostRef = useRef<HTMLDivElement>(null);

  // Keep cycles that carry at least one edge value (pre-061 cycles are null).
  const validRows = rows.filter(
    (r) => r.best_parent_edge != null || r.best_edge_over_random != null,
  );
  const hasData = validRows.length > 0;

  const xData = validRows.map((_, i) => i);
  const parentEdge = validRows.map((r) =>
    r.best_parent_edge == null ? null : r.best_parent_edge,
  );
  const candidateEdge = validRows.map((r) =>
    r.best_edge_over_random == null ? null : r.best_edge_over_random,
  );
  const zeroLine = validRows.map(() => 0);

  const fmt = (v: number | null) =>
    v == null ? "" : (v >= 0 ? "+" : "") + v.toFixed(3);

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
        values: (_u, vals) => vals.map((v) => fmt(v as number | null)),
      },
    ],
    scales: { x: { time: false }, y: { auto: true } },
    series: [
      {},
      {
        label: "Parent edge",
        stroke: "var(--gold)",
        width: 2,
        points: { show: true, size: 5, fill: "var(--gold)" },
      },
      {
        label: "Candidate edge",
        stroke: "#60a5fa",
        width: 2,
        points: { show: true, size: 4, fill: "#60a5fa" },
      },
      {
        // Random/noise floor.
        label: "Random",
        stroke: "var(--text-3)",
        width: 1,
        dash: [4, 4],
        points: { show: false },
      },
    ],
  };

  usePlotSimple(
    opts,
    [xData, parentEdge, candidateEdge, zeroLine] as uPlot.AlignedData,
    hostRef,
    height,
    hasData,
  );

  if (!hasData) {
    return (
      <div className="flex items-center justify-center py-8 text-[12px] text-text-3">
        Edge vs random appears once cycles run with a baseline
      </div>
    );
  }

  return <div data-chart="edge-vs-random" ref={hostRef} style={{ width: "100%" }} />;
}
