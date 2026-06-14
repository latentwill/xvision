// frontend/web/src/features/home/pulse.ts
//
// Pure selectors for the home Pulse band (dashboard redesign, audit F3).
// Components consume these — never raw API joins — so the hero's "which run
// do we show, and what do its numbers mean" logic stays unit-tested.

import type { RunChartPayload, RunSummary } from "@/api/types.gen";
import type { V2Marker } from "@/components/chart/v2/types";

export interface SeriesPoint {
  time: number;
  value: number;
}

/** QA #1: buy/sell trade markers for the hero equity overlay. Maps the run
 *  chart payload's trades to V2Markers (kind/time/price). The equity overlay
 *  anchors to the curve, so the absolute fill price is informational only. */
export function tradeMarkersFromPayload(
  payload: RunChartPayload | undefined,
): V2Marker[] {
  const trades = payload?.markers?.trades ?? [];
  return trades.map((t) => ({
    kind: t.side === "Buy" ? "buy" : "sell",
    time: t.time,
    price: t.price,
  }));
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

export function latestEvaluatedStrategyRuns(
  runs: RunSummary[],
  limit = 5,
): RunSummary[] {
  const out: RunSummary[] = [];
  const seen = new Set<string>();
  const completed = runs
    .filter((r) => r.status === "completed" && isChartableRun(r))
    .sort((a, b) => (b.completed_at ?? "").localeCompare(a.completed_at ?? ""));

  for (const run of completed) {
    const strategyId = run.agent_id?.trim() || run.id;
    if (seen.has(strategyId)) continue;
    seen.add(strategyId);
    out.push(run);
    if (out.length >= limit) break;
  }

  return out;
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

// ─── pulse view switcher ─────────────────────────────────────────────────────

export const PULSE_VIEWS = [
  "return",
  "trades",
  "hold",
  "drawdown",
  "field",
] as const;
export type PulseView = (typeof PULSE_VIEWS)[number];
export const PULSE_VIEW_STORAGE_KEY = "xvn:pulse-view";

export function normalizePulseView(raw: string | null): PulseView {
  return (PULSE_VIEWS as readonly string[]).includes(raw ?? "")
    ? (raw as PulseView)
    : "return";
}

// ─── "All runs" field view ───────────────────────────────────────────────────

export interface FieldRunSeries {
  runId: string;
  label: string;
  /** Elapsed fraction of the run's own window, 0..1. */
  fraction: number[];
  returnPct: (number | null)[];
}

/** Normalize one run's raw equity curve for the field overlay. Returns null
 * for series that can't be charted (under 2 finite samples, zero base
 * equity, or zero time span). */
export function fieldRunSeries(
  runId: string,
  label: string,
  equity: { time: number; equity_usd: number }[],
): FieldRunSeries | null {
  const pts = equity.filter(
    (p) => Number.isFinite(p.time) && Number.isFinite(p.equity_usd),
  );
  if (pts.length < 2) return null;
  const base = pts[0].equity_usd;
  const t0 = pts[0].time;
  const span = pts[pts.length - 1].time - t0;
  if (base === 0 || span <= 0) return null;
  return {
    runId,
    label,
    fraction: pts.map((p) => (p.time - t0) / span),
    returnPct: pts.map((p) => (p.equity_usd / base - 1) * 100),
  };
}

/** Align per-run fraction grids onto one shared x column (union of all
 * fractions); missing samples become null gaps (chart uses spanGaps). */
export function alignFieldSeries(series: FieldRunSeries[]): {
  x: number[];
  ys: (number | null)[][];
} {
  const x = [...new Set(series.flatMap((s) => s.fraction))].sort(
    (a, b) => a - b,
  );
  const ys = series.map((s) => {
    const byFraction = new Map(
      s.fraction.map((f, i) => [f, s.returnPct[i]] as const),
    );
    return x.map((f) => byFraction.get(f) ?? null);
  });
  return { x, ys };
}

// ─── "vs Buy & Hold" view ────────────────────────────────────────────────────

/** Merge the strategy return-% curve with the server baseline (raw USD,
 * sampled at equity timestamps) onto one axis; baseline normalizes to its
 * own first sample. */
export function holdCompareSeries(
  equity: SeriesPoint[],
  baseline: { time: number; equity_usd: number }[],
): { time: number[]; strategy: (number | null)[]; hold: (number | null)[] } {
  const time = equity.map((p) => p.time);
  const strategy = equity.map((p) =>
    Number.isFinite(p.value) ? p.value : null,
  );
  const base = baseline.find((b) => Number.isFinite(b.equity_usd))?.equity_usd;
  const holdByTime = new Map(
    base
      ? baseline
          .filter((b) => Number.isFinite(b.equity_usd))
          .map((b) => [b.time, (b.equity_usd / base - 1) * 100] as const)
      : [],
  );
  const hold = time.map((t) => holdByTime.get(t) ?? null);
  return { time, strategy, hold };
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
