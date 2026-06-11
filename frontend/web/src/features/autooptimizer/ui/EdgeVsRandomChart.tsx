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

// Resolved token hex values (from styles/tokens.css — dark theme).
// uPlot draws to canvas and cannot resolve CSS custom properties, so we
// pass concrete hex values here. Update if tokens.css changes.
const TOKEN_TEXT_3 = "#9aa3b2"; // --text-3
const TOKEN_BORDER = "#2c313b"; // --border
const TOKEN_GOLD = "#00e676"; // --gold

const EXPLAINER =
  "Each cycle's best experiment edge vs a random-baseline strategy — above 0 means the optimizer beats chance.";

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
        stroke: TOKEN_TEXT_3,
        ticks: { stroke: TOKEN_BORDER },
        grid: { stroke: TOKEN_BORDER },
        values: (_u, vals) => vals.map((v) => (v == null ? "" : `#${v + 1}`)),
      },
      {
        stroke: TOKEN_TEXT_3,
        ticks: { stroke: TOKEN_BORDER },
        grid: { stroke: TOKEN_BORDER },
        values: (_u, vals) => vals.map((v) => fmt(v as number | null)),
      },
    ],
    scales: { x: { time: false }, y: { auto: true } },
    series: [
      {},
      {
        label: "Parent edge",
        stroke: TOKEN_GOLD,
        width: 2,
        points: { show: true, size: 5, fill: TOKEN_GOLD },
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
        stroke: TOKEN_TEXT_3,
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
      <div className="space-y-1 py-4">
        <p className="text-[11px] text-text-3" data-testid="edge-vs-random-explainer">
          {EXPLAINER}
        </p>
        <p className="text-[11px] text-text-4">
          Appears once cycles run with a baseline.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-1">
      <p className="text-[11px] text-text-3" data-testid="edge-vs-random-explainer">
        {EXPLAINER}
      </p>
      <div data-chart="edge-vs-random" ref={hostRef} style={{ width: "100%" }} />
    </div>
  );
}
