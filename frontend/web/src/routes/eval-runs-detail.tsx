import { useParams, Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { evalKeys, getRun } from "@/api/eval";
import { chartKeys, getRunChart } from "@/api/chart";
import { RunChart } from "@/components/chart/RunChart";
import type {
  DecisionRowDto,
  RunSummary,
} from "@/api/types.gen";

const STATUS_TONE: Record<string, "gold" | "warn" | "danger" | "default" | "info"> = {
  completed: "gold",
  running: "info",
  queued: "default",
  failed: "danger",
  cancelled: "warn",
};

export function EvalRunDetailRoute() {
  const { runId } = useParams<{ runId: string }>();
  const id = runId ?? "";
  const q = useQuery({
    queryKey: evalKeys.run(id),
    queryFn: () => getRun(id),
    enabled: id.length > 0,
  });
  const chart = useQuery({
    queryKey: chartKeys.run(id),
    queryFn: () => getRunChart(id),
    enabled: !!id,
  });

  if (q.isPending) {
    return (
      <>
        <Topbar title="Run detail" sub={id ? id : "Loading…"} />
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
        <Topbar title="Run detail" sub={id} />
        <ErrorState err={q.error} onRetry={() => q.refetch()} runId={id} />
      </>
    );
  }

  const detail = q.data;
  return (
    <>
      <Topbar
        title={`Run ${detail.summary.id.slice(0, 12)}…`}
        sub={`${detail.summary.scenario_id} · ${detail.summary.mode}`}
      />

      <SummaryCard summary={detail.summary} />

      <h2 className="font-serif italic text-[20px] text-text mt-8 mb-3">
        Decisions <span className="text-text-3 text-[14px]">({detail.decisions.length})</span>
      </h2>
      <Card>
        {detail.decisions.length === 0 ? (
          <EmptyDecisions />
        ) : (
          <DecisionsTable rows={detail.decisions} />
        )}
      </Card>

      <h2 className="font-serif italic text-[20px] text-text mt-8 mb-3">
        Equity
      </h2>
      <Card className="p-5">
        {chart.isPending && (
          <div className="text-text-3 text-[13px] text-center py-6">
            Loading chart…
          </div>
        )}
        {chart.isError && (
          <div className="text-danger text-[13px] text-center py-6">
            Chart unavailable: {String(chart.error)}
          </div>
        )}
        {chart.data && <RunChart payload={chart.data} />}
      </Card>
    </>
  );
}

// ────────────────────────────────────────────────────────────────────────────

function SummaryCard({ summary }: { summary: RunSummary }) {
  const tone = STATUS_TONE[summary.status] ?? "default";
  return (
    <Card className="p-5">
      <div className="flex items-center justify-between mb-4">
        <div>
          <div className="text-text-3 text-[12px] font-mono">{summary.id}</div>
          <div className="text-text-2 text-[12px] mt-1">
            strategy{" "}
            <code className="font-mono text-text">
              {summary.agent_id.slice(0, 12)}
            </code>
          </div>
        </div>
        <Pill tone={tone}>
          <span
            className="w-1.5 h-1.5 rounded-full"
            style={dotColor(tone)}
          />
          {summary.status}
        </Pill>
      </div>

      <div className="grid grid-cols-3 gap-x-8 gap-y-3">
        <Metric label="Sharpe" value={fmtNumber(summary.sharpe)} />
        <Metric label="Max DD" value={fmtPct(summary.max_drawdown_pct)} />
        <Metric label="Total return" value={fmtPct(summary.total_return_pct)} />
        <Metric label="Mode" value={summary.mode} />
        <Metric label="Started" value={fmtTime(summary.started_at)} />
        <Metric
          label="Completed"
          value={summary.completed_at ? fmtTime(summary.completed_at) : "—"}
        />
      </div>

      {summary.error ? (
        <div className="mt-4 p-3 border border-danger/40 bg-danger/[0.06] rounded-sm">
          <div className="text-[11px] text-danger uppercase tracking-wide mb-1">
            error
          </div>
          <code className="font-mono text-[12px] text-text whitespace-pre-wrap">
            {summary.error}
          </code>
        </div>
      ) : null}
    </Card>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-text-3 text-[11px] uppercase tracking-wide mb-1">
        {label}
      </div>
      <div className="font-mono text-text">{value}</div>
    </div>
  );
}

function DecisionsTable({ rows }: { rows: DecisionRowDto[] }) {
  return (
    <table className="w-full">
      <thead>
        <tr className="text-left text-text-2 text-[12px] border-b border-border-soft">
          <th className="font-normal py-2.5 px-5">#</th>
          <th className="font-normal py-2.5 px-3">Time</th>
          <th className="font-normal py-2.5 px-3">Asset</th>
          <th className="font-normal py-2.5 px-3">Action</th>
          <th className="font-normal py-2.5 px-3 text-right">Conviction</th>
          <th className="font-normal py-2.5 px-3 text-right">Size</th>
          <th className="font-normal py-2.5 px-3 text-right">Fill</th>
          <th className="font-normal py-2.5 px-3 text-right">PnL</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr
            key={`${r.decision_index}`}
            className="border-b border-border-soft last:border-b-0 hover:bg-surface-hover transition-colors"
          >
            <td className="py-2.5 px-5 font-mono text-text-3 text-[12px]">
              {r.decision_index}
            </td>
            <td className="py-2.5 px-3 text-text-3 text-[12px]">
              {fmtTime(r.timestamp)}
            </td>
            <td className="py-2.5 px-3 font-mono text-text-2">{r.asset}</td>
            <td className="py-2.5 px-3 text-text">{r.action}</td>
            <td className="py-2.5 px-3 text-right font-mono">
              {fmtNumber(r.conviction)}
            </td>
            <td className="py-2.5 px-3 text-right font-mono">
              {fmtNumber(r.order_size)}
            </td>
            <td className="py-2.5 px-3 text-right font-mono text-text-2">
              {fmtNumber(r.fill_price)}
            </td>
            <td
              className={`py-2.5 px-3 text-right font-mono ${pnlClass(r.pnl_realized)}`}
            >
              {fmtNumber(r.pnl_realized)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function pnlClass(n: number | null | undefined): string {
  if (n == null) return "text-text-3";
  if (n > 0) return "text-gold";
  if (n < 0) return "text-danger";
  return "text-text-2";
}

function EmptyDecisions() {
  return (
    <div className="px-6 py-12 text-center text-text-2">
      <div className="font-serif italic text-[22px] text-text-3 mb-2">
        no decisions
      </div>
      <p className="m-0 text-[13px]">
        This run hasn't recorded any decisions yet — likely still queued or
        running.
      </p>
    </div>
  );
}

function ErrorState({
  err,
  onRetry,
  runId,
}: {
  err: unknown;
  onRetry: () => void;
  runId: string;
}) {
  if (err instanceof ApiError && err.code === "not_found") {
    return (
      <Card className="px-6 py-12 text-center">
        <div className="font-serif italic text-[24px] text-text-3 mb-3">
          run not found
        </div>
        <p className="m-0 mb-5 text-text-2 text-[13px]">
          No run with id <code className="font-mono text-text">{runId}</code>.
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

  const detail =
    err instanceof ApiError
      ? `${err.code}: ${err.message}`
      : err instanceof Error
        ? err.message
        : String(err);

  return (
    <Card className="px-6 py-12 text-center">
      <div className="font-serif italic text-[24px] text-danger mb-3">
        couldn't load run
      </div>
      <p className="m-0 mb-5 max-w-md mx-auto text-text-2 leading-snug">
        <code className="text-danger font-mono text-[12px]">{detail}</code>
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

function fmtNumber(n: number | null | undefined): string {
  if (n == null) return "—";
  return n.toFixed(2);
}

function fmtPct(n: number | null | undefined): string {
  if (n == null) return "—";
  const sign = n > 0 ? "+" : "";
  return `${sign}${n.toFixed(2)}%`;
}

function fmtTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function dotColor(tone: "gold" | "warn" | "danger" | "default" | "info") {
  return {
    gold: { background: "var(--gold)" },
    warn: { background: "var(--warn)" },
    danger: { background: "var(--danger)" },
    info: { background: "var(--info)" },
    default: { background: "var(--text-3)" },
  }[tone];
}
