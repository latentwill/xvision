import { useMemo } from "react";
import { Link } from "react-router-dom";
import { ChartFrame } from "../primitives/ChartFrame";
import { MultiStrategyEquityPane } from "../primitives/MultiStrategyEquityPane";
import { StrategyCard, type StrategyCardArm } from "../primitives/StrategyCard";
import { StrategyCardGrid } from "../primitives/StrategyCardGrid";
import { StrategyRosterPills } from "../primitives/StrategyRosterPills";
import { useChart2Theme } from "../hooks/useChart2Theme";
import type { CompareSelection } from "../hooks/useCompareSelection";
import type { DrawdownPoint, EquityPoint } from "../types";
import type { StrategyListItem } from "@/api/strategies";
import type { ComparisonReport, ComparisonRunSummary } from "@/api/types.gen";
import { displayStrategyName, shortId } from "@/lib/run-display";

type Props = {
  report: ComparisonReport;
  selection: CompareSelection;
  strategies?: StrategyListItem[];
};

export function ComparisonABDashboard({
  report,
  selection,
  strategies = [],
}: Props) {
  const theme = useChart2Theme();
  const arms = useMemo(
    () => buildArms(report, strategies, theme.compare.palette),
    [report, strategies, theme.compare.palette],
  );
  const selectedSet = new Set(selection.selectedIds);
  const selectedArms = arms.filter((arm) => selectedSet.has(arm.runId));

  return (
    <div className="space-y-4">
      <div className="flex flex-col gap-1">
        <div className="font-sans font-medium text-[30px] leading-tight text-text">
          {selectedArms.length} strategies, one frame
        </div>
        <div className="text-[13px] text-text-3">
          Latest compare payload from selected run ids.
        </div>
      </div>

      <ChartFrame title="Equity overlay" range="All" onRange={() => undefined}>
        <MultiStrategyEquityPane
          arms={selectedArms.map((arm) => ({
            id: arm.runId,
            label: arm.label,
            color: arm.color,
            equity: arm.equity,
            returnPct: arm.metrics.returnPct,
          }))}
          height={320}
        />
      </ChartFrame>

      <StrategyRosterPills
        items={arms.map((arm) => ({
          id: arm.runId,
          label: arm.label,
          color: arm.color,
          active: selectedSet.has(arm.runId),
        }))}
        canRemove={selection.count > 2}
        onToggle={selection.toggle}
        onRemove={selection.remove}
        onAdd={selection.add}
      />

      <StrategyCardGrid count={selectedArms.length}>
        {selectedArms.map((arm, idx) => (
          <StrategyCard
            key={arm.runId}
            arm={arm}
            lead={idx === 0}
            removable={selection.count > 2}
            onRemove={selection.remove}
            onSetLead={selection.setLead}
          />
        ))}
      </StrategyCardGrid>

      <div className="text-[12px] text-text-3">
        <Link className="hover:text-text hover:underline" to={`/eval-runs/compare?ids=${selection.selectedIds.join(",")}`}>
          Open run-centric comparison
        </Link>
      </div>
    </div>
  );
}

function buildArms(
  report: ComparisonReport,
  strategies: StrategyListItem[],
  palette: readonly string[],
): StrategyCardArm[] {
  const curveByRun = new Map(report.equity_curves.map((curve) => [curve.run_id, curve] as const));
  return report.runs.map((run, idx) => {
    const strategy = strategies.find((s) => s.agent_id === run.agent_id);
    const equity = (curveByRun.get(run.id)?.samples ?? []).map((sample) => ({
      time: Date.parse(sample.timestamp) / 1000,
      value: sample.equity_usd,
    }));
    return {
      id: run.agent_id,
      runId: run.id,
      label: labelForRun(run, strategies),
      shortId: shortId(run.id, 8),
      kind: run.mode,
      color: strategy?.color ?? palette[idx % palette.length],
      status: run.status,
      equity,
      drawdown: drawdownFromEquity(equity),
      metrics: {
        returnPct: run.metrics?.total_return_pct ?? null,
        sharpe: run.metrics?.sharpe ?? null,
        maxDrawdownPct: run.metrics?.max_drawdown_pct ?? null,
        decisions: run.metrics?.n_decisions ?? null,
      },
    };
  });
}

function labelForRun(run: ComparisonRunSummary, strategies: StrategyListItem[]): string {
  const name = run.strategy_name?.trim();
  return name || displayStrategyName(run.agent_id, strategies);
}

function drawdownFromEquity(points: EquityPoint[]): DrawdownPoint[] {
  let peak = Number.NEGATIVE_INFINITY;
  return points.map((point) => {
    peak = Math.max(peak, point.value);
    const value = peak > 0 ? ((point.value - peak) / peak) * 100 : 0;
    return { time: point.time, value };
  });
}
