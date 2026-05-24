/**
 * UplotCompareOverlayPane — multi-arm equity overlay (A/B compare).
 *
 * Each arm is assigned a color from theme.compare.palette.
 * Legend shown via uPlot's built-in `legend: { show: true }` for M0.
 */
import "uplot/dist/uPlot.min.css";

import React, { useRef } from "react";
import uPlot from "uplot";

import { type EquityPoint } from "../types";
import { columnarToUplotCompare } from "../adapters/columnar-to-uplot";
import { themeToUplotOptions } from "../adapters/theme-to-uplot";
import { useChart2Theme } from "../hooks/useChart2Theme";
import { useSyncKey } from "./PaneStack";
import { usePlot } from "./usePlot";

/** One arm in the compare overlay. Named with "Overlay" prefix to avoid
 * clashing with the CompareArm in types.ts when both are re-exported. */
export interface CompareOverlayArm {
  id: string;
  label: string;
  equity: EquityPoint[];
  color?: string;
}

export interface UplotCompareOverlayPaneProps {
  arms: CompareOverlayArm[];
  height?: number;
}

export function UplotCompareOverlayPane({
  arms,
  height = 160,
}: UplotCompareOverlayPaneProps): React.ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const syncKey = useSyncKey();

  // Convert arms to the shape expected by columnarToUplotCompare.
  const compareInput = arms.map((arm) => ({
    time: arm.equity.map((p) => p.time),
    values: arm.equity.map((p) => p.value),
  }));

  const data = columnarToUplotCompare(compareInput);

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;

  const cursorOpts: uPlot.Cursor = {
    ...(baseOpts.cursor as uPlot.Cursor | undefined),
    ...(syncKey != null
      ? { sync: { key: syncKey, setSeries: true } }
      : {}),
  };

  // Build one uPlot series per arm.
  const uplotSeries: uPlot.Series[] = [
    {},
    ...arms.map((arm, idx) => ({
      label: arm.label,
      stroke: arm.color ?? theme.compare.palette[idx % theme.compare.palette.length],
      width: 1.5,
      points: { show: false },
    })),
  ];

  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0,
    height,
    cursor: cursorOpts,
    legend: { show: true },
    series: uplotSeries,
  };

  usePlot(opts, data, hostRef, height);

  return <div ref={hostRef} style={{ width: "100%" }} />;
}
