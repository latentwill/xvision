// frontend/web/src/features/home/optimizer-summary.ts
//
// Pure aggregation selectors for the home Optimizer panel ("is the machine
// doing good work?"). Operates on the autooptimizer ladder + stats wire
// shapes; all operator-facing label decisions live in the component.

import type { MutatorScore, StatsRow } from "@/features/autooptimizer/api";

export interface LadderTotals {
  proposals: number;
  accepted: number;
  rejectedOverfit: number;
}

/** Roll the experiment-writer ladder up into total experiments proposed /
 * accepted / rejected-as-overfit. */
export function ladderTotals(scores: MutatorScore[]): LadderTotals {
  let proposals = 0;
  let accepted = 0;
  let rejectedOverfit = 0;
  for (const s of scores) {
    proposals += s.proposals;
    accepted += s.accepted;
    rejectedOverfit += s.rejected_overfit;
  }
  return { proposals, accepted, rejectedOverfit };
}

/** Top experiment writers: by avg ΔSharpe desc, then accepted desc, then
 * proposals desc. Writers with zero proposals are excluded (no evidence). */
export function topWriters(scores: MutatorScore[], n = 3): MutatorScore[] {
  return scores
    .filter((s) => s.proposals > 0)
    .sort((a, b) => {
      if (b.avg_delta_sharpe !== a.avg_delta_sharpe) {
        return b.avg_delta_sharpe - a.avg_delta_sharpe;
      }
      if (b.accepted !== a.accepted) return b.accepted - a.accepted;
      return b.proposals - a.proposals;
    })
    .slice(0, n);
}

/** Compact display name for a writer model id: strips org/path prefixes
 * ("google/gemini-3.1-flash-lite" → "gemini-3.1-flash-lite",
 * "hf.co/unsloth/Qwen3-4B-Instruct-2507-GGUF:UD-Q4_K_XL" →
 * "Qwen3-4B-Instruct-2507-GGUF:UD-Q4_K_XL"). */
export function shortModelName(model: string): string {
  const segments = model.split("/").filter((s) => s.length > 0);
  return segments.length > 0 ? segments[segments.length - 1] : model;
}

export interface CycleTrendPoint {
  cycleId: string;
  ts: string;
  kept: number;
  suspect: number;
  dropped: number;
}

/** Last `n` cycles, oldest → newest, for the kept/suspect/dropped trend. */
export function cycleTrend(stats: StatsRow[], n = 10): CycleTrendPoint[] {
  return [...stats]
    .sort((a, b) => a.ts.localeCompare(b.ts))
    .slice(-n)
    .map((r) => ({
      cycleId: r.cycle_id,
      ts: r.ts,
      kept: r.kept,
      suspect: r.suspect,
      dropped: r.dropped,
    }));
}

/** Cumulative optimizer spend: the newest finite `cum_cost_usd`. Rows can
 * carry `null` cost fields (unpriced calls) — walk backwards to the last
 * finite value. Returns null when no row has one. */
export function cumulativeSpendUsd(stats: StatsRow[]): number | null {
  const sorted = [...stats].sort((a, b) => a.ts.localeCompare(b.ts));
  for (let i = sorted.length - 1; i >= 0; i--) {
    const v = sorted[i].cum_cost_usd;
    if (v !== null && v !== undefined && Number.isFinite(v)) return v;
  }
  return null;
}

/** The most recent cycle row (by ts), or null. Drives the honest idle
 * state: "last cycle <when> — kept X · suspect Y · dropped Z". */
export function lastCycle(stats: StatsRow[]): StatsRow | null {
  if (stats.length === 0) return null;
  return [...stats].sort((a, b) => a.ts.localeCompare(b.ts))[stats.length - 1];
}
