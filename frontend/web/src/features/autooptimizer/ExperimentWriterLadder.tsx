// ExperimentWriterLadder — mutator performance scoreboard.
// Operator-facing name: "Experiment writer ladder" (Mutator → "Experiment writer",
// per the terminology lock at docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md).
//
// Columns: Model | Proposals | Accepted | Rejected | Acceptance Rate | Avg ΔSharpe
// Sorted by acceptance_rate descending.

import { Card, CardHeader } from "@/components/primitives/Card";
import { useLadder, type MutatorScore } from "./api";

export function ExperimentWriterLadder() {
  const { data: scores, isPending, isError } = useLadder();

  if (isPending) {
    return (
      <div className="text-[13px] text-text-3 py-4">
        Loading experiment-writer ladder…
      </div>
    );
  }

  if (isError) {
    return (
      <div className="text-[13px] text-red-500 py-4">
        Failed to load ladder data.
      </div>
    );
  }

  const sorted = scores
    ? [...scores].sort((a, b) => acceptanceRate(b) - acceptanceRate(a))
    : [];

  return (
    <Card>
      <CardHeader title="Experiment writer ladder" />
      {sorted.length === 0 ? (
        <div className="px-5 pb-5 text-[13px] text-text-3">No data yet.</div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-[13px] border-collapse">
            <thead>
              <tr className="border-b border-border">
                <th className="text-left font-medium text-text-3 px-5 py-3">
                  Model
                </th>
                <th className="text-right font-medium text-text-3 px-4 py-3">
                  Proposals
                </th>
                <th className="text-right font-medium text-text-3 px-4 py-3">
                  Accepted
                </th>
                <th className="text-right font-medium text-text-3 px-4 py-3">
                  Rejected
                </th>
                <th className="text-right font-medium text-text-3 px-4 py-3">
                  Accept %
                </th>
                <th className="text-right font-medium text-text-3 px-5 py-3">
                  Avg ΔSharpe
                </th>
              </tr>
            </thead>
            <tbody>
              {sorted.map((row, i) => (
                <LadderRow key={i} row={row} rank={i + 1} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}

function LadderRow({ row, rank }: { row: MutatorScore; rank: number }) {
  const rate = acceptanceRate(row);
  const ratePct = (rate * 100).toFixed(1);
  const deltaSharpe = row.avg_delta_sharpe.toFixed(3);
  const deltaCls =
    row.avg_delta_sharpe > 0
      ? "text-green-600 dark:text-green-400"
      : row.avg_delta_sharpe < 0
        ? "text-red-500 dark:text-red-400"
        : "text-text-3";

  return (
    <tr className="border-b border-border last:border-0 hover:bg-surface-elev/40">
      <td className="px-5 py-3">
        <div className="flex items-center gap-2">
          <span className="text-[11px] text-text-3 w-5 shrink-0">
            #{rank}
          </span>
          <div>
            <div className="text-text font-medium">{row.model}</div>
            <div className="text-[11px] text-text-3">
              {row.provider} · v{row.prompt_version}
            </div>
          </div>
        </div>
      </td>
      <td className="px-4 py-3 text-right text-text tabular-nums">
        {row.proposals}
      </td>
      <td className="px-4 py-3 text-right text-green-600 dark:text-green-400 tabular-nums">
        {row.accepted}
      </td>
      <td className="px-4 py-3 text-right text-text-3 tabular-nums">
        {row.rejected_overfit}
      </td>
      <td className="px-4 py-3 text-right tabular-nums">
        <span
          className={
            rate >= 0.5
              ? "text-green-600 dark:text-green-400"
              : rate >= 0.25
                ? "text-text"
                : "text-text-3"
          }
        >
          {ratePct}%
        </span>
      </td>
      <td className={`px-5 py-3 text-right tabular-nums ${deltaCls}`}>
        {row.avg_delta_sharpe >= 0 ? "+" : ""}
        {deltaSharpe}
      </td>
    </tr>
  );
}

function acceptanceRate(row: MutatorScore): number {
  if (row.proposals === 0) return 0;
  return row.accepted / row.proposals;
}
