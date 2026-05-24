/**
 * HeroGradientEquity — single-series equity pane for the B4
 * GradientHeroDashboard. Wraps uPlot with the xvnGradientFill 5-stop
 * area-fill + xvnSheen top-band highlight + xvnLastDot halo on the
 * lead series.
 *
 * Single series (the lead strategy's equity). Use
 * MultiStrategyEquityPane (B1) for N-strategy overlays — this variant
 * is intentionally hero-focused.
 */
import "uplot/dist/uPlot.min.css";

import { useRef, type ReactElement } from "react";
import uPlot from "uplot";

import { themeToUplotOptions } from "../adapters/theme-to-uplot";
import {
  xvnGradientFill,
  xvnLastDot,
  xvnSheen,
} from "../adapters/uplot-plugins";
import { useChart2Theme } from "../hooks/useChart2Theme";
import { usePlot } from "./usePlot";

export interface HeroGradientEquityProps {
  time: number[];
  /** % return aligned to `time`. */
  values: number[];
  /** Stroke + halo color for the lead series. */
  color?: string;
  height?: number;
}

export function HeroGradientEquity({
  time,
  values,
  color,
  height = 320,
}: HeroGradientEquityProps): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const stroke = color ?? theme.warm.gold;

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;

  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0,
    height,
    cursor: { show: true, drag: { x: true, y: false } },
    legend: { show: false },
    scales: { x: { time: true }, y: { auto: true } },
    series: [
      {},
      {
        stroke,
        width: 1.8,
        points: { show: false },
      },
    ],
    plugins: [
      // Order matters: fill first (drawn behind), then sheen, then halo.
      xvnGradientFill(1),
      xvnSheen(),
      xvnLastDot(1, stroke),
    ],
  };

  usePlot(opts, [time, values] as uPlot.AlignedData, hostRef, height);

  return <div ref={hostRef} style={{ width: "100%" }} />;
}
