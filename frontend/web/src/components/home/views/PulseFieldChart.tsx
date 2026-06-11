// frontend/web/src/components/home/views/PulseFieldChart.tsx
//
// "All runs" field view: every recent completed run as a faint return-%
// line over its own elapsed fraction (0..1), hero run highlighted. Run
// identification is inline (caption row), never a popup; the x-axis is
// unlabeled because elapsed fraction is not wall-clock time.
//
// Non-hero series use an rgba-encoded stroke instead of a per-series `alpha`
// field (uPlot typings don't expose `alpha`). They reuse the equity hue at
// 28% opacity — a "field" of faint traces in the same family as the hero
// line. gridStrong was tried first but is a near-background grid tone and
// disappears entirely in dark mode (verified in the live smoke test).

import "uplot/dist/uPlot.min.css";

import { useRef, useState, type ReactElement } from "react";
import uPlot from "uplot";

import { themeToUplotOptions } from "@/components/chart/v2/adapters/theme-to-uplot";
import { xvnZeroLine } from "@/components/chart/v2/adapters/uplot-plugins";
import { useChart2Theme } from "@/components/chart/v2/hooks/useChart2Theme";
import { usePlot } from "@/components/chart/v2/primitives/usePlot";
import {
  alignFieldSeries,
  fieldRunSeries,
  type FieldRunSeries,
} from "@/features/home/pulse";

export interface PulseFieldRun {
  runId: string;
  label: string;
  equity: { time: number; equity_usd: number }[];
}

/**
 * Convert a CSS hex or named color to rgba with the given alpha. Only
 * handles the 6-digit hex format (#RRGGBB) that all theme tokens use.
 * Falls back to rgba(128,128,128,alpha) for unexpected formats.
 */
function withAlpha(hexColor: string, alpha: number): string {
  const m = /^#([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})$/.exec(hexColor);
  if (!m) return `rgba(128,128,128,${alpha})`;
  return `rgba(${parseInt(m[1], 16)},${parseInt(m[2], 16)},${parseInt(m[3], 16)},${alpha})`;
}

export function PulseFieldChart({
  runs,
  heroRunId,
  height = 210,
}: {
  runs: PulseFieldRun[];
  heroRunId: string | null;
  height?: number;
}): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const [focusLabel, setFocusLabel] = useState<string | null>(null);

  const normalized: FieldRunSeries[] = runs
    .map((r) => fieldRunSeries(r.runId, r.label, r.equity))
    .filter((s): s is FieldRunSeries => s !== null);
  const heroLabel =
    normalized.find((s) => s.runId === heroRunId)?.label ??
    normalized[0]?.label ??
    "";
  const { x, ys } = alignFieldSeries(normalized);

  // Non-hero series: equity hue at 28% opacity — visible on the carbon
  // background but clearly secondary to the full-strength hero line.
  const mutedStroke = withAlpha(theme.panes.equity, 0.28);

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;
  const baseAxes = (baseOpts.axes as uPlot.Axis[] | undefined) ?? [];
  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0,
    height,
    legend: { show: false },
    cursor: { focus: { prox: 16 } },
    scales: { x: { time: false } },
    axes: [
      { ...baseAxes[0], show: false },
      {
        ...baseAxes[1],
        size: 52,
        values: (_u: uPlot, vals: (number | null)[]) =>
          vals.map((v) => (v != null ? `${v.toFixed(1)}%` : "")),
      },
    ],
    series: [
      {},
      ...normalized.map((s): uPlot.Series => {
        const isHero = s.runId === heroRunId;
        return {
          label: s.label,
          stroke: isHero ? theme.panes.equity : mutedStroke,
          width: isHero ? 1.8 : 1,
          points: { show: false },
          spanGaps: true,
        };
      }),
    ],
    plugins: [xvnZeroLine()],
    hooks: {
      setSeries: [
        (u: uPlot, idx: number | null) => {
          setFocusLabel(
            idx != null && idx > 0 ? (u.series[idx]?.label as string) : null,
          );
        },
      ],
    },
  };

  usePlot(opts, [x, ...ys] as uPlot.AlignedData, hostRef, height);

  return (
    <div data-testid="pulse-field-chart" style={{ width: "100%" }}>
      <div ref={hostRef} style={{ width: "100%" }} />
      <div
        data-testid="pulse-field-caption"
        className="flex items-center gap-3 px-2 pt-1 text-[11px] text-text-4"
      >
        <span className="text-gold">● {heroLabel} (latest)</span>
        <span>{normalized.length} runs · x = elapsed fraction of each run</span>
        {focusLabel && focusLabel !== heroLabel ? (
          <span className="text-text-3">hover: {focusLabel}</span>
        ) : null}
      </div>
    </div>
  );
}
