// frontend/web/src/components/home/views/PulseDrawdownChart.tsx
//
// Dedicated underwater view: drawdown depth (≤ 0) as a red-tinted area.
// Same client-side computation the hero band uses (pulseChartSeries).

import "uplot/dist/uPlot.min.css";

import { useRef, type ReactElement } from "react";
import uPlot from "uplot";

import type { RunChartPayload } from "@/api/types.gen";
import { normalizeEquityToReturnPct } from "@/components/chart/v2/adapters/columnar-to-uplot";
import { themeToUplotOptions } from "@/components/chart/v2/adapters/theme-to-uplot";
import { xvnAreaFill, xvnZeroLine } from "@/components/chart/v2/adapters/uplot-plugins";
import { useChart2Theme } from "@/components/chart/v2/hooks/useChart2Theme";
import { usePlot } from "@/components/chart/v2/primitives/usePlot";
import { pulseChartSeries } from "@/features/home/pulse";

export function PulseDrawdownChart({
  payload,
  height = 210,
}: {
  payload: RunChartPayload;
  height?: number;
}): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const series = pulseChartSeries(normalizeEquityToReturnPct(payload.equity));

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
        label: "Drawdown",
        stroke: theme.panes.drawdown,
        width: 1.5,
        points: { show: false },
      },
    ],
    plugins: [xvnAreaFill(1, "rgba(255,77,77,0.16)"), xvnZeroLine()],
  };

  usePlot(
    opts,
    [series.time, series.drawdown] as uPlot.AlignedData,
    hostRef,
    height,
  );

  return (
    <div ref={hostRef} data-testid="pulse-drawdown-chart" style={{ width: "100%" }} />
  );
}
