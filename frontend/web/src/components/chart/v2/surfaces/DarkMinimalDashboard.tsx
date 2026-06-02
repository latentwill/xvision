/**
 * DarkMinimalDashboard — surface for `/charts/overview` (B1).
 *
 * Composes the Track-A `ChartFrame` chrome with the new B1 primitives:
 * `ChartsTopbar`, `KpiRow`+`KpiCard`×5, `MultiStrategyEquityPane`,
 * `DrawdownCard`, `MonthlyReturnsHeatmap`.
 *
 * Data: a `MultiStrategyEquityBundle` (returned by
 * `/api/v2/charts/dashboards/overview` — B0 ships a fixture-backed stub,
 * a follow-up replaces with a real builder pairing each `Strategy`
 * with its latest backtest run).
 *
 * No popups, no overlays — section is a vertical stack inside the
 * `/charts` shell's outlet area (per the workspace no-popups rule).
 */
import type { ReactElement } from "react";

import type { MultiStrategyEquityBundle } from "../types";
import { KpiCard, KpiRow } from "../primitives/KpiCard";
import { ChartsTopbar } from "../primitives/Topbar";
import {
  MultiStrategyEquityPane,
  type MultiStrategyEquitySeries,
} from "../primitives/MultiStrategyEquityPane";
import { DrawdownCard, type DrawdownStats } from "../primitives/DrawdownCard";
import { MonthlyReturnsHeatmap } from "../primitives/MonthlyReturnsHeatmap";

export interface DarkMinimalDashboardProps {
  payload: MultiStrategyEquityBundle;
}

function fmtPct(n: number, digits = 2): string {
  const sign = n > 0 ? "+" : "";
  return `${sign}${n.toFixed(digits)}%`;
}

function fmtRatio(n: number): string {
  return n.toFixed(2);
}

/**
 * Pick the bundle's lead strategy. Honours `bundle.lead` when set;
 * otherwise the first strategy by insertion order.
 * Exported for tests.
 */
export function pickLead(
  bundle: MultiStrategyEquityBundle,
): MultiStrategyEquityBundle["strategies"][number] | undefined {
  if (bundle.strategies.length === 0) return undefined;
  if (bundle.lead) {
    const found = bundle.strategies.find((s) => s.id === bundle.lead);
    if (found) return found;
  }
  return bundle.strategies[0];
}

/**
 * Convert a `MultiStrategyEquityBundle.strategies[]` entry into the
 * shape `MultiStrategyEquityPane` consumes. Exported for tests.
 */
export function toEquitySeries(
  strategies: MultiStrategyEquityBundle["strategies"],
): MultiStrategyEquitySeries[] {
  return strategies.map((s) => ({
    id: s.id,
    label: s.short,
    values: s.equity,
    color: s.color,
    ...(s.dashed ? { dashed: true } : {}),
  }));
}

/**
 * Derive drawdown footer stats for the lead strategy from its `drawdown`
 * column. Drawdown values are ≤ 0 (% from peak). Returns:
 *   - maxDrawdownPct: most-negative value
 *   - avgDrawdownPct: mean across the column (includes 0-values at peaks)
 *   - durationDays: longest run of strictly-negative values (in samples;
 *     callers can label as days when the bundle is daily — B1 always is)
 *   - recoveryDays: samples between the worst point and the next 0
 *     (≥ 0); `null` when still underwater
 * Exported for tests.
 */
export function deriveDrawdownStats(drawdown: number[]): DrawdownStats {
  if (drawdown.length === 0) {
    return {
      maxDrawdownPct: 0,
      avgDrawdownPct: 0,
      durationDays: 0,
      recoveryDays: 0,
    };
  }

  let maxDD = 0;
  let maxIdx = 0;
  let sum = 0;
  for (let i = 0; i < drawdown.length; i++) {
    const v = drawdown[i];
    sum += v;
    if (v < maxDD) {
      maxDD = v;
      maxIdx = i;
    }
  }
  const avgDD = sum / drawdown.length;

  // Longest run of strictly-negative values.
  let durationDays = 0;
  let cur = 0;
  for (const v of drawdown) {
    if (v < 0) {
      cur += 1;
      if (cur > durationDays) durationDays = cur;
    } else {
      cur = 0;
    }
  }

  // Recovery: samples from maxIdx forward until the next 0 (or null).
  let recoveryDays: number | null = null;
  for (let i = maxIdx; i < drawdown.length; i++) {
    if (drawdown[i] >= 0) {
      recoveryDays = i - maxIdx;
      break;
    }
  }

  return { maxDrawdownPct: maxDD, avgDrawdownPct: avgDD, durationDays, recoveryDays };
}

export function DarkMinimalDashboard({
  payload,
}: DarkMinimalDashboardProps): ReactElement {
  const lead = pickLead(payload);
  const series = toEquitySeries(payload.strategies);
  const leadStats = lead
    ? deriveDrawdownStats(lead.drawdown)
    : { maxDrawdownPct: 0, avgDrawdownPct: 0, durationDays: 0, recoveryDays: null };

  // Monthly returns heatmap rows (5 strategies × N months from bundle).
  const monthlyRows = payload.strategies.map((s) => ({
    id: s.id,
    label: s.short,
    cells: s.monthly,
  }));

  return (
    <div className="flex flex-col gap-4">
      <ChartsTopbar
        eyebrow="STRATEGY · OVERVIEW"
        headline="Strategy Comparison"
        tagline={
          lead
            ? `${lead.name} leading at ${fmtPct(lead.metrics.return)}.`
            : undefined
        }
      />

      <KpiRow>
        {lead && (
          <>
            <KpiCard
              label="Total Return"
              value={fmtPct(lead.metrics.return)}
              foot={lead.short}
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

      <div className="border border-border rounded-card bg-surface-card overflow-hidden">
        <header className="px-4 py-3 border-b border-border flex items-center justify-between gap-3">
          <div className="caps">Return %</div>
          <div className="flex items-center gap-3 text-[11px] text-text-3">
            {payload.strategies.map((s) => (
              <span key={s.id} className="inline-flex items-center gap-1.5">
                <span
                  className="inline-block w-2.5 h-[2px]"
                  style={{ backgroundColor: s.color }}
                  aria-hidden="true"
                />
                <span style={{ fontFamily: 'Geist Mono, ui-monospace, monospace' }}>
                  {s.short}
                </span>
              </span>
            ))}
          </div>
        </header>
        <div className="px-4 py-3">
          <MultiStrategyEquityPane
            time={payload.time}
            series={series}
            leadId={lead?.id}
            syncKey="dashboard-overview"
          />
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        {lead && (
          <DrawdownCard
            title={`Drawdown · ${lead.short}`}
            points={payload.time.map((t, i) => ({
              time: t,
              value: lead.drawdown[i] ?? 0,
            }))}
            stats={leadStats}
          />
        )}
        <MonthlyReturnsHeatmap rows={monthlyRows} />
      </div>
    </div>
  );
}
