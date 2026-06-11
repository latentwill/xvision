// frontend/web/src/components/home/views/PulseHoldChart.tsx
//
// "vs Buy & Hold" Pulse view: strategy return % (gold) vs buy-and-hold
// return % (muted dashed), shared axis, zero line. Inline series labels —
// no floating legend.

import "uplot/dist/uPlot.min.css";

import { useRef, type ReactElement } from "react";
import uPlot from "uplot";

import type { RunChartPayload } from "@/api/types.gen";
import { normalizeEquityToReturnPct } from "@/components/chart/v2/adapters/columnar-to-uplot";
import { themeToUplotOptions } from "@/components/chart/v2/adapters/theme-to-uplot";
import { xvnLastDot, xvnZeroLine } from "@/components/chart/v2/adapters/uplot-plugins";
import { useChart2Theme } from "@/components/chart/v2/hooks/useChart2Theme";
import { usePlot } from "@/components/chart/v2/primitives/usePlot";
import { holdCompareSeries } from "@/features/home/pulse";

export function PulseHoldChart({
  payload,
  height = 210,
}: {
  payload: RunChartPayload;
  height?: number;
}): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const series = holdCompareSeries(
    normalizeEquityToReturnPct(payload.equity),
    payload.baseline_equity ?? [],
  );

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
        label: "Strategy",
        stroke: theme.panes.equity,
        width: 1.5,
        points: { show: false },
        spanGaps: true,
      },
      {
        label: "Buy & Hold",
        // axisText is the muted-but-legible grey the axes use — neutral (no
        // loss-red), and unlike gridStrong it stays visible on the dark
        // panel background (gridStrong vanished in the live smoke test).
        stroke: theme.surface.axisText,
        width: 1,
        dash: [4, 4],
        points: { show: false },
        spanGaps: true,
      },
    ],
    plugins: [
      xvnZeroLine(),
      xvnLastDot(1, theme.panes.equity, { backgroundFill: theme.surface.bg }),
    ],
  };

  usePlot(
    opts,
    [series.time, series.strategy, series.hold] as uPlot.AlignedData,
    hostRef,
    height,
  );

  return (
    <div data-testid="pulse-hold-chart" style={{ width: "100%" }}>
      <div ref={hostRef} style={{ width: "100%" }} />
      <div className="flex items-center gap-4 px-2 pt-1 text-[11px] text-text-4">
        <span className="text-gold">— Strategy</span>
        <span>┄ Buy &amp; Hold</span>
      </div>
    </div>
  );
}
