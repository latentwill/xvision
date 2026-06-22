import { useMemo, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { compareRuns, evalKeys, listRunsPaged } from "@/api/eval";
import { listScenarios, scenarioKeys } from "@/api/scenarios";
import { listStrategies, strategyKeys, type ExitReason, type StrategyListItem } from "@/api/strategies";
import { ChartFrame } from "@/components/chart/v2/primitives/ChartFrame";
import { UplotCompareOverlayPane } from "@/components/chart/v2/primitives/UplotCompareOverlayPane";
import { useChart2Theme } from "@/components/chart/v2/hooks/useChart2Theme";
import { isInflightRunStatus } from "@/lib/run-status";
import { drawdownToneClass } from "@/lib/metric-tone";
import {
  displayScenarioName,
  displayStrategyName,
} from "@/lib/run-display";
import type {
  ComparisonRunSummary,
  Finding,
} from "@/api/types.gen";

const STATUS_TONE: Record<string, "gold" | "warn" | "danger" | "default" | "info"> = {
  completed: "gold",
  running: "info",
  queued: "default",
  failed: "danger",
  cancelled: "warn",
};

// Curve colors are stable per-position (run A always gold, run B always
// info, etc.) so the legend matches the chart without per-id randomization.
const CURVE_PALETTE = [
  { stroke: "var(--gold)", label: "gold" },
  { stroke: "var(--info)", label: "info" },
  { stroke: "var(--warn)", label: "warn" },
  { stroke: "var(--danger)", label: "danger" },
] as const;

// Non-color identifier (A, B, C, …) so runs stay distinguishable without
// relying on the palette color alone (colorblind accessibility).
const runLetter = (i: number) => String.fromCharCode(65 + (Math.max(0, i) % 26));

export function EvalCompareRoute() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const ids = useMemo(() => parseIds(params.get("ids")), [params]);
  const theme = useChart2Theme();

  const q = useQuery({
    queryKey: evalKeys.compare(ids),
    queryFn: () => compareRuns(ids),
    enabled: ids.length >= 2,
  });
  const strategies = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });
  const scenarios = useQuery({
    queryKey: scenarioKeys.list(),
    queryFn: () => listScenarios(),
  });

  // Empty / single-id state — short-circuit before hitting the api so the
  // user gets actionable copy + a link back to /eval-runs to pick more.
  if (ids.length < 2) {
    return (
      <>
        <Topbar
          title="Compare"
          sub={ids.length === 1 ? "1 id given" : "0 ids given"}
        />
        <NeedTwoOrMore
          given={ids.length}
          initialIds={ids}
          strategies={strategies.data ?? []}
          scenarios={scenarios.data ?? []}
          onCompare={(nextIds) => {
            navigate(`/eval-runs/compare?ids=${nextIds.map(encodeURIComponent).join(",")}`);
          }}
        />
      </>
    );
  }

  if (q.isPending) {
    return (
      <>
        <Topbar title="Compare" sub={`${ids.length} runs · loading…`} />
        <Card className="p-6 animate-pulse">
          <div className="h-5 w-72 bg-surface-elev rounded mb-3" />
          <div className="h-4 w-48 bg-surface-elev rounded" />
        </Card>
      </>
    );
  }

  if (q.isError || !q.data) {
    return (
      <>
        <Topbar title="Compare" sub={`${ids.length} runs`} />
        <ErrorState err={q.error} ids={ids} onRetry={() => q.refetch()} />
      </>
    );
  }

  const report = q.data;
  return (
    <>
      <Topbar
        title="Compare"
        sub={`${report.runs.length} runs · ${report.findings.length} findings`}
      />
      <MetricsTable
        runs={report.runs}
        strategies={strategies.data ?? []}
        scenarios={scenarios.data ?? []}
      />
      <ExitReasonBreakdown runs={report.runs} strategies={strategies.data ?? []} />
      <h2 className="font-sans font-semibold text-[20px] text-text mt-8 mb-3">
        Equity curves
      </h2>
      <ChartFrame title="Run equity overlay" range="All" onRange={() => undefined}>
        <UplotCompareOverlayPane
          arms={compareEquityArms(report, strategies.data ?? [], theme.compare.palette)}
          height={260}
        />
      </ChartFrame>
      <h2 className="font-sans font-semibold text-[20px] text-text mt-8 mb-3">
        Findings{" "}
        <span className="text-text-3 text-[14px]">
          ({report.findings.length})
        </span>
      </h2>
      <Card className="overflow-x-auto xvn-scroll">
        {report.findings.length === 0 ? (
          <EmptyFindings />
        ) : (
          <FindingsTable
            findings={report.findings}
            runs={report.runs}
            strategies={strategies.data ?? []}
            scenarios={scenarios.data ?? []}
          />
        )}
      </Card>
    </>
  );
}

// ────────────────────────────────────────────────────────────────────────────

type CompareSortKey =
  | "call_order"
  | "gross_return"
  | "net_return"
  | "sharpe"
  | "max_drawdown"
  | "decisions";

const COMPARE_SORT_OPTIONS: { value: CompareSortKey; label: string }[] = [
  { value: "call_order", label: "Call order" },
  { value: "gross_return", label: "Gross return % (high → low)" },
  { value: "net_return", label: "Net return % (high → low)" },
  { value: "sharpe", label: "Sharpe (high → low)" },
  { value: "max_drawdown", label: "Max DD (low → high)" },
  { value: "decisions", label: "Decisions (high → low)" },
];

function compareNumDesc(a: number | null | undefined, b: number | null | undefined): number {
  // Push nulls to the bottom regardless of sort direction so they
  // don't crowd the top of the operator's chosen sort.
  if (a == null && b == null) return 0;
  if (a == null) return 1;
  if (b == null) return -1;
  return b - a;
}

function compareNumAscMagnitude(
  a: number | null | undefined,
  b: number | null | undefined,
): number {
  // For Max DD: smaller magnitude is "better" (less loss). Sort by
  // |value| ascending. Nulls bottom.
  if (a == null && b == null) return 0;
  if (a == null) return 1;
  if (b == null) return -1;
  return Math.abs(a) - Math.abs(b);
}

function sortedRuns(runs: ComparisonRunSummary[], key: CompareSortKey): ComparisonRunSummary[] {
  if (key === "call_order") return runs;
  const out = [...runs];
  out.sort((a, b) => {
    switch (key) {
      case "gross_return":
        return compareNumDesc(a.metrics?.total_return_pct, b.metrics?.total_return_pct);
      case "net_return":
        return compareNumDesc(
          a.net_return_pct ?? a.metrics?.net_return_pct,
          b.net_return_pct ?? b.metrics?.net_return_pct,
        );
      case "sharpe":
        return compareNumDesc(a.metrics?.sharpe, b.metrics?.sharpe);
      case "max_drawdown":
        return compareNumAscMagnitude(a.metrics?.max_drawdown_pct, b.metrics?.max_drawdown_pct);
      case "decisions":
        return compareNumDesc(a.metrics?.n_decisions, b.metrics?.n_decisions);
      default:
        return 0;
    }
  });
  return out;
}

function MetricsTable({
  runs,
  strategies,
  scenarios,
}: {
  runs: ComparisonRunSummary[];
  strategies: StrategyListItem[];
  scenarios: { id: string; display_name?: string | null }[];
}) {
  // Sort is the highest-value ergonomic on compare — operator wants to
  // rank N runs by a single metric. Search/filter are NOT added: the
  // page typically shows 2-10 rows all visible at once, so substring-
  // match has no real win. Documented in
  // `docs/superpowers/audits/2026-05-21-list-surfaces-audit.md` row #5.
  const [sortKey, setSortKey] = useState<CompareSortKey>("call_order");
  const sortedRows = useMemo(() => sortedRuns(runs, sortKey), [runs, sortKey]);

  return (
    <Card className="overflow-x-auto xvn-scroll">
      <div className="flex items-center justify-between px-5 py-2.5 border-b border-border-soft">
        <div className="text-text-3 text-[12px]">
          {runs.length} {runs.length === 1 ? "run" : "runs"}
        </div>
        <label className="flex items-center gap-2 text-[12px] text-text-3">
          Sort by
          <select
            value={sortKey}
            onChange={(e) => setSortKey(e.target.value as CompareSortKey)}
            className="bg-surface-elev border border-border-soft rounded-sm px-2 py-1 text-[12px] text-text focus:outline-none focus:border-gold/40"
            data-testid="compare-sort"
          >
            {COMPARE_SORT_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
      </div>
      <table className="w-full min-w-[960px]">
        <thead>
          <tr className="text-left text-text-2 text-[12px] border-b border-border-soft">
            <th className="font-normal py-2.5 px-5">Run</th>
            <th className="font-normal py-2.5 px-3">Status</th>
            <th className="font-normal py-2.5 px-3">Scenario</th>
            <th className="font-normal py-2.5 px-3 text-right">Gross %</th>
            <th className="font-normal py-2.5 px-3 text-right">Infer cost</th>
            <th className="font-normal py-2.5 px-3 text-right">Net %</th>
            <th className="font-normal py-2.5 px-3 text-right">Sharpe</th>
            <th className="font-normal py-2.5 px-3 text-right">Max DD</th>
            <th className="font-normal py-2.5 px-3 text-right">Win rate</th>
            <th className="font-normal py-2.5 px-3 text-right">Trades</th>
            <th className="font-normal py-2.5 px-3 text-right">Decisions</th>
          </tr>
        </thead>
        <tbody>
          {sortedRows.map((r) => {
            const tone = STATUS_TONE[r.status] ?? "default";
            // Palette dot must match the equity-chart curve color for
            // this run, so derive it from the ORIGINAL `runs` index —
            // not the sorted index. Otherwise the chart legend and the
            // table dot drift apart whenever the operator picks a
            // non-default sort.
            const originalIdx = runs.findIndex((x) => x.id === r.id);
            const palette = CURVE_PALETTE[originalIdx % CURVE_PALETTE.length];
            const color = strategyColor(r, strategies, palette.stroke);
            return (
              <tr
                key={r.id}
                className="border-b border-border-soft last:border-b-0 hover:bg-surface-hover transition-colors"
              >
                <td className="py-2.5 px-5">
                  <span
                    className="mr-2 inline-flex h-4 w-4 items-center justify-center rounded-full border text-[9px] font-bold leading-none align-middle"
                    style={{ borderColor: color, color }}
                    aria-label={`Run ${runLetter(originalIdx)}`}
                  >
                    {runLetter(originalIdx)}
                  </span>
                  <Link
                    to={`/eval-runs/${encodeURIComponent(r.id)}`}
                    className="text-text hover:underline"
                  >
                    {strategyLabel(r, strategies)}
                  </Link>
                  <div className="mt-0.5 font-mono text-[11px] text-text-3 break-all select-all">
                    {r.id}
                  </div>
                </td>
                <td className="py-2.5 px-3">
                  <Pill tone={tone} animated={isInflightRunStatus(r.status)}>
                    {r.status}
                  </Pill>
                </td>
                <td className="py-2.5 px-3 text-text-2 text-[12px]">
                  {displayScenarioName(r.scenario_id, scenarios, r.mode)}
                </td>
                <MetricCell
                  value={fmtPct(r.metrics?.total_return_pct)}
                  sign={signOf(r.metrics?.total_return_pct)}
                />
                <MetricCell value={fmtCostUsd(r.metrics?.inference_cost_quote_total)} />
                <MetricCell
                  value={fmtPct(r.net_return_pct ?? r.metrics?.net_return_pct)}
                  sign={signOf(r.net_return_pct ?? r.metrics?.net_return_pct)}
                />
                <MetricCell value={fmtNumber(r.metrics?.sharpe, 3)} />
                <MetricCell
                  value={fmtPct(r.metrics?.max_drawdown_pct)}
                  toneClass={drawdownToneClass(r.metrics?.max_drawdown_pct)}
                />
                <MetricCell value={fmtNumber(r.metrics?.win_rate, 2)} />
                <MetricCell value={fmtInt(r.metrics?.n_trades)} />
                <MetricCell value={fmtInt(r.metrics?.n_decisions)} />
              </tr>
            );
          })}
        </tbody>
      </table>
    </Card>
  );
}

function MetricCell({
  value,
  sign,
  toneClass,
}: {
  value: string;
  sign?: 1 | -1 | 0;
  /** Override the sign-derived tone class (e.g. for drawdown, which is
   * always loss-coloured regardless of sign). */
  toneClass?: string;
}) {
  const tone =
    toneClass ??
    (sign == null
      ? "text-text"
      : sign > 0
        ? "text-gold"
        : sign < 0
          ? "text-danger"
          : "text-text-2");
  return <td className={`py-2.5 px-3 text-right font-mono ${tone}`}>{value}</td>;
}

function FindingsTable({
  findings,
  runs,
  strategies,
  scenarios,
}: {
  findings: Finding[];
  runs: ComparisonRunSummary[];
  strategies: StrategyListItem[];
  scenarios: { id: string; display_name?: string | null }[];
}) {
  const idToIndex = new Map(runs.map((r, i) => [r.id, i] as const));
  const runById = new Map(runs.map((r) => [r.id, r] as const));
  return (
    <table className="w-full min-w-[720px]">
      <thead>
        <tr className="text-left text-text-2 text-[12px] border-b border-border-soft">
          <th className="font-normal py-2.5 px-5">Run</th>
          <th className="font-normal py-2.5 px-3">Severity</th>
          <th className="font-normal py-2.5 px-3">Kind</th>
          <th className="font-normal py-2.5 px-3">Summary</th>
        </tr>
      </thead>
      <tbody>
        {findings.map((f) => {
          const idx = idToIndex.get(f.run_id) ?? 0;
          const run = runById.get(f.run_id);
          const palette = CURVE_PALETTE[idx % CURVE_PALETTE.length];
          const color = run ? strategyColor(run, strategies, palette.stroke) : palette.stroke;
          return (
            <tr
              key={f.id}
              className="border-b border-border-soft last:border-b-0 hover:bg-surface-hover transition-colors"
            >
              <td className="py-2.5 px-5">
                <span
                  className="mr-2 inline-flex h-4 w-4 items-center justify-center rounded-full border text-[9px] font-bold leading-none align-middle"
                  style={{ borderColor: color, color }}
                  aria-label={`Run ${runLetter(idx)}`}
                >
                  {runLetter(idx)}
                </span>
                <Link
                  to={`/eval-runs/${encodeURIComponent(f.run_id)}`}
                  className="text-text hover:underline text-[12px]"
                >
                  {run
                    ? strategyLabel(run, strategies)
                    : `Run ${f.run_id}`}
                </Link>
                {run ? (
                  <div className="mt-0.5 text-[11px] text-text-3">
                    {displayScenarioName(run.scenario_id, scenarios, run.mode)}
                  </div>
                ) : null}
              </td>
              <td className="py-2.5 px-3">
                <Pill tone={severityTone(f.severity)}>{f.severity}</Pill>
              </td>
              <td className="py-2.5 px-3 text-text">{f.kind}</td>
              <td className="py-2.5 px-3 text-text-2">{f.summary}</td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function severityTone(
  s: string,
): "gold" | "warn" | "danger" | "default" | "info" {
  switch (s) {
    case "critical":
      return "danger";
    case "warning":
      return "warn";
    case "info":
      return "info";
    default:
      return "default";
  }
}

function EmptyFindings() {
  return (
    <div className="px-6 py-12 text-center text-text-2">
      <div className="font-sans font-semibold text-[22px] text-text-3 mb-2">
        no findings
      </div>
      <p className="m-0 text-[13px]">
        None of the selected runs have extracted findings yet. Run the
        findings extractor to surface regime / risk / behavioral notes.
      </p>
    </div>
  );
}

function NeedTwoOrMore({
  given,
  initialIds,
  strategies,
  scenarios,
  onCompare,
}: {
  given: number;
  initialIds: string[];
  strategies: StrategyListItem[];
  scenarios: { id: string; display_name?: string | null }[];
  onCompare: (ids: string[]) => void;
}) {
  const runsQ = useQuery({
    queryKey: evalKeys.runs({ limit: 25, offset: 0 }),
    queryFn: () => listRunsPaged({ limit: 25, offset: 0 }),
  });
  const [selected, setSelected] = useState<Set<string>>(
    () => new Set(initialIds),
  );
  const ready = selected.size >= 2;
  const runs = runsQ.data?.items ?? [];

  function toggle(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  return (
    <Card className="overflow-hidden">
      <div className="px-6 py-6 border-b border-border-soft">
        <div className="font-sans font-semibold text-[22px] text-text-3 mb-2">
          Compare needs two or more runs
        </div>
        <p className="m-0 text-text-2 text-[13px]">
          {given === 0
            ? "Pick runs below, or open this route with ?ids=<run-a>,<run-b>."
            : "One id was provided. Pick another run below to build the comparison."}
        </p>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3 px-6 py-3 border-b border-border-soft bg-surface-2/20">
        <div className="text-[12px] text-text-2">
          {selected.size} selected
        </div>
        <div className="flex items-center gap-2">
          <Link
            to="/eval-runs"
            className="inline-flex items-center gap-2 px-3 py-1.5 rounded text-[12px] font-medium border border-border text-text-2 hover:border-text-3 hover:text-text"
          >
            Back to runs
          </Link>
          <button
            type="button"
            disabled={!ready}
            onClick={() => onCompare([...selected])}
            className={`inline-flex items-center gap-2 rounded px-3 py-1.5 text-[12px] font-medium border transition-colors ${
              ready
                ? "border-gold text-gold hover:bg-gold/10"
                : "border-border text-text-3 opacity-60 cursor-not-allowed"
            }`}
          >
            Compare {ready ? `(${selected.size})` : ""}
          </button>
        </div>
      </div>

      {runsQ.isPending ? (
        <div className="px-6 py-10 text-center text-[13px] text-text-3">
          Loading recent runs...
        </div>
      ) : runsQ.isError ? (
        <div className="px-6 py-10 text-center text-[13px] text-danger">
          Couldn't load recent runs.
        </div>
      ) : runs.length === 0 ? (
        <div className="px-6 py-10 text-center text-[13px] text-text-3">
          No eval runs yet. Start an eval first, then return to compare.
        </div>
      ) : (
        <div className="overflow-x-auto xvn-scroll">
          <table className="w-full min-w-[760px]">
            <thead>
              <tr className="text-left text-text-2 text-[12px] border-b border-border-soft">
                <th className="font-normal py-2.5 px-5 w-10" />
                <th className="font-normal py-2.5 px-3">Run</th>
                <th className="font-normal py-2.5 px-3">Scenario</th>
                <th className="font-normal py-2.5 px-3">Status</th>
                <th className="font-normal py-2.5 px-3 text-right">Return</th>
                <th className="font-normal py-2.5 px-3 text-right">Sharpe</th>
              </tr>
            </thead>
            <tbody>
              {runs.map((run) => {
                const checked = selected.has(run.id);
                return (
                  <tr
                    key={run.id}
                    className="border-b border-border-soft last:border-b-0 hover:bg-surface-hover"
                  >
                    <td className="py-2.5 pl-5 pr-2">
                      <input
                        type="checkbox"
                        aria-label={`Select run ${run.id}`}
                        checked={checked}
                        onChange={() => toggle(run.id)}
                        className="accent-gold"
                      />
                    </td>
                    <td className="py-2.5 px-3">
                      <div className="text-[13px] text-text">
                        {displayStrategyName(run.agent_id, strategies)}
                      </div>
                      <div className="mt-0.5 font-mono text-[11px] text-text-3 break-all">
                        {run.id}
                      </div>
                    </td>
                    <td className="py-2.5 px-3 text-[12px] text-text-2">
                      {displayScenarioName(run.scenario_id, scenarios, run.mode)}
                    </td>
                    <td className="py-2.5 px-3">
                      <Pill
                        tone={STATUS_TONE[run.status] ?? "default"}
                        animated={isInflightRunStatus(run.status)}
                      >
                        {run.status}
                      </Pill>
                    </td>
                    <MetricCell
                      value={fmtPct(run.total_return_pct)}
                      sign={signOf(run.total_return_pct)}
                    />
                    <MetricCell value={fmtNumber(run.sharpe, 3)} />
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}

function ErrorState({
  err,
  ids,
  onRetry,
}: {
  err: unknown;
  ids: string[];
  onRetry: () => void;
}) {
  if (err instanceof ApiError && err.code === "not_found") {
    return (
      <Card className="px-6 py-12 text-center">
        <div className="font-sans font-semibold text-[22px] text-text-3 mb-2">
          a run is missing
        </div>
        <p className="m-0 mb-5 text-text-2 text-[13px]">
          One of <code className="font-mono">{ids.join(", ")}</code> doesn't
          exist. Server said:{" "}
          <code className="text-danger font-mono">{err.message}</code>
        </p>
        <Link
          to="/eval-runs"
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
        >
          ← Back to runs
        </Link>
      </Card>
    );
  }
  if (err instanceof ApiError && err.code === "validation") {
    return (
      <Card className="px-6 py-12 text-center">
        <div className="font-sans font-semibold text-[22px] text-text-3 mb-2">
          can't compare these
        </div>
        <p className="m-0 mb-5 text-text-2 text-[13px]">
          <code className="text-danger font-mono">{err.message}</code>
        </p>
        <button
          onClick={onRetry}
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
        >
          Retry
        </button>
      </Card>
    );
  }
  const detail =
    err instanceof ApiError
      ? `${err.code}: ${err.message}`
      : err instanceof Error
        ? err.message
        : String(err);
  return (
    <Card className="px-6 py-12 text-center">
      <div className="font-sans font-semibold text-[22px] text-danger mb-3">
        couldn't load comparison
      </div>
      <p className="m-0 mb-5 text-text-2 text-[13px]">
        <code className="text-danger font-mono">{detail}</code>
      </p>
      <button
        onClick={onRetry}
        className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
      >
        Retry
      </button>
    </Card>
  );
}

// ────────────────────────────────────────────────────────────────────────────
// Exit reason breakdown

const EXIT_REASONS: ExitReason[] = [
  "stop_loss",
  "take_profit",
  "trailing_stop",
  "time_expiry",
  "signal",
  "manual",
];

type ComparisonRunSummaryWithExitReasons = import("@/api/types.gen").ComparisonRunSummary & {
  exit_reason_counts?: Partial<Record<ExitReason, number>>;
};

function ExitReasonBreakdown({
  runs,
  strategies,
}: {
  runs: import("@/api/types.gen").ComparisonRunSummary[];
  strategies: StrategyListItem[];
}) {
  const runsExt = runs as ComparisonRunSummaryWithExitReasons[];
  const hasData = runsExt.some((r) => r.exit_reason_counts != null);
  if (!hasData) return null;

  return (
    <>
      <h2 className="font-sans font-semibold text-[20px] text-text mt-8 mb-3">
        Exit reasons
      </h2>
      <Card className="overflow-x-auto xvn-scroll">
        <table className="w-full min-w-[720px]">
          <thead>
            <tr className="text-left text-text-2 text-[12px] border-b border-border-soft">
              <th className="font-normal py-2.5 px-5">Run</th>
              {EXIT_REASONS.map((r) => (
                <th key={r} className="font-normal py-2.5 px-3 text-right capitalize">
                  {r.replace(/_/g, " ")}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {runsExt.map((r) => (
              <tr
                key={r.id}
                className="border-b border-border-soft last:border-b-0 hover:bg-surface-hover transition-colors"
              >
                <td className="py-2.5 px-5">
                  <Link
                    to={`/eval-runs/${encodeURIComponent(r.id)}`}
                    className="text-text hover:underline text-[12px]"
                  >
                    {strategyLabel(r, strategies)}
                  </Link>
                </td>
                {EXIT_REASONS.map((reason) => (
                  <td
                    key={reason}
                    className="py-2.5 px-3 text-right font-mono text-[12px] text-text-2"
                  >
                    {r.exit_reason_counts?.[reason] ?? "—"}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </Card>
    </>
  );
}

// ────────────────────────────────────────────────────────────────────────────
// helpers

function parseIds(raw: string | null): string[] {
  if (!raw) return [];
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

function compareEquityArms(
  report: {
    runs: ComparisonRunSummary[];
    equity_curves: Array<{
      run_id: string;
      samples: Array<{ timestamp: string; equity_usd: number }>;
    }>;
  },
  strategies: StrategyListItem[],
  palette: readonly string[],
) {
  const curveByRun = new Map(report.equity_curves.map((curve) => [curve.run_id, curve] as const));
  return report.runs.map((run, idx) => {
    const fallback = palette[idx % palette.length] ?? CURVE_PALETTE[idx % CURVE_PALETTE.length].stroke;
    return {
      id: run.id,
      label: strategyLabel(run, strategies),
      color: strategyColor(run, strategies, fallback),
      equity: (curveByRun.get(run.id)?.samples ?? []).map((sample) => ({
        time: Date.parse(sample.timestamp) / 1000,
        value: sample.equity_usd,
      })),
    };
  });
}

function strategyLabel(run: ComparisonRunSummary, strategies: StrategyListItem[]): string {
  const name = run.strategy_name?.trim();
  return name || displayStrategyName(run.agent_id, strategies);
}

function strategyColor(
  run: ComparisonRunSummary,
  strategies: StrategyListItem[],
  fallback: string,
): string {
  return strategies.find((strategy) => strategy.agent_id === run.agent_id)?.color ?? fallback;
}

function fmtNumber(n: number | null | undefined, digits = 2): string {
  if (n == null) return "—";
  return n.toFixed(digits);
}

function fmtPct(n: number | null | undefined): string {
  if (n == null) return "—";
  const sign = n > 0 ? "+" : "";
  return `${sign}${n.toFixed(2)}%`;
}

function fmtInt(n: number | null | undefined): string {
  if (n == null) return "—";
  return n.toString();
}

function signOf(n: number | null | undefined): 1 | -1 | 0 | undefined {
  if (n == null) return undefined;
  if (n > 0) return 1;
  if (n < 0) return -1;
  return 0;
}

/** Format inference cost in USD — compact form with 4 decimal places. */
function fmtCostUsd(n: number | null | undefined): string {
  if (n == null) return "—";
  return `$${n.toFixed(4)}`;
}
