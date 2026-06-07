/**
 * OutcomeStackedChart — per-cycle outcome mix (kept / suspect / dropped).
 *
 * Stacked bar chart using uPlot. Each cycle is one tick on the X axis.
 * The stacking is done by accumulating values: the Y values for each
 * series represent the running total (uPlot paints fill between series).
 */
import "uplot/dist/uPlot.min.css";

import { useRef } from "react";
import uPlot from "uplot";
import type { StatsRow } from "../api";
import { usePlotSimple } from "./usePlotSimple";

export interface OutcomeStackedChartProps {
  rows: StatsRow[];
  height?: number;
}

// Stacked bars: uPlot doesn't natively stack, so we pre-accumulate.
function buildStackedData(rows: StatsRow[]): uPlot.AlignedData {
  const x = rows.map((_, i) => i);
  // Layer 1: kept (bottom)
  const kept = rows.map((r) => r.kept);
  // Layer 2: kept + suspect (middle band)
  const keptSuspect = rows.map((r) => r.kept + r.suspect);
  // Layer 3: kept + suspect + dropped (top)
  const total = rows.map((r) => r.kept + r.suspect + r.dropped);
  return [x, kept, keptSuspect, total] as uPlot.AlignedData;
}

export function OutcomeStackedChart({
  rows,
  height = 120,
}: OutcomeStackedChartProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const hasData = rows.length > 0;

  const data = hasData ? buildStackedData(rows) : [[], [], [], []] as uPlot.AlignedData;

  const barsPathBuilder = uPlot.paths?.bars?.({ size: [0.8, 100] });

  const seriesBase: Omit<uPlot.Series, "label" | "stroke" | "fill"> = {
    width: 0,
    points: { show: false },
    ...(barsPathBuilder ? { paths: barsPathBuilder } : {}),
  };

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
      },
    ],
    scales: {
      x: { time: false },
      y: { auto: true },
    },
    series: [
      {},
      {
        ...seriesBase,
        label: "Kept",
        stroke: "#00E676",
        fill: "rgba(0,230,118,0.55)",
      },
      {
        ...seriesBase,
        label: "Suspect",
        stroke: "#F59E0B",
        fill: "rgba(245,158,11,0.55)",
      },
      {
        ...seriesBase,
        label: "Dropped",
        stroke: "#EF4444",
        fill: "rgba(239,68,68,0.55)",
      },
    ],
  };

  usePlotSimple(opts, data, hostRef, height, hasData);

  return (
    <div data-chart="outcome-stacked">
      {!hasData ? (
        <div className="flex items-center justify-center py-6 text-[12px] text-text-3">
          No cycles yet
        </div>
      ) : (
        <>
          {/* Legend */}
          <div className="mb-2 flex items-center gap-4 text-[11px] text-text-3">
            <span className="flex items-center gap-1">
              <span className="inline-block h-2 w-3 rounded-sm" style={{ background: "rgba(0,230,118,0.7)" }} />
              Kept
            </span>
            <span className="flex items-center gap-1">
              <span className="inline-block h-2 w-3 rounded-sm" style={{ background: "rgba(245,158,11,0.7)" }} />
              Suspect
            </span>
            <span className="flex items-center gap-1">
              <span className="inline-block h-2 w-3 rounded-sm" style={{ background: "rgba(239,68,68,0.7)" }} />
              Dropped
            </span>
          </div>
          <div ref={hostRef} style={{ width: "100%" }} />
        </>
      )}
    </div>
  );
}
