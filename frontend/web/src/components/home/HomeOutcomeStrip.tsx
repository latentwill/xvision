// frontend/web/src/components/home/HomeOutcomeStrip.tsx
//
// Compact home outcome strip (CT4). Uses only data available today —
// completed eval counts and latest per-strategy return/Sharpe — to answer
// "how are things doing?" at a glance. It must NOT fake live-money metrics:
// no PnL, deployed capital, live drawdown, daily-loss buffer, or run mode.
// Live-trading metrics are gated behind the live-trading backend contract (CT5).

import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import { isInflightRunStatus } from "@/lib/run-status";

export interface HomeOutcomeStripProps {
  strategies: StrategyListItem[];
  runs: RunSummary[];
}

// Most-recent completed run per strategy. Join key: run.strategy.id when the
// server enriched the summary, falling back to run.agent_id — the list
// endpoint (/api/eval/runs) does not enrich, so without the fallback the
// strip renders "—" while completed runs exist.
export function latestCompletedRunsByStrategy(runs: RunSummary[]): RunSummary[] {
  const byStrategy = new Map<string, RunSummary>();
  for (const run of runs) {
    if (run.status !== "completed") continue;
    const strategyId = run.strategy?.id ?? run.agent_id;
    if (!strategyId) continue;
    const existing = byStrategy.get(strategyId);
    const currentCompletedAt = run.completed_at ?? "";
    const existingCompletedAt = existing?.completed_at ?? "";
    if (!existing || currentCompletedAt.localeCompare(existingCompletedAt) > 0) {
      byStrategy.set(strategyId, run);
    }
  }
  return [...byStrategy.values()];
}

export function median(values: number[]): number | null {
  const sorted = values
    .filter((value) => Number.isFinite(value))
    .sort((a, b) => a - b);
  if (sorted.length === 0) return null;
  const mid = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 1) return sorted[mid];
  return (sorted[mid - 1] + sorted[mid]) / 2;
}

function fmtPct(v: number | null): string {
  if (v === null) return "—";
  return `${v.toFixed(2)}%`;
}

function fmtNum(v: number | null): string {
  if (v === null) return "—";
  return v.toFixed(2);
}

interface CellProps {
  label: string;
  value: string;
  testId: string;
  tone?: "pos" | "neg";
}

function Cell({ label, value, testId, tone }: CellProps) {
  const toneClass = tone === "pos" ? "text-pos" : tone === "neg" ? "text-neg" : "";
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10px] uppercase tracking-wide text-muted-foreground">
        {label}
      </span>
      <span
        data-testid={testId}
        className={`text-[15px] font-mono font-semibold tabular-nums ${toneClass}`}
      >
        {value}
      </span>
    </div>
  );
}

export function HomeOutcomeStrip({ runs }: HomeOutcomeStripProps) {
  const completedCount = runs.filter((r) => r.status === "completed").length;
  const inflightCount = runs.filter((r) => isInflightRunStatus(r.status)).length;

  const latest = latestCompletedRunsByStrategy(runs);

  const returns = latest
    .map((r) => r.total_return_pct)
    .filter((v): v is number => v !== null && Number.isFinite(v));
  const bestReturn = returns.length > 0 ? Math.max(...returns) : null;

  const sharpes = latest
    .map((r) => r.sharpe)
    .filter((v): v is number => v !== null && Number.isFinite(v));
  const medianSharpe = median(sharpes);

  return (
    <section
      data-testid="home-outcome-strip"
      className="flex flex-wrap items-center gap-x-10 gap-y-4 rounded-md border border-border bg-card px-5 py-3"
    >
      <Cell
        label="Completed evals"
        value={String(completedCount)}
        testId="home-outcome-completed"
      />
      <Cell
        label="In flight"
        value={String(inflightCount)}
        testId="home-outcome-inflight"
      />
      <Cell
        label="Best return"
        value={fmtPct(bestReturn)}
        testId="home-outcome-best-return"
        tone={bestReturn === null ? undefined : bestReturn >= 0 ? "pos" : "neg"}
      />
      <Cell
        label="Median Sharpe"
        value={fmtNum(medianSharpe)}
        testId="home-outcome-median-sharpe"
      />
    </section>
  );
}
