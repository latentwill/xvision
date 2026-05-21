import { useMemo } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { compareRuns, evalKeys } from "@/api/eval";
import { chartKeys, getCompareChart } from "@/api/chart";
import { listScenarios, scenarioKeys } from "@/api/scenarios";
import { listStrategies, strategyKeys } from "@/api/strategies";
import { CompareChart } from "@/components/chart/CompareChart";
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

export function EvalCompareRoute() {
  const [params] = useSearchParams();
  const ids = useMemo(() => parseIds(params.get("ids")), [params]);

  const q = useQuery({
    queryKey: evalKeys.compare(ids),
    queryFn: () => compareRuns(ids),
    enabled: ids.length >= 2,
  });
  const chart = useQuery({
    queryKey: chartKeys.compare(ids),
    queryFn: () => getCompareChart(ids),
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
        <NeedTwoOrMore given={ids.length} />
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
      <h2 className="font-serif italic text-[20px] text-text mt-8 mb-3">
        Equity curves
      </h2>
      <Card className="p-5">
        {chart.isPending ? (
          <p className="m-0 text-text-3 text-[13px] text-center py-6">
            Loading chart…
          </p>
        ) : chart.error ? (
          <p className="m-0 text-danger text-[13px] text-center py-6">
            Chart unavailable: {String(chart.error)}
          </p>
        ) : chart.data ? (
          <CompareChart payload={chart.data} />
        ) : null}
      </Card>
      <h2 className="font-serif italic text-[20px] text-text mt-8 mb-3">
        Findings{" "}
        <span className="text-text-3 text-[14px]">
          ({report.findings.length})
        </span>
      </h2>
      <Card>
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

function MetricsTable({
  runs,
  strategies,
  scenarios,
}: {
  runs: ComparisonRunSummary[];
  strategies: { agent_id: string; display_name?: string | null }[];
  scenarios: { id: string; display_name?: string | null }[];
}) {
  return (
    <Card>
      <table className="w-full">
        <thead>
          <tr className="text-left text-text-2 text-[12px] border-b border-border-soft">
            <th className="font-normal py-2.5 px-5">Run</th>
            <th className="font-normal py-2.5 px-3">Status</th>
            <th className="font-normal py-2.5 px-3">Scenario</th>
            <th className="font-normal py-2.5 px-3 text-right">Total return</th>
            <th className="font-normal py-2.5 px-3 text-right">Sharpe</th>
            <th className="font-normal py-2.5 px-3 text-right">Max DD</th>
            <th className="font-normal py-2.5 px-3 text-right">Win rate</th>
            <th className="font-normal py-2.5 px-3 text-right">Trades</th>
            <th className="font-normal py-2.5 px-3 text-right">Decisions</th>
          </tr>
        </thead>
        <tbody>
          {runs.map((r, i) => {
            const tone = STATUS_TONE[r.status] ?? "default";
            const palette = CURVE_PALETTE[i % CURVE_PALETTE.length];
            return (
              <tr
                key={r.id}
                className="border-b border-border-soft last:border-b-0 hover:bg-surface-hover transition-colors"
              >
                <td className="py-2.5 px-5">
                  <span
                    className="inline-block w-2 h-2 rounded-full mr-2"
                    style={{ background: palette.stroke }}
                    aria-hidden
                  />
                  <Link
                    to={`/eval-runs/${encodeURIComponent(r.id)}`}
                    className="text-text hover:underline"
                  >
                    {displayStrategyName(r.agent_id, strategies)}
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
                  {displayScenarioName(r.scenario_id, scenarios)}
                </td>
                <MetricCell
                  value={fmtPct(r.metrics?.total_return_pct)}
                  sign={signOf(r.metrics?.total_return_pct)}
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
  strategies: { agent_id: string; display_name?: string | null }[];
  scenarios: { id: string; display_name?: string | null }[];
}) {
  const idToIndex = new Map(runs.map((r, i) => [r.id, i] as const));
  const runById = new Map(runs.map((r) => [r.id, r] as const));
  return (
    <table className="w-full">
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
          return (
            <tr
              key={f.id}
              className="border-b border-border-soft last:border-b-0 hover:bg-surface-hover transition-colors"
            >
              <td className="py-2.5 px-5">
                <span
                  className="inline-block w-2 h-2 rounded-full mr-2"
                  style={{ background: palette.stroke }}
                  aria-hidden
                />
                <Link
                  to={`/eval-runs/${encodeURIComponent(f.run_id)}`}
                  className="text-text hover:underline text-[12px]"
                >
                  {run
                    ? displayStrategyName(run.agent_id, strategies)
                    : `Run ${f.run_id}`}
                </Link>
                {run ? (
                  <div className="mt-0.5 text-[11px] text-text-3">
                    {displayScenarioName(run.scenario_id, scenarios)}
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
      <div className="font-serif italic text-[22px] text-text-3 mb-2">
        no findings
      </div>
      <p className="m-0 text-[13px]">
        None of the selected runs have extracted findings yet. Run the
        findings extractor to surface regime / risk / behavioral notes.
      </p>
    </div>
  );
}

function NeedTwoOrMore({ given }: { given: number }) {
  return (
    <Card className="px-6 py-12 text-center">
      <div className="font-serif italic text-[22px] text-text-3 mb-2">
        compare needs two or more runs
      </div>
      <p className="m-0 mb-5 text-text-2 text-[13px]">
        {given === 0
          ? "No run ids in the URL — this view expects ?ids=<a>,<b>."
          : "One id was provided — pick another to compare it against."}
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
        <div className="font-serif italic text-[22px] text-text-3 mb-2">
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
        <div className="font-serif italic text-[22px] text-text-3 mb-2">
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
      <div className="font-serif italic text-[22px] text-danger mb-3">
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
// helpers

function parseIds(raw: string | null): string[] {
  if (!raw) return [];
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
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
