/**
 * ComparisonABDashboard — surface for `/charts/compare` (B2).
 *
 * Composes ChartsTopbar (headline = "N strategies, one frame") +
 * MultiStrategyEquityPane (hero overlay with inline legend) +
 * StrategyRosterPills + StrategyCardGrid of StrategyCards.
 *
 * Selection state is URL-synced via `useChart2Roster` so deep links
 * like `/charts/compare?ids=fib,ema,brk` restore the same view. The
 * first id in `selectedIds` is treated as lead.
 *
 * Reuses B1's MultiStrategyEquityPane and Topbar primitives.
 */
import type { ReactElement } from "react";

import type { MultiStrategyEquityBundle, MultiStrategyBundleEntry } from "../types";
import { ChartsTopbar } from "../primitives/Topbar";
import {
  MultiStrategyEquityPane,
  type MultiStrategyEquitySeries,
} from "../primitives/MultiStrategyEquityPane";
import { StrategyRosterPills } from "../primitives/StrategyRosterPills";
import { StrategyCardGrid } from "../primitives/StrategyCardGrid";
import { StrategyCard } from "../primitives/StrategyCard";
import { LeadCardChrome } from "../primitives/LeadCardChrome";
import { useChart2Roster } from "../hooks/useChart2Roster";

export interface ComparisonABDashboardProps {
  payload: MultiStrategyEquityBundle;
}

/**
 * Order strategies by `selectedIds` then by the bundle's original
 * insertion order. Items not in `selectedIds` are filtered out.
 * Exported for tests.
 */
export function pickSelectedInOrder(
  strategies: MultiStrategyBundleEntry[],
  selectedIds: readonly string[],
): MultiStrategyBundleEntry[] {
  const order = new Map(selectedIds.map((id, i) => [id, i]));
  return strategies
    .filter((s) => order.has(s.id))
    .sort((a, b) => (order.get(a.id) ?? 0) - (order.get(b.id) ?? 0));
}

export function ComparisonABDashboard({
  payload,
}: ComparisonABDashboardProps): ReactElement {
  // Every strategy id in the bundle, in stable insertion order.
  const availableIds = payload.strategies.map((s) => s.id);
  // Default selection: the first 6 (or all if fewer), so the URL stays
  // tidy when the page loads cold. The hook enforces the min-2 invariant.
  const defaultSelected = availableIds.slice(0, Math.min(6, availableIds.length));

  const roster = useChart2Roster({
    available: availableIds,
    defaultSelected,
  });

  const selectedInOrder = pickSelectedInOrder(payload.strategies, roster.selectedIds);
  const leadId = selectedInOrder[0]?.id;

  // Hero overlay series (one per selected strategy).
  const heroSeries: MultiStrategyEquitySeries[] = selectedInOrder.map((s) => ({
    id: s.id,
    label: s.short,
    values: s.equity,
    color: s.color,
    ...(s.dashed ? { dashed: true } : {}),
  }));

  const rosterPillItems = payload.strategies.map((s) => ({
    id: s.id,
    label: s.short,
    color: s.color,
  }));

  return (
    <div className="flex flex-col gap-4">
      <ChartsTopbar
        eyebrow="STRATEGY · COMPARISON"
        headline={`${roster.count} strategies, one frame`}
        tagline="Click a pill to add or drop. Min 2."
      />

      <div className="border border-border rounded-card bg-surface-card overflow-hidden">
        <header className="px-4 py-3 border-b border-border flex items-center justify-between gap-3">
          <div className="caps">Hero overlay</div>
          <div className="flex items-center gap-3 text-[11px] text-text-3 overflow-x-auto">
            {selectedInOrder.map((s) => (
              <span key={s.id} className="inline-flex items-center gap-1.5 shrink-0">
                <span
                  aria-hidden="true"
                  className="inline-block w-2.5 h-[2px]"
                  style={{ backgroundColor: s.color }}
                />
                <span style={{ fontFamily: '"JetBrains Mono", monospace' }}>
                  {s.short}
                </span>
                <span
                  className="text-text-3"
                  style={{ fontFamily: '"JetBrains Mono", monospace' }}
                >
                  {s.metrics.return >= 0 ? "+" : ""}
                  {s.metrics.return.toFixed(2)}%
                </span>
              </span>
            ))}
          </div>
        </header>
        <div className="px-4 py-3">
          <MultiStrategyEquityPane
            time={payload.time}
            series={heroSeries}
            leadId={leadId}
            syncKey="dashboard-compare"
          />
        </div>
      </div>

      <StrategyRosterPills
        available={rosterPillItems}
        selectedIds={roster.selectedIds}
        onToggle={roster.toggle}
        onRemove={roster.remove}
        canRemove={roster.canRemove}
      />

      <StrategyCardGrid count={selectedInOrder.length}>
        {selectedInOrder.map((s, i) => (
          <LeadCardChrome key={s.id} lead={i === 0}>
            <StrategyCard
              id={s.id}
              name={s.name}
              short={s.short}
              caption={`${s.kind} · ${s.short.split(" · ")[1] ?? ""}`}
              color={s.color}
              metrics={{
                return: s.metrics.return,
                sharpe: s.metrics.sharpe,
                mdd: s.metrics.mdd,
                win: s.metrics.win,
              }}
              time={payload.time}
              equity={s.equity}
              lead={i === 0}
              removable={roster.canRemove(s.id)}
              onRemove={roster.remove}
              chips={["EMA", "RSI", "MACD"]}
            />
          </LeadCardChrome>
        ))}
      </StrategyCardGrid>
    </div>
  );
}
