/**
 * MiniSparkline — micro uPlot pane: a single series, no axes, no
 * cursor, no legend. Used inside `StrategyCard` (B2) to summarise a
 * strategy's equity (price-like line) or its drawdown (negative area)
 * in a card-grid cell.
 *
 * `variant="price"` strokes in the strategy's accent color with a
 * faint area fill; `variant="drawdown"` strokes in danger-red with a
 * red area fill. Both use the `xvnAreaFill` plugin from the
 * uplot-plugins port.
 */
import "uplot/dist/uPlot.min.css";

import { useRef, type ReactElement } from "react";
import uPlot from "uplot";

import { themeToUplotOptions } from "../adapters/theme-to-uplot";
import { xvnAreaFill } from "../adapters/uplot-plugins";
import { useChart2Theme } from "../hooks/useChart2Theme";
import { usePlot } from "./usePlot";

export type MiniSparklineVariant = "price" | "drawdown";

export interface MiniSparklineProps {
  /** Parallel-array timeline + values. */
  time: number[];
  values: number[];
  /** Stroke color when `variant="price"`. */
  color: string;
  variant?: MiniSparklineVariant;
  height?: number;
}

export function MiniSparkline({
  time,
  values,
  color,
  variant = "price",
  height = 48,
}: MiniSparklineProps): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();

  const stroke = variant === "drawdown" ? theme.warm.danger : color;
  // Pull a fill color from the stroke. For price: stroke@22 alpha
  // (hex-suffix idiom from the handoff). For drawdown: red @22 alpha.
  const fillTop =
    variant === "drawdown"
      ? "rgba(200,68,58,0.22)"
      : `${color}22`;

  // Strip cursor + axes from the base theme options so the sparkline
  // is a pure line.
  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;

  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series" | "axes" | "cursor">),
    width: 0,
    height,
    cursor: { show: false },
    legend: { show: false },
    axes: [
      { show: false },
      { show: false },
    ],
    scales: { x: { time: true }, y: { auto: true } },
    series: [
      {},
      {
        stroke,
        width: 1.4,
        points: { show: false },
      },
    ],
    plugins: [xvnAreaFill(1, fillTop)],
  };

  usePlot(opts, [time, values] as uPlot.AlignedData, hostRef, height);

  return <div ref={hostRef} style={{ width: "100%", height }} />;
}
