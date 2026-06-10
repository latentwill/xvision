// frontend/web/src/components/home/StrategyOutcomesSummary.tsx
//
// Compact, capped home summary of strategy eval outcomes (CT3). Answers
// "what should I inspect first?" by surfacing only the strongest and weakest
// recent strategies (by latest completed eval) plus a count of un-evaluated
// ones. Full lists live on /strategies and /eval-runs — home must not become
// another long table. Uses existing completed eval data only; no live money.

import { Link } from "react-router-dom";
import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import {
  coverageCounts,
  strategyEvalCoverage,
} from "@/features/strategies/coverage";

export interface StrategyOutcomesSummaryProps {
  strategies: StrategyListItem[];
  runs: RunSummary[];
}

const MAX_PER_SECTION = 3;

// A strategy that has at least one completed eval, with its most-recent run.
interface EvaluatedStrategy {
  strategy: StrategyListItem;
  latest: RunSummary;
  completedCount: number;
}

function fmt(v: number | null | undefined, digits = 2): string {
  if (v === null || v === undefined) return "—";
  return v.toFixed(digits);
}

function num(v: number | null | undefined): number {
  return v === null || v === undefined ? 0 : v;
}

// Display-only "passing": positive return AND Sharpe over 1.0. Coloring is
// gated to >=3 completed evals (see SummaryRow) so we never render a verdict
// on thin data.
function isPassing(run: RunSummary): boolean {
  return (
    run.total_return_pct !== null &&
    run.total_return_pct > 0 &&
    run.sharpe !== null &&
    run.sharpe > 1.0
  );
}

// Strategies with at least one completed run visible in the supplied runs
// page (the only ones we can render metrics for). The full evaluated/origin
// accounting — including hash-keyed CLI runs and server-side eval flags —
// lives in `strategyEvalCoverage`.
function evaluatedStrategies(
  strategies: StrategyListItem[],
  runs: RunSummary[],
): EvaluatedStrategy[] {
  const out: EvaluatedStrategy[] = [];
  for (const item of strategyEvalCoverage(strategies, runs)) {
    if (item.latestRun === null) continue;
    out.push({
      strategy: item.strategy,
      latest: item.latestRun,
      completedCount: item.completedRunCount,
    });
  }
  return out;
}

interface MetricCellProps {
  label: string;
  value: string;
}

function MetricCell({ label, value }: MetricCellProps) {
  return (
    <div className="flex flex-col gap-0.5 min-w-[72px]">
      <span className="text-[10px] text-muted-foreground uppercase tracking-wide">
        {label}
      </span>
      <span className="text-[13px] font-mono font-medium">{value}</span>
    </div>
  );
}

function SummaryRow({ item }: { item: EvaluatedStrategy }) {
  const { strategy, latest, completedCount } = item;

  // Verdict coloring only once a strategy has enough completed evals to judge.
  const showVerdict = completedCount >= 3;
  const colorClass = !showVerdict
    ? "border-border"
    : isPassing(latest)
      ? "border-green-500/30 bg-green-500/5"
      : "border-amber-500/30 bg-amber-500/5";

  return (
    <li
      data-testid={`summary-row-${strategy.agent_id}`}
      className={`flex flex-wrap items-center gap-4 rounded-md border px-4 py-2.5 ${colorClass}`}
    >
      <Link
        to={`/eval-runs/${latest.id}`}
        className="flex-1 min-w-[140px] text-sm font-medium hover:underline"
      >
        {strategy.display_name}
      </Link>
      <div className="flex gap-5">
        <MetricCell
          label="Return"
          value={
            latest.total_return_pct !== null
              ? `${fmt(latest.total_return_pct)}%`
              : "—"
          }
        />
        <MetricCell label="Sharpe" value={fmt(latest.sharpe)} />
        <MetricCell
          label="Max DD"
          value={
            latest.max_drawdown_pct !== null
              ? `${fmt(latest.max_drawdown_pct)}%`
              : "—"
          }
        />
      </div>
    </li>
  );
}

function SummarySection({
  title,
  items,
}: {
  title: string;
  items: EvaluatedStrategy[];
}) {
  return (
    <div>
      <h3 className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
        {title}
      </h3>
      <ul className="space-y-2">
        {items.map((item) => (
          <SummaryRow key={item.strategy.agent_id} item={item} />
        ))}
      </ul>
    </div>
  );
}

function SummaryHeader() {
  return (
    <div className="mb-3 flex items-baseline gap-2">
      <h2 className="text-sm font-semibold tracking-tight">Strategy outcomes</h2>
      <span className="text-xs text-muted-foreground">
        · latest completed evals
      </span>
    </div>
  );
}

export function StrategyOutcomesSummary({
  strategies,
  runs,
}: StrategyOutcomesSummaryProps) {
  // No strategies at all → direct the operator to create one first. Prompting
  // an eval here would be a dead end (there is nothing to evaluate yet).
  if (strategies.length === 0) {
    return (
      <section data-testid="strategy-outcomes-summary">
        <SummaryHeader />
        <p className="px-1 text-[13px] text-muted-foreground">
          No strategies configured.{" "}
          <Link to="/strategies" className="text-primary hover:underline">
            Create one →
          </Link>
        </p>
      </section>
    );
  }

  const evaluated = evaluatedStrategies(strategies, runs);

  // Strongest: highest return, Sharpe as tie-break.
  const strongest = [...evaluated]
    .sort((a, b) => {
      const byReturn = num(b.latest.total_return_pct) - num(a.latest.total_return_pct);
      if (byReturn !== 0) return byReturn;
      return num(b.latest.sharpe) - num(a.latest.sharpe);
    })
    .slice(0, MAX_PER_SECTION);

  const strongestIds = new Set(strongest.map((e) => e.strategy.agent_id));

  // Weakest: lowest return, deeper drawdown as tie-break. Excludes anything
  // already shown as strongest so a strategy never appears in both sections.
  const weakest = [...evaluated]
    .filter((e) => !strongestIds.has(e.strategy.agent_id))
    .sort((a, b) => {
      const byReturn = num(a.latest.total_return_pct) - num(b.latest.total_return_pct);
      if (byReturn !== 0) return byReturn;
      return num(b.latest.max_drawdown_pct) - num(a.latest.max_drawdown_pct);
    })
    .slice(0, MAX_PER_SECTION);

  // Segmented coverage line (replaces the old "N strategies have no
  // completed evals yet" nag): user strategies genuinely awaiting a first
  // eval are actionable (link to /eval-runs); optimizer-generated strategies
  // are evaluated inside optimizer cycles and are informational only.
  const counts = coverageCounts(strategyEvalCoverage(strategies, runs));
  const awaitingText = `${counts.userAwaitingFirstEval} user ${
    counts.userAwaitingFirstEval === 1 ? "strategy" : "strategies"
  } awaiting first eval`;
  const lineageText = `${counts.optimizerLineage} optimizer-generated (evaluated in lineage)`;

  return (
    <section data-testid="strategy-outcomes-summary">
      <SummaryHeader />

      {evaluated.length === 0 ? (
        <p className="px-1 text-[13px] text-muted-foreground">
          No completed evals yet.{" "}
          <Link to="/eval-runs" className="text-primary hover:underline">
            Run an eval →
          </Link>
        </p>
      ) : (
        <div className="space-y-4">
          {strongest.length > 0 && (
            <SummarySection title="Strongest recent" items={strongest} />
          )}
          {weakest.length > 0 && (
            <SummarySection title="Needs review" items={weakest} />
          )}
        </div>
      )}

      <div className="mt-3 flex items-center justify-between gap-3 text-[12px]">
        {counts.userAwaitingFirstEval > 0 || counts.optimizerLineage > 0 ? (
          <span
            data-testid="eval-coverage-line"
            className="text-muted-foreground"
          >
            {counts.userAwaitingFirstEval > 0 ? (
              <Link
                to="/eval-runs"
                className="hover:text-foreground hover:underline"
              >
                {awaitingText}
              </Link>
            ) : null}
            {counts.userAwaitingFirstEval > 0 && counts.optimizerLineage > 0
              ? " · "
              : null}
            {counts.optimizerLineage > 0 ? <span>{lineageText}</span> : null}
          </span>
        ) : (
          <span />
        )}
        <Link
          to="/strategies"
          className="font-medium text-primary hover:underline"
        >
          View all strategies →
        </Link>
      </div>
    </section>
  );
}
