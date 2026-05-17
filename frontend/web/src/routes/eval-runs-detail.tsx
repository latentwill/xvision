import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams, Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { cancelRun, downloadEvalRunExport, evalKeys, getRun, retryRun } from "@/api/eval";
import { chartKeys, getRunChart, openRunStream } from "@/api/chart";
import { RunChart } from "@/components/chart/RunChart";
import { ReviewPanel } from "@/features/eval-runs/review";
import { useTraceDock } from "@/stores/trace-dock";
import type {
  DecisionRowDto,
  RunDetail,
  RunSummary,
} from "@/api/types.gen";
import {
  MobileEvalRunDetail,
  MobileEvalRunDetailError,
  MobileEvalRunDetailLoading,
} from "./eval-runs-detail-mobile";

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
  const qc = useQueryClient();
  const q = useQuery({
    queryKey: evalKeys.run(id),
    queryFn: () => getRun(id),
    enabled: id.length > 0,
    refetchInterval: (query) => {
      const detail = query.state.data;
      const status = detail?.summary.status;
      return status === "queued" || status === "running" ? 2000 : false;
    },
  });
  const chart = useQuery({
    queryKey: chartKeys.run(id),
    queryFn: () => getRunChart(id),
    enabled: !!id,
  });
  const navigate = useNavigate();
  const cancel = useMutation({
    mutationFn: cancelRun,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: evalKeys.all });
    },
  });
  const retry = useMutation({
    mutationFn: retryRun,
    onSuccess: (detail) => {
      qc.invalidateQueries({ queryKey: evalKeys.all });
      if (detail.summary.id !== id) {
        navigate(`/eval-runs/${detail.summary.id}`);
      }
    },
  });
  useLiveRunStream(id, q.data, qc);
  const isPhone = useIsPhone();

  // TODO(agent-run-observability): cross-link decision-row click → open dock + set decisionFilter to span's decision_idx. Needs design pass — eval-run decision rows do not map 1:1 to agent-run span decision_idx values.

  useEffect(() => {
    if (!id) return;
    const status = q.data?.summary.status;
    useTraceDock
      .getState()
      .setActiveRun(id, status === "queued" || status === "running" ? "live" : "post-hoc");
  }, [id, q.data?.summary.status]);

  if (q.isPending) {
    if (isPhone) return <MobileEvalRunDetailLoading id={id} />;
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
    if (isPhone) {
      return (
        <MobileEvalRunDetailError
          err={q.error}
          onRetry={() => q.refetch()}
          runId={id}
        />
      );
    }
    return (
      <>
        <Topbar title="Run detail" sub={id} />
        <ErrorState err={q.error} onRetry={() => q.refetch()} runId={id} />
      </>
    );
  }

  const detail = q.data;
  if (isPhone) {
    return (
      <MobileEvalRunDetail
        detail={detail}
        onCancel={() => cancel.mutate(detail.summary.id)}
        cancelling={cancel.variables === detail.summary.id && cancel.isPending}
        onRetry={() => retry.mutate(detail.summary.id)}
        retrying={retry.variables === detail.summary.id && retry.isPending}
      />
    );
  }
  return (
    <>
      <Topbar
        title={`Run ${detail.summary.id.slice(0, 12)}…`}
        sub={`${detail.summary.scenario_id} · ${detail.summary.mode}`}
      />

      <SummaryCard
        summary={detail.summary}
        onCancel={() => cancel.mutate(detail.summary.id)}
        cancelling={cancel.variables === detail.summary.id && cancel.isPending}
        onRetry={() => retry.mutate(detail.summary.id)}
        retrying={retry.variables === detail.summary.id && retry.isPending}
      />

      <h2 className="font-serif italic text-[20px] text-text mt-8 mb-3">
        Decisions <span className="text-text-3 text-[14px]">({detail.decisions.length})</span>
      </h2>
      <DecisionsPanel rows={detail.decisions} />

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

      <h2 className="font-serif italic text-[20px] text-text mt-8 mb-3">
        Review
      </h2>
      {/*
        `key={detail.summary.id}` resets every piece of local state in
        ReviewPanel when the route is reused for a different run id —
        otherwise selectedId/generate-mutation state can bleed across
        navigations because the route element is mounted once and just
        re-renders with a new :runId.
      */}
      <ReviewPanel
        key={detail.summary.id}
        runId={detail.summary.id}
        runIsCompleted={detail.summary.status === "completed"}
      />
    </>
  );
}

// ────────────────────────────────────────────────────────────────────────────

type LiveRunEvent =
  | { event: "decision"; data: DecisionRowDto }
  | { event: "status"; data: { phase: string; message: string | null } };

function useLiveRunStream(
  runId: string,
  detail: RunDetail | undefined,
  queryClient: ReturnType<typeof useQueryClient>,
) {
  const status = detail?.summary.status;
  const shouldStream = Boolean(status && !isTerminalStatus(status));
  useEffect(() => {
    if (!runId || !shouldStream) return;

    const es = openRunStream(runId);
    const updateRun = (updater: (current: RunDetail) => RunDetail) => {
      queryClient.setQueryData<RunDetail>(evalKeys.run(runId), (current) => {
        if (!current) return current;
        return updater(current);
      });
    };

    const onDecision = (ev: Event) => {
      const parsed = JSON.parse((ev as MessageEvent).data) as LiveRunEvent;
      if (parsed.event !== "decision") return;
      updateRun((current) => {
        const exists = current.decisions.some(
          (row) => row.decision_index === parsed.data.decision_index,
        );
        if (exists) {
          return {
            ...current,
            decisions: current.decisions
              .map((row) =>
                row.decision_index === parsed.data.decision_index
                  ? parsed.data
                  : row,
              )
              .sort((a, b) => a.decision_index - b.decision_index),
          };
        }
        return {
          ...current,
          decisions: [...current.decisions, parsed.data].sort(
            (a, b) => a.decision_index - b.decision_index,
          ),
        };
      });
    };

    const onStatus = (ev: Event) => {
      const parsed = JSON.parse((ev as MessageEvent).data) as LiveRunEvent;
      if (parsed.event !== "status") return;
      updateRun((current) => ({
        ...current,
        summary: {
          ...current.summary,
          status: parsed.data.phase,
          error:
            parsed.data.phase === "failed"
              ? (parsed.data.message ?? current.summary.error)
              : current.summary.error,
        },
      }));
      if (isTerminalStatus(parsed.data.phase)) {
        es.close();
        queryClient.invalidateQueries({ queryKey: evalKeys.run(runId) });
        queryClient.invalidateQueries({ queryKey: chartKeys.run(runId) });
      }
    };

    es.addEventListener("decision", onDecision);
    es.addEventListener("status", onStatus);
    es.onerror = () => {
      es.close();
      queryClient.invalidateQueries({ queryKey: evalKeys.run(runId) });
    };

    return () => {
      es.removeEventListener("decision", onDecision);
      es.removeEventListener("status", onStatus);
      es.close();
    };
  }, [runId, shouldStream, queryClient]);
}

function isTerminalStatus(status: string): boolean {
  return status === "completed" || status === "failed" || status === "cancelled";
}

function SummaryCard({
  summary,
  onCancel,
  cancelling,
  onRetry,
  retrying,
}: {
  summary: RunSummary;
  onCancel: () => void;
  cancelling: boolean;
  onRetry: () => void;
  retrying: boolean;
}) {
  const tone = STATUS_TONE[summary.status] ?? "default";
  const inflight = summary.status === "queued" || summary.status === "running";
  const terminal = isTerminalStatus(summary.status);
  const canRetry = summary.status === "failed";
  const [downloading, setDownloading] = useState(false);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const agentRunId = traceRunId(summary);

  async function handleDownload() {
    setDownloadError(null);
    setDownloading(true);
    try {
      await downloadEvalRunExport(summary.id);
    } catch (err) {
      setDownloadError(err instanceof Error ? err.message : String(err));
    } finally {
      setDownloading(false);
    }
  }

  return (
    <Card className="p-5">
      <div className="flex items-center justify-between mb-4">
        <div>
          <div className="flex items-center">
            <div className="text-text-3 text-[12px] font-mono">{summary.id}</div>
            <Link
              to={`/agent-runs/${encodeURIComponent(agentRunId)}`}
              className="text-[12px] text-info hover:underline ml-3"
            >
              View agent trace →
            </Link>
          </div>
          <div className="text-text-2 text-[12px] mt-1">
            strategy{" "}
            <code className="font-mono text-text">
              {summary.agent_id.slice(0, 12)}
            </code>
          </div>
        </div>
        <div className="flex items-center gap-3">
          {inflight ? (
            <button
              type="button"
              aria-label={`Stop eval run ${summary.id}`}
              onClick={onCancel}
              disabled={cancelling}
              className="rounded-sm border border-warn/40 bg-warn/[0.08] px-2.5 py-1 text-[12px] text-warn hover:border-warn/70 hover:bg-warn/[0.14] hover:text-text disabled:opacity-50"
            >
              {cancelling ? "Stopping..." : "Stop eval"}
            </button>
          ) : null}
          {canRetry ? (
            <button
              type="button"
              aria-label={`Retry eval run ${summary.id}`}
              onClick={onRetry}
              disabled={retrying}
              className="rounded-sm border border-info/40 bg-info/[0.08] px-2.5 py-1 text-[12px] text-info hover:border-info/70 hover:bg-info/[0.14] hover:text-text disabled:opacity-50"
            >
              {retrying ? "Retrying..." : "Retry"}
            </button>
          ) : null}
          {terminal ? (
            <button
              type="button"
              aria-label={`Download eval run ${summary.id} as JSON`}
              onClick={handleDownload}
              disabled={downloading}
              className="rounded-sm border border-border bg-surface-elev px-2.5 py-1 text-[12px] text-text-2 hover:border-gold/40 hover:text-text disabled:opacity-50"
            >
              {downloading ? "Preparing JSON…" : "Download JSON"}
            </button>
          ) : null}
          <Pill tone={tone} animated={summary.status === "running"}>
            <span
              className="w-1.5 h-1.5 rounded-full"
              style={dotColor(tone)}
            />
            {summary.status}
          </Pill>
        </div>
      </div>
      {downloadError ? (
        <div className="mb-4 rounded-sm border border-danger/30 bg-danger/[0.06] px-2 py-1 text-[12px] text-danger">
          Download failed: {downloadError}
        </div>
      ) : null}

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
        <Metric label="Tokens" value={fmtTokens(summary)} />
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

type DecisionFilter = "all" | "buy" | "sell" | "hold" | "close";

function DecisionsPanel({ rows }: { rows: DecisionRowDto[] }) {
  const [filter, setFilter] = useState<DecisionFilter>("all");
  const counts = useMemo(() => decisionCounts(rows), [rows]);
  const filtered = useMemo(
    () => rows.filter((row) => filter === "all" || decisionKind(row.action) === filter),
    [rows, filter],
  );

  return (
    <Card>
      {rows.length === 0 ? (
        <EmptyDecisions />
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-2 border-b border-border-soft px-4 py-3">
            {(["all", "buy", "sell", "hold", "close"] as DecisionFilter[]).map((value) => (
              <button
                key={value}
                type="button"
                onClick={() => setFilter(value)}
                className={`dec-filter ${filter === value ? "dec-filter--active" : ""}`}
                aria-pressed={filter === value}
              >
                <span>{decisionFilterLabel(value)}</span>
                <span className="dec-filter__count">{counts[value]}</span>
              </button>
            ))}
          </div>
          <div className="xvn-scroll xvn-scroll--always max-h-[520px] overflow-x-auto">
            <DecisionsTable rows={filtered} />
          </div>
        </>
      )}
    </Card>
  );
}

function DecisionsTable({ rows }: { rows: DecisionRowDto[] }) {
  return (
    <table className="w-full min-w-[980px]">
      <thead>
        <tr className="sticky top-0 z-10 bg-surface-card text-left text-text-2 text-[12px] border-b border-border-soft">
          <th className="font-normal py-2.5 px-5">#</th>
          <th className="font-normal py-2.5 px-3">Time</th>
          <th className="font-normal py-2.5 px-3">Asset</th>
          <th className="font-normal py-2.5 px-3">Action</th>
          <th className="font-normal py-2.5 px-3 text-right">Conviction</th>
          <th className="font-normal py-2.5 px-3 text-right">Size</th>
          <th className="font-normal py-2.5 px-3 text-right">Fill</th>
          <th className="font-normal py-2.5 px-3 text-right">PnL</th>
          <th className="font-normal py-2.5 px-3">Reasoning</th>
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
            <td className="py-2.5 px-3">
              <DecisionSignal action={r.action} />
            </td>
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
            <td className="py-2.5 px-3 text-text-2 text-[12px] leading-snug max-w-[320px]">
              {decisionReasoning(r)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function DecisionSignal({ action }: { action: string }) {
  const kind = decisionKind(action);
  return (
    <span className={`dec-pill dec-pill--${kind}`}>
      <span className="dec-pill__label">{decisionActionLabel(kind)}</span>
      <span className="dec-pill__raw">{action}</span>
    </span>
  );
}

function decisionKind(action: string): Exclude<DecisionFilter, "all"> {
  if (action === "long_open") return "buy";
  if (action === "short_open") return "sell";
  if (action === "flat") return "close";
  return "hold";
}

function decisionCounts(rows: DecisionRowDto[]): Record<DecisionFilter, number> {
  return rows.reduce<Record<DecisionFilter, number>>(
    (acc, row) => {
      acc.all += 1;
      acc[decisionKind(row.action)] += 1;
      return acc;
    },
    { all: 0, buy: 0, sell: 0, hold: 0, close: 0 },
  );
}

function decisionFilterLabel(filter: DecisionFilter): string {
  return filter === "all" ? "All" : decisionActionLabel(filter);
}

function decisionActionLabel(filter: Exclude<DecisionFilter, "all">): string {
  return {
    buy: "BUY",
    sell: "SELL",
    hold: "HOLD",
    close: "CLOSE",
  }[filter];
}

function decisionReasoning(row: DecisionRowDto): string {
  const extended = row as DecisionRowDto & { reasoning?: string | null };
  return extended.reasoning?.trim() || row.justification?.trim() || "—";
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

function fmtTokens(summary: RunSummary): string {
  const total =
    (summary.actual_input_tokens ?? 0) +
    (summary.actual_output_tokens ?? 0);
  return total > 0 ? total.toLocaleString() : "—";
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

function traceRunId(summary: RunSummary): string {
  const withTraceId = summary as RunSummary & { agent_run_id?: string | null };
  return withTraceId.agent_run_id ?? summary.id;
}

// Defensive viewport check: when matchMedia is absent (jsdom, SSR), default
// to desktop so existing tests keep targeting the desktop layout.
function useIsPhone(): boolean {
  const [isPhone, setIsPhone] = useState(() => {
    if (typeof window === "undefined") return false;
    if (typeof window.matchMedia !== "function") return false;
    return window.matchMedia("(max-width: 767px)").matches;
  });
  useEffect(() => {
    if (typeof window === "undefined") return;
    if (typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia("(max-width: 767px)");
    const update = () => setIsPhone(mq.matches);
    mq.addEventListener("change", update);
    return () => mq.removeEventListener("change", update);
  }, []);
  return isPhone;
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
