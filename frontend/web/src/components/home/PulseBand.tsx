// frontend/web/src/components/home/PulseBand.tsx
//
// Home hero ("Pulse band", dashboard redesign / audit F3): equity area chart
// of the latest meaningful completed run with a client-side drawdown band,
// flanked by Geist-Mono KPI numerals with micro-sparklines, an HONEST
// execution-state chip (live capital vs no live capital), and a freshness stamp.
//
// Honesty rules (docs/design/README.md): numbers come from the latest
// completed eval and say so; drawdown always rides next to return; "no live
// capital deployed" is a designed first-class state, not an apologetic dash.

import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";

import { chartKeys, getCompareChart, getRunChart } from "@/api/chart";
import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import { Card } from "@/components/primitives/Card";
import { normalizeEquityToReturnPct } from "@/components/chart/v2/adapters/columnar-to-uplot";
import { displayStrategyName } from "@/lib/run-display";
import type { LivenessCounts } from "@/features/live/strip-status";
import {
  evalThroughput,
  formatRelativeTime,
  isChartableRun,
  latestEvaluatedStrategyRuns,
  latestCompletionStamp,
  normalizePulseView,
  pickHeroRun,
  PULSE_VIEW_STORAGE_KEY,
  pulseChartSeries,
  recentMetricSeries,
  tradeMarkersFromPayload,
  type PulseView,
} from "@/features/home/pulse";
import { PulseEquityChart } from "./PulseEquityChart";
import { PulseViewSwitcher } from "./PulseViewSwitcher";
import { PulseDrawdownChart } from "./views/PulseDrawdownChart";
import { PulseFieldChart } from "./views/PulseFieldChart";
import { PulseHoldChart } from "./views/PulseHoldChart";
import { PulseTradesChart } from "./views/PulseTradesChart";
import { Sparkline, type SparklineTone } from "./Sparkline";

export interface PulseBandProps {
  runs: RunSummary[];
  strategies: StrategyListItem[];
  /** Honest liveness counts over the non-terminal agent-run population;
   * `null` while loading. */
  liveness: LivenessCounts | null;
  runsPending?: boolean;
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
  if (v === null || !Number.isFinite(v) || v === 0) return "text-text";
  return v > 0 ? "text-gold" : "text-danger";
}

// ─── sub-components ──────────────────────────────────────────────────────────

function ExecutionChip({ liveness }: { liveness: LivenessCounts | null }) {
  if (liveness === null) return null;
  const liveCount = liveness.liveActive + liveness.livePaused;
  if (liveCount > 0) {
    return (
      <span
        data-testid="execution-chip"
        className="inline-flex items-center gap-1.5 rounded-sm border border-gold/40 px-2.5 py-1 text-[11px] font-medium uppercase tracking-wide text-gold xvn-live-glow"
      >
        <span className="h-1.5 w-1.5 rounded-full bg-gold" aria-hidden />
        Live capital deployed · {liveCount}
      </span>
    );
  }
  return (
    <span
      data-testid="execution-chip"
      className="inline-flex items-center gap-1.5 rounded-sm border border-border-soft px-2.5 py-1 text-[11px] font-medium uppercase tracking-wide text-text-3"
    >
      <span className="h-1.5 w-1.5 rounded-full bg-text-4" aria-hidden />
      No live capital · paper/sim only
    </span>
  );
}

function Kpi({
  label,
  value,
  valueClass = "text-text",
  spark,
  sparkTone = "gold",
  sub,
  testId,
}: {
  label: string;
  value: string;
  valueClass?: string;
  spark?: number[];
  sparkTone?: SparklineTone;
  sub?: string;
  testId: string;
}) {
  return (
    <div className="flex flex-col gap-1 px-5 py-3.5 min-w-0">
      <span className="caps">{label}</span>
      <span
        data-testid={testId}
        className={`font-mono tabular-nums text-[24px] leading-none font-semibold tracking-tight ${valueClass}`}
      >
        {value}
      </span>
      {spark && spark.length >= 2 ? (
        <Sparkline values={spark} tone={sparkTone} width={84} height={18} />
      ) : sub ? (
        <span className="text-[11px] text-text-4">{sub}</span>
      ) : (
        <span className="text-[11px] text-text-4">single run</span>
      )}
    </div>
  );
}

function HeroEmptyState() {
  return (
    <div className="px-6 py-12 text-center space-y-3">
      <p className="caps">Pulse</p>
      <p className="text-[17px] font-medium text-text">
        No completed evals yet
      </p>
      <p className="mx-auto max-w-sm text-[13px] text-text-3">
        Pick a strategy and scenario to backtest, or start a paper deployment.
        The pulse band lights up with equity, drawdown, and throughput once the
        first eval completes.
      </p>
      <div className="flex items-center justify-center gap-3 pt-1">
        <Link
          to="/eval-runs?start=1"
          className="inline-flex items-center gap-1.5 rounded bg-gold px-3.5 py-1.5 text-[13px] font-medium text-bg hover:bg-gold-soft transition-colors"
        >
          Start eval
        </Link>
        <Link to="/strategies" className="text-[13px] text-text-3 hover:text-text">
          Browse strategies →
        </Link>
      </div>
    </div>
  );
}

// ─── view slot helpers ────────────────────────────────────────────────────────

function ViewSkeleton() {
  return <div className="h-[210px] animate-pulse rounded bg-surface-elev" />;
}

function ViewError({ onRetry }: { onRetry: () => void }) {
  return (
    <div
      data-testid="pulse-view-error"
      className="flex h-[210px] flex-col items-center justify-center gap-2 rounded border border-border-soft"
    >
      <p className="text-[13px] text-text-3">Couldn&apos;t load this view.</p>
      <button
        type="button"
        onClick={onRetry}
        className="rounded-sm border border-border-soft px-2.5 py-1 text-[11px] font-medium uppercase tracking-wide text-text-3 hover:text-text"
      >
        Retry
      </button>
    </div>
  );
}

function ViewEmpty({ message, testId }: { message: string; testId?: string }) {
  return (
    <div
      data-testid={testId}
      className="rounded border border-border-soft px-4 py-10 text-center"
    >
      <p className="text-[13px] text-text-3">{message}</p>
    </div>
  );
}

// ─── main component ──────────────────────────────────────────────────────────

export function PulseBand({
  runs,
  strategies,
  liveness,
  runsPending = false,
}: PulseBandProps) {
  const selectableRuns = useMemo(() => latestEvaluatedStrategyRuns(runs), [runs]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const selectedRun =
    selectedRunId != null
      ? selectableRuns.find((run) => run.id === selectedRunId)
      : undefined;
  const heroRun = selectedRun ?? selectableRuns[0] ?? pickHeroRun(runs);
  const heroRunId = heroRun?.id ?? "";

  useEffect(() => {
    if (
      selectedRunId != null &&
      !selectableRuns.some((run) => run.id === selectedRunId)
    ) {
      setSelectedRunId(null);
    }
  }, [selectableRuns, selectedRunId]);

  // View selection — read localStorage in the initializer so the lazy view's
  // query is enabled on first render (no flash-fire of the default view).
  const [view, setView] = useState<PulseView>(() =>
    normalizePulseView(window.localStorage.getItem(PULSE_VIEW_STORAGE_KEY)),
  );
  const changeView = (v: PulseView) => {
    setView(v);
    window.localStorage.setItem(PULSE_VIEW_STORAGE_KEY, v);
  };

  const chart = useQuery({
    // QA #1: fetch trade markers alongside equity so the return view can overlay
    // green "B" / red "S" markers on the curve.
    queryKey: chartKeys.run(heroRunId, ["equity", "markers"]),
    queryFn: () => getRunChart(heroRunId, ["equity", "markers"]),
    enabled: heroRunId !== "",
    staleTime: 30_000,
  });

  const tradesChart = useQuery({
    queryKey: chartKeys.run(heroRunId, ["bars", "markers"]),
    queryFn: () => getRunChart(heroRunId, ["bars", "markers"]),
    enabled: heroRunId !== "" && view === "trades",
    staleTime: 30_000,
  });
  const holdChart = useQuery({
    queryKey: chartKeys.run(heroRunId, ["equity", "baseline"]),
    queryFn: () => getRunChart(heroRunId, ["equity", "baseline"]),
    enabled: heroRunId !== "" && view === "hold",
    staleTime: 30_000,
  });
  const fieldRunIds = runs
    .filter((r) => r.status === "completed" && isChartableRun(r))
    .sort((a, b) => (b.completed_at ?? "").localeCompare(a.completed_at ?? ""))
    .slice(0, 10)
    .map((r) => r.id);
  const fieldChart = useQuery({
    queryKey: chartKeys.compare(fieldRunIds),
    queryFn: () => getCompareChart(fieldRunIds),
    enabled: view === "field" && fieldRunIds.length >= 2,
    staleTime: 30_000,
  });

  const equityMarkers = tradeMarkersFromPayload(chart.data);
  const series = chart.data
    ? pulseChartSeries(normalizeEquityToReturnPct(chart.data.equity))
    : null;
  const hasSeries = series !== null && series.time.length >= 2;

  const throughput = evalThroughput(runs);
  const freshness = formatRelativeTime(latestCompletionStamp(runs));
  const returnSpark = recentMetricSeries(runs, (r) => r.total_return_pct);
  const drawdownSpark = recentMetricSeries(runs, (r) =>
    r.max_drawdown_pct === null ? null : -r.max_drawdown_pct,
  );
  const sharpeSpark = recentMetricSeries(runs, (r) => r.sharpe);

  const strategyName = heroRun
    ? displayStrategyName(heroRun.agent_id ?? "", strategies)
    : "";

  return (
    <section data-testid="pulse-band" aria-label="Portfolio pulse">
      <Card className="relative overflow-hidden p-0 xvn-panel-wash xvn-grain">
        {/* Header: eyebrow, strategy context, execution-state chip */}
        <div className="relative flex flex-wrap items-start justify-between gap-3 px-5 pt-4 pb-3">
          <div className="min-w-0">
            <p className="caps mb-1">Pulse · latest eval</p>
            {heroRun ? (
              <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 min-w-0">
                <span className="text-[15px] font-medium text-text truncate max-w-[280px]">
                  {strategyName}
                </span>
                <Link
                  to={`/eval-runs/${heroRun.id}`}
                  className="text-[12px] text-text-3 hover:text-text"
                >
                  View run →
                </Link>
                {freshness ? (
                  <span
                    data-testid="pulse-freshness"
                    className="text-[11px] text-text-4"
                  >
                    updated {freshness}
                  </span>
                ) : null}
              </div>
            ) : null}
          </div>
          <ExecutionChip liveness={liveness} />
        </div>

        {selectableRuns.length > 1 && !runsPending ? (
          <div
            role="group"
            aria-label="Pulse strategy selector"
            data-testid="pulse-strategy-selector"
            className="relative flex flex-wrap items-center gap-1.5 px-5 pb-2"
          >
            {selectableRuns.map((run) => {
              const label = displayStrategyName(run.agent_id ?? "", strategies);
              const selected = run.id === heroRunId;
              return (
                <button
                  key={run.id}
                  type="button"
                  aria-pressed={selected}
                  title={label}
                  onClick={() => setSelectedRunId(run.id)}
                  className={`max-w-[180px] truncate rounded-sm border px-2.5 py-1 text-[11px] font-medium transition-colors ${
                    selected
                      ? "border-gold/40 text-gold"
                      : "border-border-soft text-text-3 hover:text-text"
                  }`}
                >
                  {label}
                </button>
              );
            })}
          </div>
        ) : null}

        {/* View switcher — full-width sub-row below the header */}
        {heroRun !== null && !runsPending ? (
          <PulseViewSwitcher view={view} onViewChange={changeView} />
        ) : null}

        {/* Body: per-view chart, loading skeleton, or designed empty state */}
        {runsPending ? (
          <div className="px-5 pb-4"><ViewSkeleton /></div>
        ) : heroRun === null ? (
          <HeroEmptyState />
        ) : (
          <div className="relative px-3 pb-2">
            {view === "return" &&
              (chart.isPending ? <ViewSkeleton /> :
               chart.isError ? <ViewError onRetry={() => chart.refetch()} /> :
               hasSeries ? <PulseEquityChart series={series!} markers={equityMarkers} /> :
               <ViewEmpty
                 testId="pulse-chart-unavailable"
                 message="No equity samples recorded for this run."
               />)}
            {view === "drawdown" &&
              (chart.isPending ? <ViewSkeleton /> :
               chart.isError ? <ViewError onRetry={() => chart.refetch()} /> :
               hasSeries ? <PulseDrawdownChart payload={chart.data!} /> :
               <ViewEmpty message="No equity samples recorded for this run." />)}
            {view === "trades" &&
              (tradesChart.isPending ? <ViewSkeleton /> :
               tradesChart.isError ? <ViewError onRetry={() => tradesChart.refetch()} /> :
               (tradesChart.data?.bars.length ?? 0) >= 2 ?
                 <PulseTradesChart payload={tradesChart.data!} /> :
               <ViewEmpty message="No market bars cached for this run." />)}
            {view === "hold" &&
              (holdChart.isPending ? <ViewSkeleton /> :
               holdChart.isError ? <ViewError onRetry={() => holdChart.refetch()} /> :
               (holdChart.data?.baseline_equity?.length ?? 0) >= 2 ?
                 <PulseHoldChart payload={holdChart.data!} /> :
               <ViewEmpty message="Buy & Hold baseline unavailable for this run." />)}
            {view === "field" &&
              (fieldRunIds.length < 2 ?
                 <ViewEmpty message="Need at least two completed runs for the field view." /> :
               fieldChart.isPending ? <ViewSkeleton /> :
               fieldChart.isError ? <ViewError onRetry={() => fieldChart.refetch()} /> :
                 <PulseFieldChart
                   heroRunId={heroRunId}
                   runs={(fieldChart.data?.runs ?? []).map((r) => ({
                     runId: r.run_id,
                     label: displayStrategyName(
                       runs.find((x) => x.id === r.run_id)?.agent_id ?? r.label,
                       strategies,
                     ),
                     equity: r.equity,
                   }))}
                 />)}
          </div>
        )}

        {/* KPI rail — numbers are the typography. All values are from the
            latest completed eval (labelled as such); throughput spans the
            visible runs page. */}
        {heroRun !== null ? (
          <div className="relative grid grid-cols-2 sm:grid-cols-4 border-t border-border-soft divide-x divide-border-soft">
            <Kpi
              label="Return · latest run"
              value={fmtSignedPct(heroRun.total_return_pct)}
              valueClass={signedTone(heroRun.total_return_pct)}
              spark={returnSpark}
              sparkTone={
                (heroRun.total_return_pct ?? 0) < 0 ? "danger" : "gold"
              }
              testId="pulse-kpi-return"
            />
            <Kpi
              label="Max drawdown"
              value={
                heroRun.max_drawdown_pct !== null
                  ? `${fmtNum(heroRun.max_drawdown_pct)}%`
                  : "—"
              }
              valueClass={
                heroRun.max_drawdown_pct ? "text-danger" : "text-text"
              }
              spark={drawdownSpark}
              sparkTone="danger"
              testId="pulse-kpi-drawdown"
            />
            <Kpi
              label="Sharpe"
              value={fmtNum(heroRun.sharpe)}
              spark={sharpeSpark}
              sparkTone="info"
              testId="pulse-kpi-sharpe"
            />
            <Kpi
              label="Evals"
              value={String(throughput.completed)}
              sub={
                throughput.inflight > 0
                  ? `${throughput.inflight} in flight`
                  : "completed · none in flight"
              }
              testId="pulse-kpi-evals"
            />
          </div>
        ) : null}
      </Card>
    </section>
  );
}
