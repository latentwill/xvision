/**
 * MultiStrategyEquityPane — N overlaid equity curves on one uPlot pane.
 *
 * Reused by B1 (Dark Minimal Strategy Dashboard) as the hero overlay, by
 * B2 (Comparison AB) likewise, and by B4 (Gradient Hero) as the basis
 * for HeroGradientEquity. Per-strategy stroke width is differentiated:
 * the lead series renders heaviest (default 1.7px), others at 1.15px,
 * and benchmark series (`dashed: true`) render with a dash pattern. The
 * lead also gets the `xvnLastDot` halo from the uplot-plugins port.
 *
 * Cursor sync key is configurable so callers can join this pane to
 * other uPlot panes via `uPlot.sync()`.
 */
import "uplot/dist/uPlot.min.css";

import { useRef, type ReactElement } from "react";
import uPlot from "uplot";

import { themeToUplotOptions } from "../adapters/theme-to-uplot";
import { xvnLastDot } from "../adapters/uplot-plugins";
import { useChart2Theme } from "../hooks/useChart2Theme";
import { usePlot } from "./usePlot";

export interface MultiStrategyEquitySeries {
  id: string;
  label: string;
  /** % return aligned to the shared `time` array (parallel arrays). */
  values: number[];
  color: string;
  dashed?: boolean;
}

export interface MultiStrategyEquityPaneProps {
  /** Unix-seconds timeline shared across all series. */
  time: number[];
  series: MultiStrategyEquitySeries[];
  /** Which series receives the heavy stroke + last-dot halo. Defaults
   *  to `series[0]`. */
  leadId?: string;
  /** Stroke width for the lead series. */
  leadStrokeWidth?: number;
  /** Stroke width for non-lead series. */
  nonLeadStrokeWidth?: number;
  /** Optional uPlot sync key. When provided, the pane joins the named
   *  cursor-sync group so other panes (e.g. drawdown) move in lockstep. */
  syncKey?: string;
  height?: number;
}

/**
 * Build uPlot AlignedData from the shared timeline + per-strategy series.
 * Exposed for testing.
 */
export function buildAlignedData(
  time: number[],
  series: MultiStrategyEquitySeries[],
): uPlot.AlignedData {
  return [time, ...series.map((s) => s.values)] as uPlot.AlignedData;
}

/**
 * Resolve which series is "lead" — explicit `leadId` wins; otherwise
 * the first entry. Returns the array index, or `0` if `series` is empty.
 * Exposed for testing.
 */
export function resolveLeadIndex(
  series: MultiStrategyEquitySeries[],
  leadId?: string,
): number {
  if (!leadId) return 0;
  const idx = series.findIndex((s) => s.id === leadId);
  return idx === -1 ? 0 : idx;
}

export function MultiStrategyEquityPane({
  time,
  series,
  leadId,
  leadStrokeWidth = 1.7,
  nonLeadStrokeWidth = 1.15,
  syncKey,
  height = 280,
}: MultiStrategyEquityPaneProps): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();

  const leadIdx = resolveLeadIndex(series, leadId);
  const data = buildAlignedData(time, series);

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;

  const cursorOpts: uPlot.Cursor = {
    ...(baseOpts.cursor as uPlot.Cursor | undefined),
    ...(syncKey != null ? { sync: { key: syncKey, setSeries: true } } : {}),
  };

  // Series 0 in uPlot is the x-axis "series"; the actual data series
  // begin at index 1, so the lead series is at uPlot index `leadIdx + 1`.
  const leadUplotIdx = leadIdx + 1;
  const leadColor = series[leadIdx]?.color ?? theme.panes.equity;

  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0,
    height,
    cursor: cursorOpts,
    series: [
      {},
      ...series.map((s, i) => {
        const isLead = i === leadIdx;
        const seriesSpec: uPlot.Series = {
          label: s.label,
          stroke: s.color,
          width: isLead ? leadStrokeWidth : nonLeadStrokeWidth,
          points: { show: false },
        };
        if (s.dashed) {
          seriesSpec.dash = [4, 4];
        }
        return seriesSpec;
      }),
    ],
    plugins: [xvnLastDot(leadUplotIdx, leadColor)],
  };

  usePlot(opts, data, hostRef, height);

  return <div ref={hostRef} style={{ width: "100%" }} />;
}
