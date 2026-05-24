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
import type {
  MultiStrategyEquityBundle,
  MultiStrategyBundleEntry,
} from "../types";
import { ChartsTopbar } from "../primitives/Topbar";
import {
  MultiStrategyEquityPane,
  type MultiStrategyEquitySeries,
} from "../primitives/MultiStrategyEquityPane";
import { StrategyRosterPills } from "../primitives/StrategyRosterPills";
import { StrategyCardGrid } from "../primitives/StrategyCardGrid";
import {
  StrategyCard,
  type StrategyCardMetrics,
} from "../primitives/StrategyCard";
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
  return pickItemsSelectedInOrder(strategies, selectedIds);
}

function pickItemsSelectedInOrder<T extends { id: string }>(
  items: T[],
  selectedIds: readonly string[],
): T[] {
  const order = new Map(selectedIds.map((id, i) => [id, i]));
  return items
    .filter((item) => order.has(item.id))
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

interface ComparisonArmView {
  id: string;
  name: string;
  short: string;
  title?: string;
  caption: string;
  color: string;
  metrics: StrategyCardMetrics;
  time: number[];
  equity: number[];
  chips: string[];
  dashed?: boolean;
}

interface ComparisonViewProps {
  eyebrow: string;
  headline: string;
  tagline: string;
  arms: ComparisonArmView[];
  selectedIds: readonly string[];
  heroTime: number[];
  heroSeries: MultiStrategyEquitySeries[];
  onToggle: (id: string) => void;
  onRemove: (id: string) => void;
  canRemove: (id: string) => boolean;
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
  const selected = pickItemsSelectedInOrder(arms, selection.selectedIds);
  const { time, series } = alignHeroSeries(selected);

  return (
    <ComparisonView
      eyebrow="RUN · COMPARISON"
      headline={`${selected.length} runs, one frame`}
      tagline="Roster changes update the URL. Min 2, max 10."
      arms={arms}
      selectedIds={selection.selectedIds}
      heroTime={time}
      heroSeries={series}
      onToggle={selection.toggle}
      onRemove={selection.remove}
      canRemove={(id) => selection.selectedIds.includes(id) && selection.selectedIds.length > 2}
    />
  );
}

function runToArm(
  run: ComparisonRunSummary,
  report: ComparisonReport,
  strategies: StrategyListItem[],
  fallbackColor: string,
): ComparisonArmView {
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
    name,
    short: `${name} · ${shortId(run.id)}`,
    title: `${name} · run ${run.id} · strategy ${run.agent_id}`,
    caption: `run ${shortId(run.id)} · ${shortId(run.agent_id)}`,
    color: strategy?.color ?? fallbackColor,
    metrics: {
      return: returnPct,
      sharpe: run.metrics?.sharpe ?? 0,
      mdd: run.metrics?.max_drawdown_pct ?? 0,
      win: run.metrics?.win_rate ?? 0,
    },
    time: equityPoints.map((point) => point.time),
    equity: equityPoints.map((point) => point.value),
    chips: [shortId(run.agent_id), shortId(run.id)],
  };
}

function shortId(id: string): string {
  return id.slice(0, 8);
}

function alignHeroSeries(arms: ComparisonArmView[]): {
  time: number[];
  series: MultiStrategyEquitySeries[];
} {
  const time = Array.from(new Set(arms.flatMap((arm) => arm.time))).sort(
    (a, b) => a - b,
  );
  const index = new Map(time.map((t, i) => [t, i]));
  const series = arms.map((arm) => {
    const values: Array<number | null> = Array(time.length).fill(null);
    arm.time.forEach((t, i) => {
      const idx = index.get(t);
      if (idx !== undefined) {
        values[idx] = arm.equity[i] ?? null;
      }
    });
    return {
      id: arm.id,
      label: arm.short,
      values,
      color: arm.color,
      ...(arm.dashed ? { dashed: true } : {}),
    };
  });
  return { time, series };
}

function FixtureComparisonABDashboard({ payload }: FixtureDashboardProps): ReactElement {
  const availableIds = payload.strategies.map((s) => s.id);
  const defaultSelected = availableIds.slice(0, Math.min(6, availableIds.length));

  const roster = useChart2Roster({
    available: availableIds,
    defaultSelected,
  });

  const arms = payload.strategies.map((s): ComparisonArmView => ({
    id: s.id,
    name: s.name,
    short: s.short,
    caption: `${s.kind} · ${s.short.split(" · ")[1] ?? ""}`,
    color: s.color,
    metrics: {
      return: s.metrics.return,
      sharpe: s.metrics.sharpe,
      mdd: s.metrics.mdd,
      win: s.metrics.win,
    },
    time: payload.time,
    equity: s.equity,
    chips: ["EMA", "RSI", "MACD"],
    ...(s.dashed ? { dashed: true } : {}),
  }));
  const selected = pickItemsSelectedInOrder(arms, roster.selectedIds);
  const heroSeries: MultiStrategyEquitySeries[] = selected.map((arm) => ({
    id: arm.id,
    label: arm.short,
    values: arm.equity,
    color: arm.color,
    ...(arm.dashed ? { dashed: true } : {}),
  }));

  return (
    <ComparisonView
      eyebrow="STRATEGY · COMPARISON"
      headline={`${roster.count} strategies, one frame`}
      tagline="Click a pill to add or drop. Min 2."
      arms={arms}
      selectedIds={roster.selectedIds}
      heroTime={payload.time}
      heroSeries={heroSeries}
      onToggle={roster.toggle}
      onRemove={roster.remove}
      canRemove={roster.canRemove}
    />
  );
}

function ComparisonView({
  eyebrow,
  headline,
  tagline,
  arms,
  selectedIds,
  heroTime,
  heroSeries,
  onToggle,
  onRemove,
  canRemove,
}: ComparisonViewProps): ReactElement {
  const selected = pickItemsSelectedInOrder(arms, selectedIds);
  const leadId = selected[0]?.id;

  return (
    <div className="flex flex-col gap-4">
      <ChartsTopbar
        eyebrow={eyebrow}
        headline={headline}
        tagline={tagline}
      />

      <div className="border border-border rounded-card bg-surface-card overflow-hidden">
        <header className="px-4 py-3 border-b border-border flex items-center justify-between gap-3">
          <div className="caps">Hero overlay</div>
          <div className="flex items-center gap-3 text-[11px] text-text-3 overflow-x-auto">
            {selected.map((arm) => (
              <span
                key={arm.id}
                className="inline-flex items-center gap-1.5 shrink-0"
                title={arm.title}
              >
                <span
                  aria-hidden="true"
                  className="inline-block w-2.5 h-[2px]"
                  style={{ backgroundColor: arm.color }}
                />
                <span style={{ fontFamily: '"JetBrains Mono", monospace' }}>
                  {arm.short}
                </span>
                <span className="text-text-3" style={{ fontFamily: '"JetBrains Mono", monospace' }}>
                  {arm.metrics.return >= 0 ? "+" : ""}
                  {arm.metrics.return.toFixed(2)}%
                </span>
              </span>
            ))}
          </div>
        </header>
        <div className="px-4 py-3">
          <MultiStrategyEquityPane
            time={heroTime}
            series={heroSeries}
            leadId={leadId}
            syncKey="dashboard-compare"
          />
        </div>
      </div>

      <StrategyRosterPills
        available={arms.map((arm) => ({
          id: arm.id,
          label: arm.short,
          color: arm.color,
        }))}
        selectedIds={[...selectedIds]}
        onToggle={onToggle}
        onRemove={onRemove}
        canRemove={canRemove}
      />

      <StrategyCardGrid count={selected.length}>
        {selected.map((arm, i) => (
          <LeadCardChrome key={arm.id} lead={i === 0}>
            <StrategyCard
              id={arm.id}
              name={arm.name}
              caption={arm.caption}
              color={arm.color}
              metrics={arm.metrics}
              time={arm.time}
              equity={arm.equity}
              lead={i === 0}
              removable={canRemove(arm.id)}
              onRemove={onRemove}
              chips={arm.chips}
            />
          </LeadCardChrome>
        ))}
      </StrategyCardGrid>
    </div>
  );
}
