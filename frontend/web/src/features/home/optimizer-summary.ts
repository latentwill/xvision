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

// ─── zn2: FE-derivable digest slices ─────────────────────────────────────────
//
// All three are honest, derivable facts off the existing
// `GET /api/autooptimizer/stats?since=` rows (kept/suspect/dropped +
// best_delta_holdout + cost_usd). No live-money / P&L / capital is involved:
// these are counts, a Sharpe-style holdout delta, and a per-cycle token cost.

const THIRTY_DAYS_MS = 30 * 24 * 60 * 60 * 1000;

/** A 30-day rolling acceptance rate plus a degradation signal.
 *
 * `rate` = kept / (kept + suspect + dropped) over the in-window cycles, or
 * `null` when no in-window cycle produced any candidates (0 denominator).
 *
 * `degraded` compares the recent half of the window to the older half: it is
 * true only when the recent half's acceptance rate has dropped meaningfully
 * (≥ 15 percentage points) below the older half AND both halves carry enough
 * candidates to be evidence. This is the signal that drives the warn/gold tone
 * in the strip — e.g. when a sabotaged null-result honesty check correctly
 * degraded the machine's accept rate. */
export interface RollingAcceptance {
  rate: number | null;
  kept: number;
  total: number;
  degraded: boolean;
}

/** Minimum candidates per half before a degradation signal is trustworthy. */
const DEGRADATION_MIN_HALF_TOTAL = 4;
/** Acceptance-rate drop (recent vs older half) that counts as degradation. */
const DEGRADATION_DROP = 0.15;

export function rollingAcceptanceRate(
  stats: StatsRow[],
  opts?: { now?: Date },
): RollingAcceptance {
  const nowMs = (opts?.now ?? new Date()).getTime();
  const cutoffMs = nowMs - THIRTY_DAYS_MS;

  const inWindow = stats
    .filter((r) => {
      const t = Date.parse(r.ts);
      return Number.isFinite(t) && t >= cutoffMs;
    })
    .sort((a, b) => a.ts.localeCompare(b.ts));

  let kept = 0;
  let total = 0;
  for (const r of inWindow) {
    kept += r.kept;
    total += r.kept + r.suspect + r.dropped;
  }

  const rate = total > 0 ? kept / total : null;

  // Degradation: split the in-window cycles into older / recent halves and
  // compare acceptance rates. Needs evidence on both sides.
  let degraded = false;
  if (inWindow.length >= 2) {
    const mid = Math.floor(inWindow.length / 2);
    const older = inWindow.slice(0, mid);
    const recent = inWindow.slice(mid);
    const rateOf = (rows: StatsRow[]) => {
      let k = 0;
      let t = 0;
      for (const r of rows) {
        k += r.kept;
        t += r.kept + r.suspect + r.dropped;
      }
      return t > 0 ? { rate: k / t, total: t } : null;
    };
    const o = rateOf(older);
    const rec = rateOf(recent);
    if (
      o &&
      rec &&
      o.total >= DEGRADATION_MIN_HALF_TOTAL &&
      rec.total >= DEGRADATION_MIN_HALF_TOTAL &&
      o.rate - rec.rate >= DEGRADATION_DROP
    ) {
      degraded = true;
    }
  }

  return { rate, kept, total, degraded };
}

/** The best holdout-window Sharpe delta across the supplied cycles: the max of
 * the finite `best_delta_holdout` values. Returns null when no cycle carries a
 * finite delta (e.g. all unpriced / pre-gate). Can be negative when the best
 * candidate still underperformed its baseline on the untouched window. */
export function bestHoldoutDelta(stats: StatsRow[]): number | null {
  let best: number | null = null;
  for (const r of stats) {
    const v = r.best_delta_holdout;
    if (v !== null && v !== undefined && Number.isFinite(v)) {
      best = best === null ? v : Math.max(best, v);
    }
  }
  return best;
}

/** Client-side cost-anomaly flag: the newest cycle's `cost_usd` vs the median
 * of the trailing (all prior) cycles. Flags `anomalous` when the current cost
 * exceeds the trailing median by ≥ the anomaly factor AND the median is > 0
 * (so a zero-cost baseline can't blow up the ratio). With no trailing history
 * the result is non-anomalous with a null median. */
export interface CostAnomaly {
  anomalous: boolean;
  currentUsd: number | null;
  medianUsd: number | null;
}

/** Current cost must be ≥ this multiple of the trailing median to flag. */
const COST_ANOMALY_FACTOR = 2;

function median(values: number[]): number | null {
  const finite = values.filter((v) => Number.isFinite(v)).sort((a, b) => a - b);
  if (finite.length === 0) return null;
  const mid = Math.floor(finite.length / 2);
  return finite.length % 2 === 0
    ? (finite[mid - 1] + finite[mid]) / 2
    : finite[mid];
}

export function costAnomaly(stats: StatsRow[]): CostAnomaly {
  if (stats.length === 0) {
    return { anomalous: false, currentUsd: null, medianUsd: null };
  }
  const sorted = [...stats].sort((a, b) => a.ts.localeCompare(b.ts));
  const current = sorted[sorted.length - 1].cost_usd;
  const trailing = sorted.slice(0, -1).map((r) => r.cost_usd);
  const medianUsd = median(trailing);

  const currentUsd = Number.isFinite(current) ? current : null;

  const anomalous =
    currentUsd !== null &&
    medianUsd !== null &&
    medianUsd > 0 &&
    currentUsd >= medianUsd * COST_ANOMALY_FACTOR;

  return { anomalous, currentUsd, medianUsd };
}
