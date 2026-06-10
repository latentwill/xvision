// frontend/web/src/features/home/pulse.ts
//
// Pure selectors for the home Pulse band (dashboard redesign, audit F3).
// Components consume these — never raw API joins — so the hero's "which run
// do we show, and what do its numbers mean" logic stays unit-tested.

import type { RunSummary } from "@/api/types.gen";

export interface SeriesPoint {
  time: number;
  value: number;
}

/** Aligned uPlot-ready columns for the hero chart: equity return % plus the
 * client-side drawdown band (running max − equity, plotted as ≤ 0 values).
 * Non-finite equity samples become `null` gaps so the columns stay aligned
 * with the shared time axis (and never feed NaN into canvas paths — F8). */
export interface PulseChartSeries {
  time: number[];
  equity: (number | null)[];
  drawdown: (number | null)[];
}

/** Compute the drawdown band from an equity (return %) curve: running max
 * minus equity, expressed as negative percentage points. Skips non-finite
 * samples without advancing the running max. */
export function drawdownFromEquity(points: SeriesPoint[]): SeriesPoint[] {
  let runningMax = Number.NEGATIVE_INFINITY;
  const out: SeriesPoint[] = [];
  for (const p of points) {
    if (!Number.isFinite(p.time) || !Number.isFinite(p.value)) continue;
    if (p.value > runningMax) runningMax = p.value;
    out.push({ time: p.time, value: p.value - runningMax });
  }
  return out;
}

/** Build the aligned hero-chart columns from a return-% equity curve. */
export function pulseChartSeries(points: SeriesPoint[]): PulseChartSeries {
  const time: number[] = [];
  const equity: (number | null)[] = [];
  const drawdown: (number | null)[] = [];
  let runningMax = Number.NEGATIVE_INFINITY;
  for (const p of points) {
    if (!Number.isFinite(p.time)) continue;
    time.push(p.time);
    if (!Number.isFinite(p.value)) {
      equity.push(null);
      drawdown.push(null);
      continue;
    }
    if (p.value > runningMax) runningMax = p.value;
    equity.push(p.value);
    drawdown.push(p.value - runningMax);
  }
  return { time, equity, drawdown };
}

/** A run the home hero can chart: not live-money (no historical bars) and
 * scenario-backed. Mirrors the old home.tsx isChartableRun guard. */
export function isChartableRun(run: RunSummary): boolean {
  return run.mode !== "live" && run.scenario_id.trim().length > 0;
}

/**
 * Pick the hero run: the most recently completed chartable run that carries
 * outcome metrics (a "meaningful" run), falling back to the most recently
 * completed chartable run without metrics, then null. Sort key is
 * `completed_at` (RFC3339 strings compare lexicographically).
 */
export function pickHeroRun(runs: RunSummary[]): RunSummary | null {
  const completed = runs
    .filter((r) => r.status === "completed" && isChartableRun(r))
    .sort((a, b) => (b.completed_at ?? "").localeCompare(a.completed_at ?? ""));
  const withMetrics = completed.find(
    (r) => r.total_return_pct !== null && Number.isFinite(r.total_return_pct),
  );
  return withMetrics ?? completed[0] ?? null;
}

export interface EvalThroughput {
  completed: number;
  inflight: number;
}

/** Completed vs in-flight (queued/running) eval counts for the KPI rail. */
export function evalThroughput(runs: RunSummary[]): EvalThroughput {
  let completed = 0;
  let inflight = 0;
  for (const r of runs) {
    if (r.status === "completed") completed += 1;
    else if (r.status === "queued" || r.status === "running") inflight += 1;
  }
  return { completed, inflight };
}

/**
 * Chronological series of a per-run metric across recently completed runs —
 * feeds the KPI micro-sparklines. Oldest → newest, capped at `n` (newest
 * kept). Non-finite/null metrics are skipped.
 */
export function recentMetricSeries(
  runs: RunSummary[],
  pick: (run: RunSummary) => number | null,
  n = 24,
): number[] {
  const completed = runs
    .filter((r) => r.status === "completed")
    .sort((a, b) => (a.completed_at ?? "").localeCompare(b.completed_at ?? ""));
  const out: number[] = [];
  for (const r of completed) {
    const v = pick(r);
    if (v !== null && Number.isFinite(v)) out.push(v);
  }
  return out.slice(-n);
}

/** Latest `completed_at` across the supplied runs — the freshness stamp. */
export function latestCompletionStamp(runs: RunSummary[]): string | null {
  let latest: string | null = null;
  for (const r of runs) {
    if (r.status !== "completed" || !r.completed_at) continue;
    if (latest === null || r.completed_at.localeCompare(latest) > 0) {
      latest = r.completed_at;
    }
  }
  return latest;
}

/** Compact relative-time stamp: "just now", "5m ago", "3h ago", "2d ago".
 * Returns "" for null/invalid input (callers omit the stamp). */
export function formatRelativeTime(
  iso: string | null | undefined,
  nowMs: number = Date.now(),
): string {
  if (!iso) return "";
  const t = new Date(iso).getTime();
  if (!Number.isFinite(t)) return "";
  const deltaS = Math.max(0, Math.floor((nowMs - t) / 1000));
  if (deltaS < 60) return "just now";
  const mins = Math.floor(deltaS / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}
