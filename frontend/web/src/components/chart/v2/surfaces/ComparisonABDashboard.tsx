/**
 * ComparisonABDashboard — surface for `/charts/compare` (B2).
 *
 * Production route mode renders real eval `compare_runs(ids)` reports.
 * Chart Lab mode still accepts the B2 fixture bundle so design previews
 * remain backend-independent.
 */
import type { ReactElement } from "react";

import type { ComparisonReport, ComparisonRunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import type { CompareSelection } from "../hooks/useCompareSelection";
import type { MultiStrategyEquityBundle, MultiStrategyBundleEntry } from "../types";
import { ChartsTopbar } from "../primitives/Topbar";
import {
  MultiStrategyEquityPane,
  type MultiStrategyEquitySeries,
} from "../primitives/MultiStrategyEquityPane";
import { UplotCompareOverlayPane } from "../primitives/UplotCompareOverlayPane";
import { StrategyRosterPills } from "../primitives/StrategyRosterPills";
import { StrategyCardGrid } from "../primitives/StrategyCardGrid";
import { StrategyCard } from "../primitives/StrategyCard";
import { LeadCardChrome } from "../primitives/LeadCardChrome";
import { useChart2Roster } from "../hooks/useChart2Roster";
import { useChart2Theme } from "../hooks/useChart2Theme";

type ReportDashboardProps = {
  report: ComparisonReport;
  selection: CompareSelection;
  strategies: StrategyListItem[];
  payload?: never;
};

type FixtureDashboardProps = {
  payload: MultiStrategyEquityBundle;
  report?: never;
  selection?: never;
  strategies?: never;
};

export type ComparisonABDashboardProps = ReportDashboardProps | FixtureDashboardProps;

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

export function ComparisonABDashboard(props: ComparisonABDashboardProps): ReactElement {
  if (props.payload !== undefined) {
    return <FixtureComparisonABDashboard payload={props.payload} />;
  }
  return (
    <ReportComparisonABDashboard
      report={props.report}
      selection={props.selection}
      strategies={props.strategies}
    />
  );
}

function ReportComparisonABDashboard({
  report,
  selection,
  strategies,
}: ReportDashboardProps): ReactElement {
  const theme = useChart2Theme();
  const arms = report.runs.map((run, idx) =>
    runToArm(run, report, strategies, theme.compare.palette[idx % theme.compare.palette.length]),
  );
  const selected = arms.filter((arm) => selection.selectedIds.includes(arm.id));
  const leadId = selected[0]?.id;

  return (
    <div className="flex flex-col gap-4">
      <ChartsTopbar
        eyebrow="RUN · COMPARISON"
        headline={`${selected.length} runs, one frame`}
        tagline="Roster changes update the URL. Min 2, max 10."
      />

      <div className="border border-border rounded-card bg-surface-card overflow-hidden">
        <header className="px-4 py-3 border-b border-border flex items-center justify-between gap-3">
          <div className="caps">Hero overlay</div>
          <div className="flex items-center gap-3 text-[11px] text-text-3 overflow-x-auto">
            {selected.map((arm) => (
              <span key={arm.id} className="inline-flex items-center gap-1.5 shrink-0" title={arm.title}>
                <span
                  aria-hidden="true"
                  className="inline-block w-2.5 h-[2px]"
                  style={{ backgroundColor: arm.color }}
                />
                <span style={{ fontFamily: '"JetBrains Mono", monospace' }}>
                  {arm.short}
                </span>
                <span className="text-text-3" style={{ fontFamily: '"JetBrains Mono", monospace' }}>
                  {arm.returnPct >= 0 ? "+" : ""}
                  {arm.returnPct.toFixed(2)}%
                </span>
              </span>
            ))}
          </div>
        </header>
        <div className="px-4 py-3">
          <UplotCompareOverlayPane
            arms={selected.map((arm) => ({
              id: arm.id,
              label: arm.short,
              color: arm.color,
              equity: arm.equityPoints,
            }))}
            height={280}
          />
        </div>
      </div>

      <StrategyRosterPills
        available={arms.map((arm) => ({
          id: arm.id,
          label: arm.short,
          color: arm.color,
        }))}
        selectedIds={selection.selectedIds}
        onToggle={selection.toggle}
        onRemove={selection.remove}
        canRemove={(id) => selection.selectedIds.includes(id) && selection.selectedIds.length > 2}
      />

      <StrategyCardGrid count={selected.length}>
        {selected.map((arm, i) => (
          <LeadCardChrome key={arm.id} lead={arm.id === leadId}>
            <StrategyCard
              id={arm.id}
              name={arm.name}
              short={arm.short}
              caption={arm.caption}
              color={arm.color}
              metrics={arm.metrics}
              time={arm.time}
              equity={arm.equity}
              lead={i === 0}
              removable={selection.selectedIds.length > 2}
              onRemove={selection.remove}
              chips={[shortId(arm.run.agent_id), shortId(arm.id)]}
            />
          </LeadCardChrome>
        ))}
      </StrategyCardGrid>
    </div>
  );
}

function runToArm(
  run: ComparisonRunSummary,
  report: ComparisonReport,
  strategies: StrategyListItem[],
  fallbackColor: string,
) {
  const strategy = strategies.find((item) => item.agent_id === run.agent_id);
  const name = run.strategy_name?.trim() || strategy?.display_name || run.agent_id;
  const curve = report.equity_curves.find((item) => item.run_id === run.id);
  const equityPoints = (curve?.samples ?? []).map((sample) => ({
    time: Date.parse(sample.timestamp) / 1000,
    value: sample.equity_usd,
  }));
  const returnPct = run.metrics?.total_return_pct ?? 0;
  return {
    id: run.id,
    run,
    name,
    short: `${name} · ${shortId(run.id)}`,
    title: `${name} · run ${run.id} · strategy ${run.agent_id}`,
    caption: `run ${shortId(run.id)} · ${shortId(run.agent_id)}`,
    color: strategy?.color ?? fallbackColor,
    returnPct,
    metrics: {
      return: returnPct,
      sharpe: run.metrics?.sharpe ?? 0,
      mdd: run.metrics?.max_drawdown_pct ?? 0,
      win: run.metrics?.win_rate ?? 0,
    },
    time: equityPoints.map((point) => point.time),
    equity: equityPoints.map((point) => point.value),
    equityPoints,
  };
}

function shortId(id: string): string {
  return id.slice(0, 8);
}

function FixtureComparisonABDashboard({
  payload,
}: FixtureDashboardProps): ReactElement {
  const availableIds = payload.strategies.map((s) => s.id);
  const defaultSelected = availableIds.slice(0, Math.min(6, availableIds.length));

  const roster = useChart2Roster({
    available: availableIds,
    defaultSelected,
  });

  const selectedInOrder = pickSelectedInOrder(payload.strategies, roster.selectedIds);
  const leadId = selectedInOrder[0]?.id;

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
                <span className="text-text-3" style={{ fontFamily: '"JetBrains Mono", monospace' }}>
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
