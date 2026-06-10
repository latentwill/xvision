// frontend/web/src/components/home/StrategyLeaderboard.tsx
//
// Home strategy leaderboard (dashboard redesign §4): top strategies by
// latest completed eval as compact rows — return / Sharpe / max DD in mono
// with pos/neg tones, sample-size chip (explicit low-n warning on thin
// data), origin chip (user vs Optimizer), and eval freshness. The segmented
// awaiting-first-eval coverage line (formerly StrategyOutcomesSummary's
// footer) closes the card.

import { Link } from "react-router-dom";

import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import {
  coverageCounts,
  strategyEvalCoverage,
} from "@/features/strategies/coverage";
import {
  strategyLeaderboard,
  type LeaderboardEntry,
} from "@/features/home/leaderboard";
import { formatRelativeTime } from "@/features/home/pulse";

export interface StrategyLeaderboardProps {
  strategies: StrategyListItem[];
  runs: RunSummary[];
}

// ─── formatting ──────────────────────────────────────────────────────────────

function fmtSignedPct(v: number | null): string {
  if (v === null || !Number.isFinite(v)) return "—";
  const sign = v > 0 ? "+" : "";
  return `${sign}${v.toFixed(2)}%`;
}

function fmtNum(v: number | null): string {
  if (v === null || !Number.isFinite(v)) return "—";
  return v.toFixed(2);
}

function signedTone(v: number | null): string {
  if (v === null || !Number.isFinite(v) || v === 0) return "text-text-2";
  return v > 0 ? "text-gold" : "text-danger";
}

// ─── sub-components ──────────────────────────────────────────────────────────

function MetricCell({
  label,
  value,
  valueClass = "text-text-2",
}: {
  label: string;
  value: string;
  valueClass?: string;
}) {
  return (
    <div className="flex w-[76px] flex-col gap-0.5">
      <span className="caps">{label}</span>
      <span className={`font-mono tabular-nums text-[13px] font-medium ${valueClass}`}>
        {value}
      </span>
    </div>
  );
}

function LeaderboardRow({
  entry,
  rank,
}: {
  entry: LeaderboardEntry;
  rank: number;
}) {
  const { strategy, run, sampleSize, lowSample, origin, lastEvalAt } = entry;
  const freshness = formatRelativeTime(lastEvalAt);
  return (
    <li
      data-testid={`leaderboard-row-${strategy.agent_id}`}
      className="flex flex-wrap items-center gap-x-4 gap-y-2 px-5 py-3 hover:bg-surface-hover transition-colors"
    >
      <span className="w-4 shrink-0 font-mono tabular-nums text-[12px] text-text-4">
        {rank}
      </span>
      <div className="min-w-[150px] flex-1">
        <div className="flex flex-wrap items-center gap-2 min-w-0">
          <Link
            to={`/strategies/${strategy.agent_id}`}
            className="truncate max-w-[220px] text-[13px] font-medium text-text hover:underline"
          >
            {strategy.display_name}
          </Link>
          {origin === "optimizer" ? (
            <Pill tone="info" data-testid="origin-chip">
              Optimizer
            </Pill>
          ) : null}
          {lowSample ? (
            <Pill tone="warn" data-testid="low-sample-chip">
              low n · {sampleSize} eval{sampleSize === 1 ? "" : "s"}
            </Pill>
          ) : (
            <span className="text-[11px] text-text-4 font-mono tabular-nums">
              n={sampleSize}
            </span>
          )}
        </div>
        <div className="mt-0.5 flex items-center gap-2 text-[11px] text-text-4">
          {freshness ? <span>eval {freshness}</span> : null}
          <Link
            to={`/eval-runs/${run.id}`}
            className="hover:text-text hover:underline"
          >
            Latest eval →
          </Link>
        </div>
      </div>
      <div className="flex gap-4">
        <MetricCell
          label="Return"
          value={fmtSignedPct(run.total_return_pct)}
          valueClass={signedTone(run.total_return_pct)}
        />
        <MetricCell label="Sharpe" value={fmtNum(run.sharpe)} />
        <MetricCell
          label="Max DD"
          value={
            run.max_drawdown_pct !== null && Number.isFinite(run.max_drawdown_pct)
              ? `${fmtNum(run.max_drawdown_pct)}%`
              : "—"
          }
          valueClass={
            run.max_drawdown_pct ? "text-danger" : "text-text-2"
          }
        />
      </div>
    </li>
  );
}

// ─── main component ──────────────────────────────────────────────────────────

export function StrategyLeaderboard({
  strategies,
  runs,
}: StrategyLeaderboardProps) {
  const coverage = strategyEvalCoverage(strategies, runs);
  const entries = strategyLeaderboard(coverage, 6);
  const counts = coverageCounts(coverage);

  const awaitingText = `${counts.userAwaitingFirstEval} user ${
    counts.userAwaitingFirstEval === 1 ? "strategy" : "strategies"
  } awaiting first eval`;
  const lineageText = `${counts.optimizerLineage} optimizer-generated (evaluated in lineage)`;

  return (
    <section data-testid="strategy-leaderboard" aria-label="Strategy leaderboard">
      <Card className="p-0 overflow-hidden xvn-card-hover">
        <div className="flex items-center justify-between gap-3 px-5 pt-4 pb-3 border-b border-border-soft">
          <div className="flex items-baseline gap-2">
            <span className="text-[15px] font-medium text-text">
              Strategy leaderboard
            </span>
            <span className="text-[11px] text-text-4">
              by latest completed eval
            </span>
          </div>
          <Link
            to="/strategies"
            className="text-[12px] text-text-3 hover:text-text"
          >
            View all →
          </Link>
        </div>

        {strategies.length === 0 ? (
          <p className="px-5 py-6 text-[13px] text-text-3">
            No strategies configured.{" "}
            <Link to="/strategies" className="text-gold hover:underline">
              Create one →
            </Link>
          </p>
        ) : entries.length === 0 ? (
          <p className="px-5 py-6 text-[13px] text-text-3">
            No completed evals on this page yet.{" "}
            <Link to="/eval-runs" className="text-gold hover:underline">
              Run an eval →
            </Link>
          </p>
        ) : (
          <ol className="divide-y divide-border-soft">
            {entries.map((entry, i) => (
              <LeaderboardRow
                key={entry.strategy.agent_id}
                entry={entry}
                rank={i + 1}
              />
            ))}
          </ol>
        )}

        {/* Segmented coverage footer — same semantics as the old
            StrategyOutcomesSummary line. */}
        {counts.userAwaitingFirstEval > 0 || counts.optimizerLineage > 0 ? (
          <div
            data-testid="eval-coverage-line"
            className="border-t border-border-soft px-5 py-2.5 text-[12px] text-text-3"
          >
            {counts.userAwaitingFirstEval > 0 ? (
              <Link
                to="/eval-runs"
                className="hover:text-text hover:underline"
              >
                {awaitingText}
              </Link>
            ) : null}
            {counts.userAwaitingFirstEval > 0 && counts.optimizerLineage > 0
              ? " · "
              : null}
            {counts.optimizerLineage > 0 ? <span>{lineageText}</span> : null}
          </div>
        ) : null}
      </Card>
    </section>
  );
}
