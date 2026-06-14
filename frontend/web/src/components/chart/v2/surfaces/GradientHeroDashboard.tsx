/**
 * GradientHeroDashboard — surface for `/charts/hero` (B4). The "hero"
 * variant of the multi-strategy dashboard:
 *   - AuraBackground + GrainOverlay chrome behind everything.
 *   - GradientHeadline topbar.
 *   - 5-up KpiRow with `cornerGlow="gold"` on the Total Return card.
 *   - HeroGradientEquity (single-series, multi-stop gold fill + sheen
 *     + halo) for the lead strategy.
 *   - PerformanceRadar with the top 3 strategies overlaid.
 *   - DrawdownCard with `leadStyle="gold-tinted-red"`.
 *   - MarketContextCard wired to GET /api/v2/charts/market-context.
 *     Falls back to the embedded literals on pending/error so the
 *     dashboard always looks complete (no loading flash for scalar data).
 *
 * Per spec §11.3 resolution, B4 mounts only at /charts/hero in this
 * wave; the /-replacement decision is the B5 review milestone.
 */
import type { ReactElement } from "react";
import { useQuery } from "@tanstack/react-query";

import type { MultiStrategyEquityBundle } from "../types";
import { getMarketContext, marketContextKeys } from "@/api/chart";
import type { MarketContextData, RegimeWeight } from "../types";
import { KpiCard, KpiRow } from "../primitives/KpiCard";
import {
  AuraBackground,
  GrainOverlay,
  GradientHeadline,
  HeroGradientEquity,
  PerformanceRadar,
  MarketContextCard,
  GlassCard,
} from "../primitives";
import { DrawdownCard, type DrawdownStats } from "../primitives/DrawdownCard";
import {
  pickLead,
  deriveDrawdownStats,
} from "./DarkMinimalDashboard";
import type { RadarStrategy } from "../primitives/PerformanceRadar";

export interface GradientHeroDashboardProps {
  payload: MultiStrategyEquityBundle;
}

function fmtPct(n: number, d = 2): string {
  const sign = n > 0 ? "+" : "";
  return `${sign}${n.toFixed(d)}%`;
}

/** Format an absolute (sign-stripped) percentage for the headline emphasis.
 *  Produces "1.02%" rather than "+1.02%" so the suffix ("is up"/"is down")
 *  carries the direction and the number stays clean. */
function fmtAbsPct(n: number, d = 2): string {
  return `${Math.abs(n).toFixed(d)}%`;
}

function fmtRatio(n: number): string {
  return n.toFixed(2);
}

/**
 * Normalise a strategy's metric vector into [0,1] values for the
 * 6-axis radar. Bounds picked so realistic strategies land in the
 * mid-range: Return ±150%, Sharpe 0..3, Stability ~ inverse of Max DD,
 * Win 0..100%, Consistency ~ Profit Factor / 2, Drawdown ~ same as
 * Stability axis but flipped.
 *
 * Exported for tests.
 */
export function strategyToRadar(
  s: MultiStrategyEquityBundle["strategies"][number],
): number[] {
  // Bounds — keep these calibrated to handoff sample data.
  const returnNorm = Math.max(0, Math.min(1, (s.metrics.return + 50) / 200));
  const sharpeNorm = Math.max(0, Math.min(1, s.metrics.sharpe / 3));
  // Stability = inverse of max drawdown magnitude (less negative = better).
  const stabilityNorm = Math.max(
    0,
    Math.min(1, 1 - Math.abs(s.metrics.mdd) / 40),
  );
  const winNorm = Math.max(0, Math.min(1, s.metrics.win / 100));
  const consistencyNorm = Math.max(0, Math.min(1, s.metrics.pf / 2));
  // Drawdown axis: 1 = no drawdown, 0 = ≥40%.
  const drawdownNorm = stabilityNorm;
  return [
    returnNorm,
    sharpeNorm,
    stabilityNorm,
    winNorm,
    consistencyNorm,
    drawdownNorm,
  ];
}

/**
 * Fallback literals — used while the market-context query is pending or when
 * it errors, so the dashboard always renders a complete card. Loading choice:
 * literal fallback (not skeleton) because the data is all scalar numbers and
 * a brief flash of slightly-stale data is preferable to a blank/skeleton card.
 */
const FALLBACK_MARKET_CONTEXT: MarketContextData = {
  price: 65_128.4,
  fundingPct: 0.012,
  openInterestUsd: 7_450_000_000,
  liq24hUsd: 84_000_000,
};

const FALLBACK_REGIMES: RegimeWeight[] = [
  { label: "BULL", pct: 62 },
  { label: "SIDEWAYS", pct: 22 },
  { label: "BEAR", pct: 9 },
  { label: "HIGH VOL", pct: 7 },
];

export function GradientHeroDashboard({
  payload,
}: GradientHeroDashboardProps): ReactElement {
  const lead = pickLead(payload);

  const marketCtxQ = useQuery({
    queryKey: marketContextKeys.get(),
    queryFn: getMarketContext,
    // Market context is low-churn; refresh every 60 s is sufficient.
    refetchInterval: 60_000,
    refetchOnWindowFocus: false,
  });
  const marketData = marketCtxQ.data?.data ?? FALLBACK_MARKET_CONTEXT;
  const marketRegimes = marketCtxQ.data?.regimes ?? FALLBACK_REGIMES;
  const leadStats: DrawdownStats = lead
    ? deriveDrawdownStats(lead.drawdown)
    : { maxDrawdownPct: 0, avgDrawdownPct: 0, durationDays: 0, recoveryDays: null };

  const top3: RadarStrategy[] = payload.strategies.slice(0, 3).map((s) => ({
    id: s.id,
    label: s.short,
    color: s.color,
    values: strategyToRadar(s),
  }));

  return (
    <div className="relative">
      {/* Chrome — z-index 0 */}
      <AuraBackground />
      <GrainOverlay />

      {/* Content — z-index 1 */}
      <div className="relative z-[1] flex flex-col gap-4">
        <header className="px-1 flex items-end justify-between gap-4">
          <div>
            <div className="caps text-gold">CRYPTO · STRATEGY HUB</div>
            <div className="mt-1">
              {lead ? (
                <GradientHeadline
                  prefix="The"
                  bracketed={lead.name}
                  suffix={lead.metrics.return >= 0 ? "is up" : "is down"}
                  emphasis={fmtAbsPct(lead.metrics.return)}
                />
              ) : (
                <GradientHeadline
                  prefix="No strategy selected"
                  bracketed=""
                />
              )}
            </div>
          </div>
        </header>

        <KpiRow>
          {lead && (
            <>
              <KpiCard
                label="Total Return"
                value={fmtPct(lead.metrics.return)}
                foot={lead.short}
                cornerGlow="gold"
              />
              <KpiCard
                label="Sharpe"
                value={fmtRatio(lead.metrics.sharpe)}
                foot="risk-adjusted"
              />
              <KpiCard
                label="Max DD"
                value={fmtPct(lead.metrics.mdd)}
                foot="peak to trough"
                intent="danger"
              />
              <KpiCard
                label="Win Rate"
                value={`${lead.metrics.win.toFixed(1)}%`}
                foot="winners / total"
              />
              <KpiCard
                label="Profit Factor"
                value={fmtRatio(lead.metrics.pf)}
                foot="gross win / gross loss"
              />
            </>
          )}
        </KpiRow>

        <GlassCard className="p-4">
          <div className="caps mb-3">Return % · {lead?.short ?? "—"}</div>
          {lead && (
            <HeroGradientEquity
              time={payload.time}
              values={lead.equity}
              color={lead.color}
            />
          )}
        </GlassCard>

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
          <GlassCard className="p-4 flex items-center justify-center">
            <div className="flex flex-col items-center gap-3">
              <div className="caps">Performance Radar</div>
              <PerformanceRadar strategies={top3} />
              <div className="flex flex-wrap items-center justify-center gap-3 text-[11px] text-text-3">
                {top3.map((s) => (
                  <span
                    key={s.id}
                    className="inline-flex items-center gap-1.5"
                  >
                    <span
                      aria-hidden="true"
                      className="inline-block w-2 h-2 rounded-full"
                      style={{ backgroundColor: s.color }}
                    />
                    {s.label}
                  </span>
                ))}
              </div>
            </div>
          </GlassCard>

          <div className="lg:col-span-2 grid grid-cols-1 md:grid-cols-2 gap-4">
            {lead && (
              <DrawdownCard
                title={`Drawdown · ${lead.short}`}
                points={payload.time.map((t, i) => ({
                  time: t,
                  value: lead.drawdown[i] ?? 0,
                }))}
                stats={leadStats}
                leadStyle="gold-tinted-red"
              />
            )}
            <GlassCard>
              <MarketContextCard
                data={marketData}
                regimes={marketRegimes}
              />
            </GlassCard>
          </div>
        </div>
      </div>
    </div>
  );
}
