import { useMemo } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { compareRuns, evalKeys } from "@/api/eval";
import type {
  ComparisonEquityCurve,
  ComparisonRunSummary,
  CompareFinding,
  MetricsSummary,
} from "@/api/types.compare";

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
      <MetricsTable runs={report.runs} />
      <h2 className="font-serif italic text-[20px] text-text mt-8 mb-3">
        Equity curves
      </h2>
      <Card className="p-5">
        <EquityOverlay curves={report.equity_curves} runs={report.runs} />
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
          <FindingsTable findings={report.findings} runs={report.runs} />
        )}
      </Card>
    </>
  );
}

// ────────────────────────────────────────────────────────────────────────────

function MetricsTable({ runs }: { runs: ComparisonRunSummary[] }) {
  return (
    <Card>
      <table className="w-full">
        <thead>
          <tr className="text-left text-text-2 text-[12px] border-b border-border-soft">
            <th className="font-normal py-2.5 px-5">Run</th>
            <th className="font-normal py-2.5 px-3">Status</th>
            <th className="font-normal py-2.5 px-3">Strategy</th>
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
                    className="font-mono text-text hover:underline"
                  >
                    {r.id.slice(0, 12)}…
                  </Link>
                </td>
                <td className="py-2.5 px-3">
                  <Pill tone={tone}>{r.status}</Pill>
                </td>
                <td className="py-2.5 px-3 font-mono text-text-2 text-[12px]">
                  {r.strategy_bundle_hash.slice(0, 12)}
                </td>
                <td className="py-2.5 px-3 text-text-2 text-[12px]">
                  {r.scenario_id}
                </td>
                <MetricCell
                  value={fmtPct(r.metrics?.total_return_pct)}
                  sign={signOf(r.metrics?.total_return_pct)}
                />
                <MetricCell value={fmtNumber(r.metrics?.sharpe, 3)} />
                <MetricCell value={fmtPct(r.metrics?.max_drawdown_pct)} />
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

function MetricCell({ value, sign }: { value: string; sign?: 1 | -1 | 0 }) {
  const tone =
    sign == null
      ? "text-text"
      : sign > 0
        ? "text-gold"
        : sign < 0
          ? "text-danger"
          : "text-text-2";
  return <td className={`py-2.5 px-3 text-right font-mono ${tone}`}>{value}</td>;
}

// Multi-curve overlay. Each curve is normalized to its own [min,max] so
// shape-comparison reads regardless of absolute equity (one run starting
// at $100k and another at $10k still align). For absolute-axis comparison
// the operator clicks through to `/eval-runs/<id>`.
function EquityOverlay({
  curves,
  runs,
}: {
  curves: ComparisonEquityCurve[];
  runs: ComparisonRunSummary[];
}) {
  const drawn = curves.filter((c) => c.samples.length > 0);
  if (drawn.length === 0) {
    return (
      <p className="m-0 text-text-3 text-[13px] text-center py-6">
        no equity samples on any run
      </p>
    );
  }
  const w = 800;
  const h = 220;
  const pad = 6;

  const idToIndex = new Map(runs.map((r, i) => [r.id, i] as const));

  return (
    <div>
      <Legend curves={drawn} runs={runs} idToIndex={idToIndex} />
      <svg
        viewBox={`0 0 ${w} ${h}`}
        className="w-full h-[220px]"
        aria-label={`Equity overlay for ${drawn.length} runs`}
      >
        {drawn.map((curve) => {
          const idx = idToIndex.get(curve.run_id) ?? 0;
          const palette = CURVE_PALETTE[idx % CURVE_PALETTE.length];
          const ys = curve.samples.map((s) => s.equity_usd);
          const min = Math.min(...ys);
          const max = Math.max(...ys);
          const range = max - min || 1;
          const path = curve.samples
            .map((s, i) => {
              const x = (i / (curve.samples.length - 1 || 1)) * w;
              const y =
                h - pad - ((s.equity_usd - min) / range) * (h - 2 * pad);
              return `${i === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`;
            })
            .join(" ");
          return (
            <path
              key={curve.run_id}
              d={path}
              fill="none"
              stroke={palette.stroke}
              strokeWidth="1.4"
              opacity={0.85}
            />
          );
        })}
      </svg>
      <div className="text-[11px] text-text-3 mt-2">
        Each curve is normalized to its own range — shape comparison only.
        Click a run id above for absolute-axis detail.
      </div>
    </div>
  );
}

function Legend({
  curves,
  runs,
  idToIndex,
}: {
  curves: ComparisonEquityCurve[];
  runs: ComparisonRunSummary[];
  idToIndex: Map<string, number>;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-5 gap-y-2 mb-3">
      {curves.map((curve) => {
        const idx = idToIndex.get(curve.run_id) ?? 0;
        const palette = CURVE_PALETTE[idx % CURVE_PALETTE.length];
        const run = runs.find((r) => r.id === curve.run_id);
        return (
          <span
            key={curve.run_id}
            className="inline-flex items-center gap-2 text-[12px] text-text-2"
          >
            <span
              className="w-3 h-[2px] rounded-sm"
              style={{ background: palette.stroke }}
              aria-hidden
            />
            <code className="font-mono">{curve.run_id.slice(0, 8)}…</code>
            <span className="text-text-3">
              {fmtMetricsBrief(run?.metrics)}
            </span>
          </span>
        );
      })}
    </div>
  );
}

function FindingsTable({
  findings,
  runs,
}: {
  findings: CompareFinding[];
  runs: ComparisonRunSummary[];
}) {
  const idToIndex = new Map(runs.map((r, i) => [r.id, i] as const));
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
                  className="font-mono text-text hover:underline text-[12px]"
                >
                  {f.run_id.slice(0, 8)}…
                </Link>
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

function fmtMetricsBrief(m: MetricsSummary | null | undefined): string {
  if (!m) return "(no metrics)";
  return `${fmtPct(m.total_return_pct)} · sharpe ${m.sharpe.toFixed(2)}`;
}
