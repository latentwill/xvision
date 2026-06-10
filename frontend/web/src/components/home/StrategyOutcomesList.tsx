// frontend/web/src/components/home/StrategyOutcomesList.tsx
//
// Shows all strategies with their eval metrics (most-recent completed run).
// Win-threshold coloring only at n >= 10 runs.

import { Link } from "react-router-dom";
import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface StrategyOutcomesListProps {
  strategies: StrategyListItem[];
  runs: RunSummary[];
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function fmt(v: number | null | undefined, digits = 2): string {
  if (v === null || v === undefined) return "—";
  return v.toFixed(digits);
}

type WinState = "win" | "loss" | "neutral";

function getWinState(n: number, mostRecent: RunSummary | null): WinState {
  if (n < 10 || mostRecent === null) return "neutral";
  const { total_return_pct, sharpe } = mostRecent;
  if (
    total_return_pct !== null &&
    total_return_pct > 0 &&
    sharpe !== null &&
    sharpe > 1.0
  ) {
    return "win";
  }
  return "loss";
}

function rowColorClass(state: WinState): string {
  if (state === "win") {
    return "border-green-500/30 bg-green-500/5";
  }
  if (state === "loss") {
    return "border-amber-500/30 bg-amber-500/5";
  }
  return "border-border";
}

// ─── Sub-components ───────────────────────────────────────────────────────────

interface MetricCellProps {
  label: string;
  value: string;
}

function MetricCell({ label, value }: MetricCellProps) {
  return (
    <div className="flex flex-col gap-0.5 min-w-[80px]">
      <span className="text-[10px] text-muted-foreground uppercase tracking-wide">
        {label}
      </span>
      <span className="text-[13px] font-mono font-medium">{value}</span>
    </div>
  );
}

interface StrategyRowProps {
  strategy: StrategyListItem;
  completedRuns: RunSummary[];
}

function StrategyRow({ strategy, completedRuns }: StrategyRowProps) {
  const n = completedRuns.length;

  // Sort by completed_at desc, take most recent
  const sorted = [...completedRuns].sort((a, b) => {
    const ta = a.completed_at ?? "";
    const tb = b.completed_at ?? "";
    return tb.localeCompare(ta);
  });
  const mostRecent = sorted[0] ?? null;

  const winState = getWinState(n, mostRecent);
  const colorClass = rowColorClass(winState);

  return (
    <li
      data-testid={`strategy-row-${strategy.agent_id}`}
      className={`flex flex-wrap items-center gap-4 rounded-md border px-4 py-3 ${colorClass}`}
    >
      {/* Strategy name */}
      <div className="flex-1 min-w-[160px]">
        <span className="text-sm font-medium">{strategy.display_name}</span>
        {n > 0 && (
          <span className="ml-2 text-[11px] text-muted-foreground">
            {n} eval{n !== 1 ? "s" : ""}
          </span>
        )}
      </div>

      {/* Metrics */}
      {mostRecent ? (
        <div className="flex gap-5">
          <MetricCell
            label="Return"
            value={
              mostRecent.total_return_pct !== null
                ? `${fmt(mostRecent.total_return_pct)}%`
                : "—"
            }
          />
          <MetricCell
            label="Sharpe"
            value={fmt(mostRecent.sharpe)}
          />
          <MetricCell
            label="Max DD"
            value={
              mostRecent.max_drawdown_pct !== null
                ? `${fmt(mostRecent.max_drawdown_pct)}%`
                : "—"
            }
          />
        </div>
      ) : (
        <span className="text-[12px] text-muted-foreground">no evals yet</span>
      )}

      {/* Action links */}
      <div className="flex gap-3 ml-auto">
        {mostRecent ? (
          <Link
            to={`/eval-runs/${mostRecent.id}`}
            className="text-[12px] text-primary hover:underline font-medium"
          >
            View chart →
          </Link>
        ) : (
          <Link
            to="/eval-runs"
            className="text-[12px] text-primary hover:underline font-medium"
          >
            Run eval →
          </Link>
        )}
      </div>
    </li>
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

export function StrategyOutcomesList({
  strategies,
  runs,
}: StrategyOutcomesListProps) {
  // Group completed runs by strategy id
  const completedByStrategy = new Map<string, RunSummary[]>();
  for (const strategy of strategies) {
    completedByStrategy.set(strategy.agent_id, []);
  }
  for (const run of runs) {
    if (run.status !== "completed") continue;
    // The list endpoint doesn't enrich run.strategy; agent_id is the same id.
    const sid = run.strategy?.id ?? run.agent_id;
    if (!sid) continue;
    const existing = completedByStrategy.get(sid);
    if (existing) existing.push(run);
  }

  // Sort: strategies with completed runs first (by most-recent completed_at desc),
  // then no-eval strategies at end
  const sorted = [...strategies].sort((a, b) => {
    const aRuns = completedByStrategy.get(a.agent_id) ?? [];
    const bRuns = completedByStrategy.get(b.agent_id) ?? [];
    const aLatest = aRuns
      .map((r) => r.completed_at ?? "")
      .sort()
      .at(-1) ?? "";
    const bLatest = bRuns
      .map((r) => r.completed_at ?? "")
      .sort()
      .at(-1) ?? "";
    if (aLatest && !bLatest) return -1;
    if (!aLatest && bLatest) return 1;
    return bLatest.localeCompare(aLatest);
  });

  return (
    <section data-testid="strategy-outcomes-list">
      <div className="mb-3 flex items-baseline gap-2">
        <h2 className="text-sm font-semibold tracking-tight">
          Strategy outcomes
        </h2>
        <span className="text-xs text-muted-foreground">
          · most recent eval per strategy
        </span>
      </div>

      {sorted.length === 0 ? (
        <p className="text-[13px] text-muted-foreground px-1">
          No strategies configured.{" "}
          <Link to="/strategies" className="text-primary hover:underline">
            Create one →
          </Link>
        </p>
      ) : (
        <ul className="space-y-2">
          {sorted.map((strategy) => (
            <StrategyRow
              key={strategy.agent_id}
              strategy={strategy}
              completedRuns={completedByStrategy.get(strategy.agent_id) ?? []}
            />
          ))}
        </ul>
      )}
    </section>
  );
}
