// src/features/marketplace/routes/PerformanceSection.tsx
//
// The inspector's first-class performance citizen (QA17). App-native styling.
// Full-width, min-height 360px. Built entirely from the existing chart v2
// stack — no new chart infrastructure:
//   - ChartFrame wrapper (range presets, no-popup inline expand)
//   - a single time-aligned equity series fed to a uPlot pane that mirrors
//     HeroGradientEquity but attaches the W1 `xvnTradeMarkers` draw-hook
//     (gold ▲ buy / red ▼ sell) sourced from on-chain trades
//   - MarkerDock rendered INLINE below the chart (never a side rail)
//   - a designed EmptyState when there is no live record yet — NEVER a fake
//     curve. This empty state is the shipping default for real on-chain
//     listings; the chart lights up the moment the eval link lands.
//
// The broken two-layer raw-SVG backtest polyline from the old EquityPanel is
// gone: this renders ONE time-aligned equity series so uPlot keeps a single
// coordinate space.
import "uplot/dist/uPlot.min.css";

import { useMemo, useRef, useState, type ReactElement } from "react";
import { Link } from "react-router-dom";
import uPlot from "uplot";

import { ChartFrame, type RangePreset } from "@/components/chart/v2/primitives/ChartFrame";
import { MarkerDock } from "@/components/chart/v2/primitives/MarkerDock";
import { EmptyState } from "@/components/chart/v2/primitives/EmptyState";
import { usePlot } from "@/components/chart/v2/primitives/usePlot";
import { themeToUplotOptions } from "@/components/chart/v2/adapters/theme-to-uplot";
import {
  xvnGradientFill,
  xvnLastDot,
  xvnSheen,
  xvnTradeMarkers,
} from "@/components/chart/v2/adapters/uplot-plugins";
import { useChart2Theme } from "@/components/chart/v2/hooks/useChart2Theme";
import type { V2Marker } from "@/components/chart/v2/types";
import type { EquityCurve, TradeRecord } from "@/features/marketplace/data/types";

interface Props {
  curve: EquityCurve;
  trades: TradeRecord[];
  /**
   * Live PnL (USD) from the Degen Arena on-chain runner, when available.
   * When present: rendered as authoritative "Live · Degen Arena (on-chain)" figure.
   * When null/undefined: the live row is hidden; only the "Indicative · backtest"
   * caption is shown on the equity curve.
   *
   * TODO(degen provenance): wire live Degen Arena PnL source — the on-chain
   * marketplace indexer is dormant/env-gated; connect this prop once
   * `onChain.tradesMeta.netPnlUsd` is populated from the indexer.
   */
  liveDegenPnlUsd?: number | null;
}

// Inline provenance banner — full-width, no popup, dark-mode safe.
// Shows "Live · Degen Arena (on-chain)" when live data exists, and always
// shows the "Indicative · backtest" caption so users understand the equity
// curve is not a real-money track record.
function ProvenanceBanner({
  liveDegenPnlUsd,
}: {
  liveDegenPnlUsd?: number | null;
}): ReactElement {
  const hasLive = liveDegenPnlUsd != null;
  const pnlPositive = hasLive && liveDegenPnlUsd >= 0;

  return (
    <div
      data-testid="provenance-banner"
      className="flex flex-wrap gap-x-4 gap-y-1.5 items-center rounded-card border border-border bg-surface-card px-3 py-2"
    >
      {/* Live Degen Arena figure — only when data is available */}
      {hasLive && (
        <div
          data-testid="provenance-live"
          className="flex items-center gap-1.5 min-w-0"
        >
          {/* Pulsing live dot */}
          <span className="relative flex h-2 w-2 shrink-0">
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-60" />
            <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-500" />
          </span>
          <span className="font-mono text-[10px] tracking-[0.12em] uppercase text-emerald-500 whitespace-nowrap">
            Live · Degen Arena (on-chain)
          </span>
          <span
            className={[
              "font-mono text-[12px] font-semibold tabular-nums whitespace-nowrap",
              pnlPositive ? "text-emerald-400" : "text-danger",
            ].join(" ")}
          >
            {pnlPositive ? "+" : ""}
            {liveDegenPnlUsd!.toLocaleString("en-US", {
              style: "currency",
              currency: "USD",
              maximumFractionDigits: 2,
            })}
          </span>
          <span className="font-mono text-[9px] text-text-3 uppercase tracking-[0.1em] whitespace-nowrap">
            authoritative
          </span>
        </div>
      )}

      {/* Divider between live and backtest when both present */}
      {hasLive && (
        <div className="h-3 w-px bg-border shrink-0 hidden sm:block" aria-hidden />
      )}

      {/* Indicative backtest caption — always shown */}
      <div
        data-testid="provenance-indicative"
        className="flex items-center gap-1.5 min-w-0"
      >
        <span className="font-mono text-[10px] tracking-[0.12em] uppercase text-text-3 whitespace-nowrap">
          Indicative · backtest
        </span>
        <span className="font-mono text-[9px] text-text-3 uppercase tracking-[0.1em] whitespace-nowrap">
          not a real-money track record
        </span>
      </div>
    </div>
  );
}

// Equity points → unix-second time axis aligned 1 point/day ending now.
// The fixture curve carries no timestamps (just a value/phase sequence), so
// we synthesize a daily cadence; real equity series replace this when the
// eval link lands.
function curveToSeries(curve: EquityCurve): { time: number[]; values: number[] } {
  const nowSec = Math.floor(Date.now() / 1000);
  const n = curve.points.length;
  const time = curve.points.map((_, i) => nowSec - (n - 1 - i) * 86_400);
  const values = curve.points.map((p) => p.value);
  return { time, values };
}

// On-chain trade → V2Marker (overhaul §3.2B mapping table).
//   action "buy"          → kind "buy"   (gold ▲)
//   action "sell"|"close" → kind "sell"  (red  ▼)
//   time  = epoch(trade.at)
//   price = trade.entry ?? trade.exit
//   text  = "<symbol> <pnlPct>%"
function tradesToMarkers(trades: TradeRecord[]): V2Marker[] {
  return trades.map((t, idx) => {
    const epoch = Math.floor(new Date(t.at).getTime() / 1000);
    const kind: V2Marker["kind"] = t.action === "buy" ? "buy" : "sell";
    const price = t.entry ?? t.exit ?? undefined;
    const pnl = t.pnlPct != null ? ` ${t.pnlPct > 0 ? "+" : ""}${t.pnlPct}%` : "";
    return {
      kind,
      time: epoch,
      price: price ?? undefined,
      text: `${t.symbol}${pnl}`.trim(),
      decision_index: idx,
    };
  });
}

// Equity pane that mirrors HeroGradientEquity (gold gradient fill + sheen +
// last-dot) but additionally attaches the W1 `xvnTradeMarkers` plugin so the
// on-chain buy/sell triangles draw onto the curve at exact timestamps.
function EquityMarkerPane({
  time,
  values,
  markers,
  height = 320,
}: {
  time: number[];
  values: number[];
  markers: V2Marker[];
  height?: number;
}): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const stroke = theme.warm.gold;

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
      // Order matters: fill behind, sheen, last-dot, then trade markers on top.
      xvnGradientFill(1),
      xvnSheen(),
      xvnLastDot(1, stroke),
      xvnTradeMarkers(markers, {
        buyColor: theme.marker.buy,
        sellColor: theme.marker.sell,
      }),
    ],
  };

  usePlot(opts, [time, values] as uPlot.AlignedData, hostRef, height);

  return <div ref={hostRef} style={{ width: "100%" }} />;
}

export function PerformanceSection({ curve, trades, liveDegenPnlUsd }: Props): ReactElement {
  const [range, setRange] = useState<RangePreset>("All");
  const [activeId, setActiveId] = useState<string | undefined>(undefined);

  const { time, values } = useMemo(() => curveToSeries(curve), [curve]);
  const markers = useMemo(() => tradesToMarkers(trades), [trades]);

  const hasEquity = curve.points.length > 0;
  const hasTrades = trades.length > 0;

  // Honest empty state — the real on-chain default until the eval link lands.
  // NEVER a fabricated curve.
  if (!hasEquity && !hasTrades) {
    return (
      <section
        data-testid="performance-section"
        className="space-y-3"
      >
        <div className="font-mono text-[11px] tracking-[0.18em] uppercase text-text-3">
          Performance
        </div>
        {/* Provenance banner — always inline, full-width, no popup. */}
        <ProvenanceBanner liveDegenPnlUsd={liveDegenPnlUsd} />
        <div data-testid="performance-empty">
          <EmptyState
            title="No live performance record yet"
            message="This strategy hasn't completed a trading cycle on-chain."
          />
          <div className="mt-3 text-center">
            {/* Routes to the eval surface (run a backtest), NOT the mint funnel
                — the empty-state context is "run a backtest", not "publish". */}
            <Link
              to="/eval-runs"
              className="font-mono text-[11.5px] text-gold hover:underline"
            >
              Run a backtest →
            </Link>
          </div>
        </div>
      </section>
    );
  }

  return (
    <section data-testid="performance-section" className="space-y-3">
      <div className="font-mono text-[11px] tracking-[0.18em] uppercase text-text-3">
        Performance
      </div>
      {/* Provenance banner — always inline, full-width, no popup. */}
      <ProvenanceBanner liveDegenPnlUsd={liveDegenPnlUsd} />
      <ChartFrame title="Equity" range={range} onRange={setRange}>
        <div style={{ minHeight: 360 }}>
          <EquityMarkerPane
            time={time}
            values={values}
            markers={markers}
            height={360}
          />
        </div>
      </ChartFrame>

      {/* MarkerDock — INLINE below the chart, full-width (never a side rail). */}
      {hasTrades && (
        <div
          data-testid="marker-dock"
          className="rounded-card border border-border bg-surface-card p-2"
        >
          <div className="font-mono text-[9px] tracking-[0.2em] uppercase text-text-3 px-1 pb-1.5">
            On-chain actuations
          </div>
          <MarkerDock markers={markers} activeId={activeId} onSelect={setActiveId} />
        </div>
      )}
    </section>
  );
}
