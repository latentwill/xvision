import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams, Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  cancelRun,
  deleteRun,
  downloadEvalRunExport,
  evalKeys,
  getRun,
  listRuns,
  retryRun,
} from "@/api/eval";
import { chartKeys, getRunChart, openRunStream } from "@/api/chart";
import { RunChart } from "@/components/chart/RunChart";
import { ReviewPanel } from "@/features/eval-runs/review";
import { RunSummaryError as RunSummaryPanel } from "@/features/eval-runs/RunSummary";
import { useAdaptivePoll } from "@/features/eval-runs/useAdaptivePoll";
import { useTraceDock } from "@/stores/trace-dock";
import { isInflightRunStatus } from "@/lib/run-status";
import {
  evalRunDisambiguator,
  evalRunLabels,
  type EvalRunLabels,
} from "@/lib/run-display";
import { listScenarios, scenarioKeys } from "@/api/scenarios";
import { getStrategy, listStrategies, strategyKeys } from "@/api/strategies";
import { agentKeys, listAgents, type Agent } from "@/api/agents";
import { agentRunKeys, getAgentRun } from "@/api/agent-runs";
import { formatCostUsd, formatCostUsdPrecise } from "@/lib/format";
import type {
  DecisionRowDto,
  RunDetail,
  RunSummary,
} from "@/api/types.gen";
import {
  derivePositionsByDecision,
  derivePriorSideByDecision,
  type PositionSide,
  type OpenPosition,
} from "@/features/decisions/positions";
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
  // Status-aware adaptive cadence — see `useAdaptivePoll` for the
  // schedule (running=2s, queued=5s, terminal=stop, 5min idle→30s).
  // The hook returns a `(status) => interval` callable that owns the
  // "ms since last status change" state via refs, so the 5-min stale
  // backoff fires correctly even when nothing in the React tree
  // re-renders. We pull the latest status off the cache rather than
  // routing it through hook deps so we don't have to call the hook
  // *after* useQuery and run into TDZ ordering.
  const pollFor = useAdaptivePoll(id);
  const q = useQuery({
    queryKey: evalKeys.run(id),
    queryFn: () => getRun(id),
    enabled: id.length > 0,
    refetchInterval: (query) => pollFor(query.state.data?.summary.status),
  });
  const chart = useQuery({
    queryKey: chartKeys.run(id),
    queryFn: () => getRunChart(id),
    enabled: !!id,
  });
  const strategies = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });
  const scenarios = useQuery({
    queryKey: scenarioKeys.list(),
    queryFn: () => listScenarios(),
  });
  // F-1 (qa-round-7): the inspector top bar needs the strategy's attached
  // agents so each can route to its detail page. `listStrategies()` only
  // returns slim `StrategyListItem` rows — fetch the full strategy to get
  // `agents: AgentRef[]`. Gated on the run's strategy id (`summary.agent_id`
  // is the pre-mint strategy id, NOT to be confused with `Agent` records;
  // see CLAUDE.md terminology lock).
  const strategyIdForRun = q.data?.summary.agent_id ?? "";
  const strategyDetail = useQuery({
    queryKey: strategyKeys.detail(strategyIdForRun),
    queryFn: () => getStrategy(strategyIdForRun),
    enabled: strategyIdForRun.length > 0,
  });
  // Pull every agent so we can map agent_id → display name in the top-bar
  // chips. Cheap and cached; agents page hits the same query.
  const agentsAll = useQuery({
    queryKey: agentKeys.list(),
    queryFn: () => listAgents(),
  });
  // Sibling runs for the same strategy power the "Run #N/M" disambiguator.
  // The list-runs API already filters by agent_id; we narrow to the same
  // scenario client-side.
  const agentId = q.data?.summary.agent_id ?? "";
  const siblings = useQuery({
    queryKey: evalKeys.runs({ agent_id: agentId || undefined }),
    queryFn: () => listRuns({ agent_id: agentId || undefined }),
    enabled: agentId.length > 0,
  });
  // F-8 (qa-round-7): linked agent run carries the per-call cost rows.
  // We display its pre-rolled `total_cost_usd` so the summary matches the
  // capsule's number exactly — both ultimately come from the same SQL
  // aggregation over `model_call_cost_usd`, so there's no double-counting
  // worry. Falls back to the eval-run id when an explicit
  // `agent_run_id` isn't on the summary (older runs / mocks).
  const agentRunIdForCost = q.data ? traceRunId(q.data.summary) : "";
  const linkedAgentRun = useQuery({
    queryKey: agentRunKeys.run(agentRunIdForCost),
    queryFn: () => getAgentRun(agentRunIdForCost),
    enabled: agentRunIdForCost.length > 0,
    // Cost is a terminal-state stat; don't burn requests while the run is
    // still inflight (the eval-run query is already polling on adaptive
    // cadence, and agent-run cost only finalizes once the agent completes).
    refetchInterval: false,
    retry: false,
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
  const remove = useMutation({
    mutationFn: deleteRun,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: evalKeys.all });
      navigate("/eval-runs");
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
      .setActiveRun(id, status && isInflightRunStatus(status) ? "live" : "post-hoc");
  }, [id, q.data?.summary.status]);

  // Drop the active run from the trace-dock store on unmount so the
  // floating capsule doesn't bleed onto the eval list or any other
  // route after the operator navigates away from the inspector.
  useEffect(() => {
    return () => {
      const dock = useTraceDock.getState();
      if (dock.activeRunId === id) {
        dock.setActiveRun(null, "post-hoc");
      }
    };
  }, [id]);

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
  const labels = evalRunLabels(
    detail.summary,
    strategies.data ?? [],
    scenarios.data ?? [],
  );
  const disambiguator = evalRunDisambiguator(
    detail.summary,
    siblings.data ?? [],
  );
  if (isPhone) {
    return (
      <MobileEvalRunDetail
        detail={detail}
        labels={labels}
        disambiguator={disambiguator}
        onCancel={() => cancel.mutate(detail.summary.id)}
        cancelling={cancel.variables === detail.summary.id && cancel.isPending}
        onRetry={() => retry.mutate(detail.summary.id)}
        retrying={retry.variables === detail.summary.id && retry.isPending}
        onDelete={() => remove.mutate(detail.summary.id)}
        deleting={remove.variables === detail.summary.id && remove.isPending}
      />
    );
  }
  return (
    <>
      <Topbar
        title={labels.title}
        sub={`${labels.subtitle} · ${disambiguator}`}
      />

      <InspectorContextStrip
        strategyId={detail.summary.agent_id}
        strategyName={labels.strategyName}
        scenarioId={detail.summary.scenario_id}
        scenarioName={labels.scenarioName}
        agents={strategyDetail.data?.agents ?? []}
        agentsAll={agentsAll.data ?? []}
      />

      <SummaryCard
        summary={detail.summary}
        equityCurve={detail.equity_curve}
        labels={labels}
        disambiguator={disambiguator}
        totalCostUsd={linkedAgentRun.data?.summary.total_cost_usd ?? null}
        onCancel={() => cancel.mutate(detail.summary.id)}
        cancelling={cancel.variables === detail.summary.id && cancel.isPending}
        onRetry={() => retry.mutate(detail.summary.id)}
        retrying={retry.variables === detail.summary.id && retry.isPending}
        retryError={
          retry.isError && retry.error
            ? retry.error instanceof Error
              ? retry.error.message
              : String(retry.error)
            : null
        }
        onDelete={() => remove.mutate(detail.summary.id)}
        deleting={remove.variables === detail.summary.id && remove.isPending}
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
  equityCurve,
  labels,
  disambiguator,
  totalCostUsd,
  onCancel,
  cancelling,
  onRetry,
  retrying,
  retryError,
  onDelete,
  deleting,
}: {
  summary: RunSummary;
  equityCurve: ReadonlyArray<{ equity_usd: number }>;
  labels: EvalRunLabels;
  disambiguator: string;
  totalCostUsd: number | null;
  onCancel: () => void;
  cancelling: boolean;
  onRetry: () => void;
  retrying: boolean;
  retryError: string | null;
  onDelete: () => void;
  deleting: boolean;
}) {
  const tone = STATUS_TONE[summary.status] ?? "default";
  const inflight = isInflightRunStatus(summary.status);
  const terminal = isTerminalStatus(summary.status);
  // Three statuses can re-enqueue:
  // - `failed` / `cancelled` → "Retry" (recovery after a fix or stop)
  // - `completed` → "Rerun" (re-test the same agent/scenario for a
  //   fresh trace; useful for verifying result stability)
  // The button label + tooltip adapt below so the operator can tell
  // the two semantics apart at a glance. The engine classifies the
  // request as `RetryReason::FailureRecovery` vs `RetryReason::ManualRerun`
  // for the audit log; if the backend rejects, the existing
  // `retry.isError` path surfaces a classified error.
  const canRetry =
    summary.status === "failed" ||
    summary.status === "cancelled" ||
    summary.status === "completed";
  const isRerun = summary.status === "completed";
  const retryLabel = isRerun ? "Rerun" : "Retry";
  const retryInflightLabel = isRerun ? "Rerunning…" : "Retrying...";
  const retryTooltip = isRerun
    ? "Rerun: produces a fresh trace against the same agent/scenario inputs. Useful for re-testing a fix or verifying result stability."
    : "Retry: re-enqueue with the same inputs.";
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
    <Card className="p-5 !border-border-soft">
      <Link
        to="/eval-runs"
        className="inline-flex items-center gap-1.5 text-[12px] text-text-2 hover:text-text mb-3"
      >
        ← Back to runs
      </Link>
      <div className="flex items-center justify-between mb-4">
        <div className="min-w-0">
          <div className="font-serif text-[30px] leading-none text-text truncate">
            {labels.strategyName}
          </div>
          <div
            data-testid="eval-run-id"
            className="mt-1 font-mono text-[12px] text-text-3 break-all select-all"
            aria-label={`Eval run id ${summary.id}`}
          >
            {summary.id}
          </div>
          <div className="mt-1 text-[14px] text-text-2 truncate">
            {labels.scenarioName}
          </div>
          <div
            data-testid="eval-run-meta"
            className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-text-3"
          >
            <span className="text-text-2">{disambiguator}</span>
            <span
              className="font-mono"
              title={summary.id}
              aria-label={`Run id ${summary.id}`}
            >
              run {labels.shortRunId}
            </span>
            <Link
              to={`/agent-runs/${encodeURIComponent(agentRunId)}`}
              className="text-info hover:underline"
            >
              View agent trace →
            </Link>
          </div>
        </div>
        {/*
          Each visible button takes `min-w-[16ch]` so the column floor
          is the widest natural label ("Preparing JSON…" with padding).
          The previous `grid grid-flow-col auto-cols-fr` shell only
          equalizes columns when the grid has an explicit container
          width — in an unconstrained inline-grid `1fr` collapses to
          content size, so the operator still saw mismatched widths.
          The status pill stays outside the grid so it keeps its
          natural size. See `qa-eval-inspector-buttons-actually-uniform`.
        */}
        <div className="flex items-center gap-3">
          <div
            data-testid="eval-run-actions"
            className="flex items-center gap-3"
          >
            {inflight ? (
              <button
                type="button"
                aria-label={`Stop eval run ${summary.id}`}
                onClick={onCancel}
                disabled={cancelling}
                className="min-w-[16ch] rounded-sm border border-warn/40 bg-warn/[0.08] px-2.5 py-1 text-[12px] text-warn hover:border-warn/70 hover:bg-warn/[0.14] hover:text-text disabled:opacity-50"
              >
                {cancelling ? "Stopping..." : "Stop eval"}
              </button>
            ) : null}
            {canRetry ? (
              <button
                type="button"
                aria-label={`${retryLabel} eval run ${summary.id}`}
                title={retryTooltip}
                onClick={onRetry}
                disabled={retrying}
                className="min-w-[16ch] rounded-sm border border-info/40 bg-info/[0.08] px-2.5 py-1 text-[12px] text-info hover:border-info/70 hover:bg-info/[0.14] hover:text-text disabled:opacity-50"
              >
                {retrying ? retryInflightLabel : retryLabel}
              </button>
            ) : null}
            {terminal ? (
              <button
                type="button"
                aria-label={`Download eval run ${summary.id} as JSON`}
                onClick={handleDownload}
                disabled={downloading}
                className="min-w-[16ch] rounded-sm border border-border-soft bg-surface-elev px-2.5 py-1 text-[12px] text-text-2 hover:border-gold/40 hover:text-text disabled:opacity-50"
              >
                {downloading ? "Preparing JSON…" : "Download JSON"}
              </button>
            ) : null}
            <button
              type="button"
              aria-label={`Delete eval run ${summary.id}`}
              onClick={onDelete}
              disabled={deleting}
              className="min-w-[16ch] rounded-sm border border-danger/40 bg-danger/[0.06] px-2.5 py-1 text-[12px] text-danger hover:border-danger/70 hover:bg-danger/[0.12] hover:text-text disabled:opacity-50"
            >
              {deleting ? "Deleting…" : "Delete"}
            </button>
          </div>
          <Pill tone={tone} animated={inflight}>
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
      {retryError ? (
        <div
          role="status"
          data-testid="eval-retry-error"
          className="mb-4 rounded-sm border border-danger/30 bg-danger/[0.06] px-2 py-1 text-[12px] text-danger"
        >
          {isRerun ? "Rerun failed" : "Retry failed"}: {retryError}
        </div>
      ) : null}

      <div className="grid grid-cols-3 gap-x-8 gap-y-3">
        <Metric label="Sharpe" value={fmtNumber(summary.sharpe)} />
        <Metric label="Max DD" value={fmtPct(summary.max_drawdown_pct)} />
        <Metric label="Total return" value={fmtPct(summary.total_return_pct)} />
        <Metric
          label="Total PnL"
          value={fmtPnlUsd(totalPnlUsd(equityCurve))}
          tone={pnlTone(totalPnlUsd(equityCurve))}
        />
        <Metric label="Mode" value={summary.mode} />
        <Metric label="Started" value={fmtTime(summary.started_at)} />
        <Metric
          label="Completed"
          value={summary.completed_at ? fmtTime(summary.completed_at) : "—"}
        />
        <Metric label="Tokens" value={fmtTokens(summary)} />
        {/*
          F-8 (qa-round-7): total inference cost stat lives next to Tokens.
          Both ultimately sum over the per-call `model_call_cost_usd` rows
          recorded by xvision-observability — agent-runs API rolls them up
          server-side as `total_cost_usd` so we don't double-aggregate
          here. Falls back to em-dash when the linked agent run hasn't
          loaded (or is missing for older runs); the `title` attribute
          surfaces full precision on hover, matching the trace surfaces.
        */}
        <Metric
          label="Total cost (USD)"
          value={formatCostUsd(totalCostUsd)}
          titleValue={
            totalCostUsd != null && Number.isFinite(totalCostUsd)
              ? formatCostUsdPrecise(totalCostUsd)
              : undefined
          }
        />
      </div>

      <RunSummaryPanel error={summary.error} />
    </Card>
  );
}

function Metric({
  label,
  value,
  tone = "neutral",
  titleValue,
}: {
  label: string;
  value: string;
  tone?: "pos" | "neg" | "neutral";
  /** Tooltip text — typically a full-precision form of `value`. */
  titleValue?: string;
}) {
  const valueClass =
    tone === "pos" ? "text-gold" : tone === "neg" ? "text-danger" : "text-text";
  return (
    <div>
      <div className="text-text-3 text-[11px] uppercase tracking-wide mb-1">
        {label}
      </div>
      <div className={`font-mono ${valueClass}`} title={titleValue}>
        {value}
      </div>
    </div>
  );
}

function totalPnlUsd(
  equityCurve: ReadonlyArray<{ equity_usd: number }>,
): number | null {
  if (equityCurve.length < 2) return null;
  const start = equityCurve[0]?.equity_usd;
  const end = equityCurve[equityCurve.length - 1]?.equity_usd;
  if (start == null || end == null) return null;
  return end - start;
}

function fmtPnlUsd(pnl: number | null): string {
  if (pnl == null) return "—";
  const abs = Math.abs(pnl);
  const formatted = abs.toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  if (pnl > 0) return `+$${formatted}`;
  if (pnl < 0) return `−$${formatted}`;
  return `$${formatted}`;
}

function pnlTone(pnl: number | null): "pos" | "neg" | "neutral" {
  if (pnl == null) return "neutral";
  if (pnl > 0) return "pos";
  if (pnl < 0) return "neg";
  return "neutral";
}

type DecisionFilter = "all" | "buy" | "short" | "sell" | "cover" | "hold";
type DecisionKind = Exclude<DecisionFilter, "all">;

function DecisionsPanel({ rows }: { rows: DecisionRowDto[] }) {
  const [filter, setFilter] = useState<DecisionFilter>("all");
  // Derive open positions across the FULL unfiltered sequence — filtering
  // is purely display-side, but a close row's "positions after close = []"
  // only holds if we've walked every preceding fill. Prior-side walk runs
  // in lockstep so the action-pill can distinguish sell-a-long from
  // cover-a-short on rows whose on-the-wire action is `"flat"`.
  const positionsByDecision = useMemo(() => derivePositionsByDecision(rows), [rows]);
  const priorSideByDecision = useMemo(() => derivePriorSideByDecision(rows), [rows]);
  const counts = useMemo(
    () => decisionCounts(rows, priorSideByDecision),
    [rows, priorSideByDecision],
  );
  const filtered = useMemo(
    () =>
      rows.filter(
        (row) =>
          filter === "all" ||
          decisionKind(row.action, priorSideByDecision.get(row.decision_index) ?? "flat") ===
            filter,
      ),
    [rows, filter, priorSideByDecision],
  );

  return (
    <Card>
      {rows.length === 0 ? (
        <EmptyDecisions />
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-2 border-b border-border-soft px-4 py-3">
            {(["all", "buy", "short", "sell", "cover", "hold"] as DecisionFilter[]).map(
              (value) => (
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
              ),
            )}
          </div>
          <div className="xvn-scroll xvn-scroll--always max-h-[520px] overflow-x-auto">
            <DecisionsTable
              rows={filtered}
              positionsByDecision={positionsByDecision}
              priorSideByDecision={priorSideByDecision}
            />
          </div>
        </>
      )}
    </Card>
  );
}

function DecisionsTable({
  rows,
  positionsByDecision,
  priorSideByDecision,
}: {
  rows: DecisionRowDto[];
  positionsByDecision: Map<number, OpenPosition[]>;
  priorSideByDecision: Map<number, PositionSide>;
}) {
  return (
    <table className="w-full min-w-[1140px]">
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
          <th className="font-normal py-2.5 px-3">Open positions</th>
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
              <DecisionSignal
                action={r.action}
                priorSide={priorSideByDecision.get(r.decision_index) ?? "flat"}
              />
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
            <td
              className="py-2.5 px-3 font-mono text-[12px]"
              data-testid={`decision-open-positions-${r.decision_index}`}
            >
              <OpenPositionsCell positions={positionsByDecision.get(r.decision_index) ?? []} />
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

function OpenPositionsCell({ positions }: { positions: OpenPosition[] }) {
  if (positions.length === 0) {
    return (
      <span className="text-text-3" data-testid="decision-open-positions-flat">
        flat
      </span>
    );
  }
  return (
    <div className="flex flex-wrap gap-1.5">
      {positions.map((p) => (
        <span
          key={p.asset}
          className={`dec-pos dec-pos--${p.side}`}
          title={`${p.asset} ${p.side} ${p.qty} @ ${p.entry_price}`}
        >
          <span className="dec-pos__asset">{p.asset}</span>
          <span className="dec-pos__side">{p.side}</span>
          <span className="dec-pos__qty">{fmtPositionQty(p.qty)}</span>
          <span className="dec-pos__sep">@</span>
          <span className="dec-pos__entry">{fmtPositionEntry(p.entry_price)}</span>
        </span>
      ))}
    </div>
  );
}

function fmtPositionQty(qty: number): string {
  // Strategy-config risk_pct lands roughly anywhere in 0.0001..10 units;
  // 4 sig figs is wide enough without overflowing the cell.
  if (qty === 0) return "0";
  if (Math.abs(qty) >= 1000) return qty.toLocaleString("en-US", { maximumFractionDigits: 2 });
  return qty.toPrecision(4).replace(/\.?0+$/, "");
}

function fmtPositionEntry(price: number): string {
  if (price === 0) return "0";
  if (price >= 1000) return price.toLocaleString("en-US", { maximumFractionDigits: 0 });
  if (price >= 1) return price.toFixed(2);
  return price.toPrecision(4);
}

function DecisionSignal({
  action,
  priorSide,
}: {
  action: string;
  priorSide: PositionSide;
}) {
  const kind = decisionKind(action, priorSide);
  return (
    <span className={`dec-pill dec-pill--${kind}`}>
      <span className="dec-pill__label">{decisionActionLabel(kind)}</span>
      <span className="dec-pill__raw">{action}</span>
    </span>
  );
}

function decisionKind(action: string, priorSide: PositionSide): DecisionKind {
  if (action === "long_open") return "buy";
  if (action === "short_open") return "short";
  if (action === "flat") {
    if (priorSide === "long") return "sell";
    if (priorSide === "short") return "cover";
    // flat-from-flat is a no-op; render neutrally rather than mint a
    // misleading SELL/COVER pill on a row that closed nothing.
    return "hold";
  }
  return "hold";
}

function decisionCounts(
  rows: DecisionRowDto[],
  priorSideByDecision: Map<number, PositionSide>,
): Record<DecisionFilter, number> {
  return rows.reduce<Record<DecisionFilter, number>>(
    (acc, row) => {
      acc.all += 1;
      const prior = priorSideByDecision.get(row.decision_index) ?? "flat";
      acc[decisionKind(row.action, prior)] += 1;
      return acc;
    },
    { all: 0, buy: 0, short: 0, sell: 0, cover: 0, hold: 0 },
  );
}

function decisionFilterLabel(filter: DecisionFilter): string {
  return filter === "all" ? "All" : decisionActionLabel(filter);
}

function decisionActionLabel(filter: DecisionKind): string {
  return {
    buy: "BUY",
    short: "SHORT",
    sell: "SELL",
    cover: "COVER",
    hold: "HOLD",
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

/**
 * F-1 (qa-round-7): inline context strip on the eval inspector.
 *
 * Surfaces the three things an operator wants to one-click into when
 * triaging a run — the Strategy that produced it, the Scenario it was
 * evaluated against, and the Agent objects attached to the strategy.
 * Each pill is a `<Link>` to the corresponding detail route. No popups,
 * no hover-cards — per the CLAUDE.md "no popups" rule, the strip is a
 * flat row that lives directly under the topbar and above the summary
 * card.
 *
 * Strategy / scenario chips render even before their detail / list
 * queries resolve (we always have the id and the label-derived display
 * name). Agent chips depend on the strategy detail query (`agents[]`
 * lives on the full `Strategy` shape, not on the slim
 * `StrategyListItem`) and fall back to the raw agent id if the global
 * `listAgents()` lookup hasn't completed.
 */
function InspectorContextStrip({
  strategyId,
  strategyName,
  scenarioId,
  scenarioName,
  agents,
  agentsAll,
}: {
  strategyId: string;
  strategyName: string;
  scenarioId: string;
  scenarioName: string;
  agents: { agent_id: string; role: string }[];
  agentsAll: Agent[];
}) {
  const agentNameById = new Map(agentsAll.map((a) => [a.agent_id, a.name]));
  return (
    <div
      data-testid="eval-inspector-context-strip"
      className="mb-3 flex flex-wrap items-center gap-2 rounded-sm border border-border-soft bg-surface-elev/40 px-3 py-2 text-[11px]"
    >
      <ContextPill
        kind="Strategy"
        to={`/strategies/${encodeURIComponent(strategyId)}`}
        label={strategyName}
        idForAria={strategyId}
      />
      <span className="text-text-4">·</span>
      <ContextPill
        kind="Scenario"
        to={`/scenarios/${encodeURIComponent(scenarioId)}`}
        label={scenarioName}
        idForAria={scenarioId}
      />
      {agents.length > 0 ? (
        <>
          <span className="text-text-4">·</span>
          <span className="text-[10px] font-mono tracking-[0.18em] text-text-3 uppercase">
            Agents
          </span>
          <div className="flex flex-wrap items-center gap-1.5">
            {agents.map((ref) => (
              <ContextPill
                key={`${ref.agent_id}:${ref.role}`}
                kind={ref.role}
                to={`/agents/${encodeURIComponent(ref.agent_id)}`}
                label={agentNameById.get(ref.agent_id) ?? ref.agent_id}
                idForAria={ref.agent_id}
                compact
              />
            ))}
          </div>
        </>
      ) : null}
    </div>
  );
}

function ContextPill({
  kind,
  to,
  label,
  idForAria,
  compact = false,
}: {
  kind: string;
  to: string;
  label: string;
  idForAria: string;
  compact?: boolean;
}) {
  return (
    <Link
      to={to}
      aria-label={`Open ${kind} ${label} (${idForAria})`}
      className={`inline-flex items-center gap-1.5 rounded-sm border border-border-soft px-2 py-0.5 text-text-2 hover:border-gold/50 hover:text-text dark:hover:border-gold/40 ${
        compact ? "text-[11px]" : "text-[11px]"
      }`}
    >
      <span className="text-[9px] font-mono tracking-[0.18em] text-text-3 uppercase">
        {kind}
      </span>
      <span className="font-mono truncate max-w-[28ch]">{label}</span>
    </Link>
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
