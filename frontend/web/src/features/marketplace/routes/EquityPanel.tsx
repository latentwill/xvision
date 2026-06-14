// src/features/marketplace/routes/EquityPanel.tsx
//
// Thin delegator kept for backward-compat with existing call sites. The
// performance overhaul (§3.2B, QA17) moves all performance rendering into
// `PerformanceSection` — a full-width ChartFrame + time-aligned equity series
// + on-chain `xvnTradeMarkers` + inline MarkerDock + designed empty state.
//
// The broken two-layer raw-SVG backtest polyline lived here previously; it is
// gone. `EquityPanel` now simply forwards `curve` (+ optional on-chain
// `trades`) into `PerformanceSection`.
import { type ReactElement } from "react";
import { PerformanceSection } from "./PerformanceSection";
import type { EquityCurve, TradeRecord } from "@/features/marketplace/data/types";

interface Props {
  curve: EquityCurve;
  /** On-chain trades → buy/sell markers on the equity curve. Optional so the
   *  old single-arg call site keeps type-checking; defaults to none. */
  trades?: TradeRecord[];
  /**
   * Live PnL (USD) from the Degen Arena on-chain runner. Passed through to
   * PerformanceSection's provenance banner.
   *
   * TODO(degen provenance): wire live Degen Arena PnL source.
   */
  liveDegenPnlUsd?: number | null;
}

export function EquityPanel({ curve, trades = [], liveDegenPnlUsd }: Props): ReactElement {
  return <PerformanceSection curve={curve} trades={trades} liveDegenPnlUsd={liveDegenPnlUsd} />;
}
