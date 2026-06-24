import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams, Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Card } from "@/components/primitives/Card";
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
import { RunChartV2 } from "@/components/chart/v2/surfaces/RunChartV2";
import { runChartPayloadToV2 } from "@/components/chart/v2/adapters/run-chart-payload";
import { ReviewPanel } from "@/features/eval-runs/review";
import { RunSummaryError as RunSummaryPanel } from "@/features/eval-runs/RunSummary";
import { useAdaptivePoll } from "@/features/eval-runs/useAdaptivePoll";
import { useTraceDock } from "@/stores/trace-dock";
import { isInflightRunStatus } from "@/lib/run-status";
import { evalRunDisambiguator, evalRunLabels } from "@/lib/run-display";
import { listScenarios, scenarioKeys } from "@/api/scenarios";
import { getStrategy, listStrategies, strategyKeys } from "@/api/strategies";
import { agentKeys, listAgents } from "@/api/agents";
import { agentRunKeys, getAgentRun } from "@/api/agent-runs";
import { formatCostUsdPrecise, formatSpendUsd } from "@/lib/format";
import { drawdownMetricTone } from "@/lib/metric-tone";
import type {
  DecisionRowDto,
  FilterEventV1,
  FilterSummary,
  RunDetail,
  RunSummary,
} from "@/api/types.gen";
import { EvalTopBar } from "@/components/eval-detail/EvalTopBar";
import { MetaChip } from "@/components/eval-detail/MetaChip";
import { SignalsUsedChips } from "@/components/eval-detail/SignalsUsedChips";
import { ActionPill } from "@/components/eval-detail/ActionPill";
import { DecisionsTable } from "@/components/eval-detail/DecisionsTable";
import {
  shortAsset,
  toTimelineDecisions,
  type TimelineDecision,
} from "@/components/eval-detail/decision-view";
import { FilterSummaryPanel } from "@/features/eval-runs/FilterSummaryPanel";
import { FilterEventTimeline } from "@/features/eval-runs/FilterEventTimeline";
import {
  MobileEvalRunDetail,
  MobileEvalRunDetailError,
  MobileEvalRunDetailLoading,
} from "./eval-runs-detail-mobile";

export function EvalRunDetailRoute() {
  const { runId } = useParams<{ runId: string }>();
  const id = runId ?? "";
  const qc = useQueryClient();
  // Status-aware adaptive cadence — see `useAdaptivePoll` for the
  // schedule (running=2s, queued=5s, terminal=stop, 5min idle→30s).
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
  // The MetaChip row needs the strategy's attached agents so each can route to
  // its detail page. `listStrategies()` only returns slim rows — fetch the full
  // strategy to get `agents: AgentRef[]`. Gated on the run's strategy id
  // (`summary.agent_id` is the pre-mint strategy id; see CLAUDE.md terminology
  // lock).
  const strategyIdForRun = q.data?.summary.agent_id ?? "";
  const strategyDetail = useQuery({
    queryKey: strategyKeys.detail(strategyIdForRun),
    queryFn: () => getStrategy(strategyIdForRun),
    enabled: strategyIdForRun.length > 0,
  });
  // Pull every agent so we can map agent_id → display name in the chips.
  const agentsAll = useQuery({
    queryKey: agentKeys.list(),
    queryFn: () => listAgents(),
  });
  // Sibling runs for the same strategy power the "Run #N/M" disambiguator.
  const agentId = q.data?.summary.agent_id ?? "";
  const siblings = useQuery({
    queryKey: evalKeys.runs({ agent_id: agentId || undefined }),
    queryFn: () => listRuns({ agent_id: agentId || undefined }),
    enabled: agentId.length > 0,
  });
  // Linked agent run carries the per-call cost rows; display its pre-rolled
  // `total_cost_usd` so the summary matches the capsule's number exactly.
  const agentRunIdForCost = q.data ? traceRunId(q.data.summary) : "";
  const linkedAgentRun = useQuery({
    queryKey: agentRunKeys.run(agentRunIdForCost),
    queryFn: () => getAgentRun(agentRunIdForCost),
    enabled: agentRunIdForCost.length > 0,
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

  // QA30: the trace-dock keys on AGENT-RUN id (it fetches via
  // `getAgentRun(activeRunId)` and projects everything from that
  // summary's `financial_eval_id`). Setting `activeRun` to the
  // eval-run URL param was the source of the multi-eval bug — when
  // the user navigated to a sibling eval, the route changed, this
  // effect fired with the new eval-run id, the agent-run query
  // returned 404, the capsule fell back to its previous render, and
  // the user perceived the capsule as "not switching". Use the
  // linked agent-run id (via `traceRunId(summary)`) instead. Wait
  // until `q.data` is loaded so we have the mapping before flipping
  // the dock state.
  const traceRun = q.data ? traceRunId(q.data.summary) : "";
  useEffect(() => {
    if (!traceRun) return;
    const status = q.data?.summary.status;
    useTraceDock
      .getState()
      .setActiveRun("eval", traceRun, status && isInflightRunStatus(status) ? "live" : "post-hoc");
  }, [traceRun, q.data?.summary.status]);

  // Drop the eval scope's active run on unmount so the floating capsule
  // doesn't bleed onto other routes after navigation. Unconditional
  // (WS-2): this route is the sole owner of the eval scope, so it always
  // nulls eval on the way out — no guard on the current run id, which
  // previously left the capsule stuck when a fast nav swapped runs.
  // Only the eval scope is touched; the live scope is independent.
  useEffect(() => {
    return () => {
      useTraceDock.getState().setActiveRun("eval", null, "post-hoc");
    };
  }, []);

  // Push the eval-side cost into the trace-dock so the floating capsule
  // renders the same number as the meta strip / SummaryCard. Without this
  // the capsule would read only the agent-run's `total_cost_usd`, which
  // can be 0/null when pricing was rolled up on the eval side only —
  // leaving the capsule showing "$0.00" while the meta strip shows the
  // real cost.
  useEffect(() => {
    if (!q.data) return;
    const cost = displayCost(
      q.data.summary,
      linkedAgentRun.data?.summary.total_cost_usd ?? null,
    );
    useTraceDock.getState().setCostOverrideUsd("eval", cost);
  }, [q.data, linkedAgentRun.data?.summary.total_cost_usd]);

  if (q.isPending) {
    if (isPhone) return <MobileEvalRunDetailLoading id={id} />;
    return (
      <>
        <EvalTopBar runId={id || "loading…"} status="queued" />
        <div className="px-6 py-6">
          <Card className="p-6 animate-pulse">
            <div className="h-5 w-72 bg-surface-elev rounded mb-3" />
            <div className="h-4 w-48 bg-surface-elev rounded" />
          </Card>
        </div>
      </>
    );
  }

  // A background refetch can fail transiently while React Query still has the
  // last good detail payload. Keep rendering that known run instead of
  // replacing the page with "Run not found"; only show the not-found/error
  // screen when there is no usable run data at all.
  if (!q.data) {
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
        <EvalTopBar runId={id} status="failed" />
        <div className="px-6 py-6">
          <ErrorState err={q.error} onRetry={() => q.refetch()} runId={id} />
        </div>
      </>
    );
  }

  const detail = q.data;
  const labels = evalRunLabels(
    detail.summary,
    strategies.data ?? [],
    scenarios.data ?? [],
  );
  const disambiguator = evalRunDisambiguator(detail.summary, siblings.data ?? []);
  if (isPhone) {
    return (
      <MobileEvalRunDetail
        detail={detail}
        labels={labels}
        disambiguator={disambiguator}
        agents={strategyDetail.data?.agents ?? []}
        agentsAll={agentsAll.data ?? []}
        totalCostUsd={linkedAgentRun.data?.summary.total_cost_usd || null}
        signalsUsed={detail.signals_used ?? undefined}
        onCancel={() => cancel.mutate(detail.summary.id)}
        cancelling={cancel.variables === detail.summary.id && cancel.isPending}
        onRetry={() => retry.mutate(detail.summary.id)}
        retrying={retry.variables === detail.summary.id && retry.isPending}
        onDelete={() => remove.mutate(detail.summary.id)}
        deleting={remove.variables === detail.summary.id && remove.isPending}
      />
    );
  }

  const primaryAgent = (strategyDetail.data?.agents ?? [])[0];
  const agentNameById = new Map(
    (agentsAll.data ?? []).map((a) => [a.agent_id, a.name]),
  );
  const agentChipValue = primaryAgent
    ? (agentNameById.get(primaryAgent.agent_id) ?? primaryAgent.agent_id)
    : null;

  return (
    <div className="-mx-4 -mt-4 flex flex-col min-h-0">
      <EvalTopBar runId={detail.summary.id} status={detail.summary.status} />

      <div className="flex-1 min-h-0 px-6 py-6">
        <div className="max-w-[1400px] mx-auto">
          {/* Body header */}
          <div className="mb-5">
            {/* Breadcrumb: run ID + meta */}
            <div
              data-testid="eval-run-meta"
              className="flex items-center gap-2 flex-wrap text-[11px] font-mono text-text-3 mb-3"
            >
              <span
                data-testid="eval-run-id"
                aria-label={`Eval run id ${detail.summary.id}`}
                className="tabular-nums select-all cursor-default"
              >
                {detail.summary.id}
              </span>
              <span className="text-text-4">·</span>
              <span>
                started{" "}
                <span className="text-text-2">{fmtTime(detail.summary.started_at)}</span>
              </span>
              <span className="text-text-4">·</span>
              <span className="text-text-2">{disambiguator}</span>
            </div>

            {/* Focal metric: total return at display scale */}
            <h1
              className={[
                "text-5xl font-bold tabular-nums leading-none mb-3",
                detail.summary.total_return_pct == null
                  ? "text-text-3"
                  : detail.summary.total_return_pct > 0
                    ? "text-pos"
                    : detail.summary.total_return_pct < 0
                      ? "text-neg"
                      : "text-text",
              ].join(" ")}
            >
              <NumPop value={fmtPct(detail.summary.total_return_pct)} />
              <span className="text-[16px] font-normal text-text-3 ml-2 tracking-wide">TOTAL RETURN</span>
            </h1>

            {/* Stat rail */}
            <div className="flex items-center gap-3 flex-wrap text-[11px] font-mono tabular-nums text-text-3 mb-4">
              <span>SHARPE <NumPop className="text-text-2 ml-1" value={fmtNumber(detail.summary.sharpe)} /></span>
              <span className="text-text-4">·</span>
              <span>
                MAX DD{" "}
                <NumPop
                  className={`ml-1 ${drawdownMetricTone(detail.summary.max_drawdown_pct) === "neg" ? "text-neg" : "text-text-2"}`}
                  value={fmtPct(detail.summary.max_drawdown_pct)}
                />
              </span>
              <span className="text-text-4">·</span>
              <span>COST <NumPop className="text-text-2 ml-1" value={formatSpendUsd(displayCost(detail.summary, linkedAgentRun.data?.summary.total_cost_usd || null))} /></span>
            </div>

            <div className="flex items-center gap-2 flex-wrap">
              <MetaChip
                label="Strategy"
                value={labels.strategyName}
                tone="gold"
                ariaLabel={`Open Strategy ${labels.strategyName}`}
                onClick={() =>
                  navigate(`/strategies/${encodeURIComponent(detail.summary.agent_id)}`)
                }
              />
              {agentChipValue && primaryAgent ? (
                <MetaChip
                  label="Agent"
                  value={agentChipValue}
                  tone="info"
                  ariaLabel={`Open Agent ${agentChipValue}`}
                  onClick={() =>
                    navigate(`/agents/${encodeURIComponent(primaryAgent.agent_id)}`)
                  }
                />
              ) : null}
              <MetaChip
                label="Scenario"
                value={labels.scenarioName}
                tone="neutral"
                ariaLabel={detail.summary.mode === 'live' ? 'Forward Test' : `Open Scenario ${labels.scenarioName}`}
                onClick={
                  detail.summary.mode === 'live'
                    ? undefined
                    : () => navigate(`/scenarios/${encodeURIComponent(detail.summary.scenario_id)}`)
                }
              />
            </div>

            {/* Signals used — full-width inline chip strip.
                `signals_used` is an optional extension on RunDetail that the
                backend does not yet populate; renders nothing when absent. */}
            <SignalsUsedChips
              signals_used={detail.signals_used}
            />
          </div>

          {/*
            QA30: layout rule — no right-side boxes. The chat rail already
            eats the right edge of the desktop shell, so a `col-span-4`
            sidebar shrinks the center column where the chart + decisions
            live. MetaCard and ReviewPanel are now stacked above the center
            content as full-width strips. See CLAUDE.md "Frontend layout
            rule: no right-side boxes when the chat rail is visible".
          */}
          <div className="space-y-5">
            <MetaCard
              summary={detail.summary}
              totalCostUsd={linkedAgentRun.data?.summary.total_cost_usd || null}
            />

            <SummaryCard
              summary={detail.summary}
              equityCurve={detail.equity_curve}
              decisions={detail.decisions}
              totalCostUsd={linkedAgentRun.data?.summary.total_cost_usd || null}
              chartPending={chart.isPending}
              chartError={chart.isError ? String(chart.error) : null}
              chartNode={chart.data ? <RunChartV2 payload={runChartPayloadToV2(chart.data)} /> : null}
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

            {/* Multi-asset: per-asset decision rollup above the decisions list. */}
            <AssetRollupPanel decisions={detail.decisions} />

            {/*
              UI2: Filter activity. The two filter v1 panels (gate-check /
              suppression summary + per-bar fire/blocked timeline) are grouped
              under one inline section so operators read filter behavior in one
              place. Display-copy only — the panels still read the same data
              keys (`suppressed_in_position`, `wakeups`, `bars_scanned`,
              `filter_events`, …). Both panels return `null` when their input is
              empty, so the section collapses to just the heading + helper line
              for EveryBar runs; when BOTH are empty we render nothing at all.
              Single full-width inline section — no right sidebar, no popups.
            */}
            <FilterActivitySection
              summaries={detail.filter_summaries ?? []}
              events={detail.filter_events ?? []}
            />

            <DecisionsCard
              rows={detail.decisions}
              filterSummaries={detail.filter_summaries ?? []}
              status={detail.summary.status}
              errorMsg={detail.summary.error}
            />

            {/*
              `key={detail.summary.id}` resets ReviewPanel local state when
              the route is reused for a different run id. Inlined below the
              decisions list (was a right-side sidebar that shrank the
              chart on desktops with the chat rail open).
            */}
            <ReviewPanel
              key={detail.summary.id}
              runId={detail.summary.id}
              runCanReview={isTerminalStatus(detail.summary.status)}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

// ────────────────────────────────────────────────────────────────────────────

type LiveRunEvent =
  | { event: "decision"; data: DecisionRowDto }
  | { event: "status"; data: { phase: string; message: string | null } };

// Trailing-edge debounce window for refetching server-derived fields
// (`filter_events`, `filter_summaries`, `summary.error`) off the back of
// SSE traffic. The SSE stream only carries `decision` and `status`, so
// per-bar filter ticks would otherwise wait on the 2s adaptive poll.
const RUN_REFETCH_DEBOUNCE_MS = 250;

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

    let refetchTimer: ReturnType<typeof setTimeout> | null = null;
    const scheduleRunRefetch = () => {
      if (refetchTimer !== null) return;
      refetchTimer = setTimeout(() => {
        refetchTimer = null;
        queryClient.invalidateQueries({
          queryKey: evalKeys.run(runId),
          refetchType: "active",
        });
      }, RUN_REFETCH_DEBOUNCE_MS);
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
      scheduleRunRefetch();
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
        return;
      }
      scheduleRunRefetch();
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
      if (refetchTimer !== null) clearTimeout(refetchTimer);
      es.close();
    };
  }, [runId, shouldStream, queryClient]);
}

function isTerminalStatus(status: string): boolean {
  return status === "completed" || status === "failed" || status === "cancelled" || status === "disconnected";
}

function displayCost(summary: RunSummary, totalCostUsd: number | null): number | null {
  return summary.inference_cost_quote_total ?? totalCostUsd;
}

// ────────────────────────────────────────────────────────────────────────────

// Unified Summary action-row button. Quiet at rest — soft #141414 border on the
// elevated surface, semantic intent carried by text color — and the full accent
// (border + tint) only emerges on hover. No loud colored outlines boxing each
// label; the row reads as one toolbar rather than four competing chips. Tone
// classes append on top of this base.
const ACTION_BTN =
  "inline-flex items-center gap-1.5 rounded-sm border border-border-soft bg-surface-elev px-2.5 py-1 text-[12px] transition-colors disabled:opacity-50";

function SummaryCard({
  summary,
  equityCurve,
  decisions,
  totalCostUsd,
  chartPending,
  chartError,
  chartNode,
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
  decisions: DecisionRowDto[];
  totalCostUsd: number | null;
  chartPending: boolean;
  chartError: string | null;
  chartNode: React.ReactNode;
  onCancel: () => void;
  cancelling: boolean;
  onRetry: () => void;
  retrying: boolean;
  retryError: string | null;
  onDelete: () => void;
  deleting: boolean;
}) {
  const inflight = isInflightRunStatus(summary.status);
  const terminal = isTerminalStatus(summary.status);
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
  const displayedCostUsd = displayCost(summary, totalCostUsd);
  const verdict = summary.status === "completed" ? "PASS" : summary.status.toUpperCase();
  const totalPnl = totalPnlUsd(equityCurve) ?? summary.unrealized_pnl_usd ?? null;
  const realizedPnl = realizedPnlUsd(decisions);
  const unrealizedPnl = summary.unrealized_pnl_usd ?? unrealizedPnlUsd(totalPnl, realizedPnl);
  const decisionTape = useMemo(
    () =>
      toTimelineDecisions(decisions)
        .filter(
          (decision) =>
            decision.phase !== "filtered" &&
            decision.action &&
            decision.action !== "HOLD",
        )
        .slice(0, 6),
    [decisions],
  );

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
    <div className="bg-surface-card border border-border-soft rounded-card">
      <div
        className="flex items-center justify-between px-5 pt-4 pb-3"
        style={{ borderBottom: "1px solid var(--border-soft)" }}
      >
        <div className="flex items-baseline gap-3">
          <h2 className="m-0 font-sans text-[22px] tracking-tight text-text" style={{ fontWeight: 600 }}>
            Summary
          </h2>
          <span
            className="px-1.5 py-0.5 text-[9px] font-mono tracking-[0.18em] uppercase"
            style={{
              color: summary.status === "completed" ? "var(--gold)" : "var(--text-2)",
              background: summary.status === "completed" ? "var(--gold-bg)" : "var(--surface-elev)",
              border: `1px solid ${summary.status === "completed" ? "var(--gold-soft)" : "var(--border-strong)"}`,
              borderRadius: 4,
            }}
          >
            {verdict}
          </span>
        </div>
        <div data-testid="eval-run-actions" className="flex items-center gap-3">
          {inflight ? (
            <button
              type="button"
              aria-label={`Stop eval run ${summary.id}`}
              onClick={onCancel}
              disabled={cancelling}
              className={`${ACTION_BTN} text-warn hover:border-warn/40 hover:bg-warn/[0.08] hover:text-text`}
            >
              {cancelling ? "Stopping..." : "Stop eval"}
            </button>
          ) : null}
          <Link
            to={`/agent-runs/${encodeURIComponent(agentRunId)}`}
            className={`${ACTION_BTN} text-info hover:border-info/40 hover:text-text`}
          >
            View agent trace →
          </Link>
          {canRetry ? (
            <button
              type="button"
              aria-label={`${retryLabel} eval run ${summary.id}`}
              title={retryTooltip}
              onClick={onRetry}
              disabled={retrying}
              className={`${ACTION_BTN} text-text-2 hover:border-info/40 hover:bg-info/[0.08] hover:text-info`}
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
              className={`${ACTION_BTN} text-text-2 hover:border-gold/40 hover:text-text`}
            >
              {downloading ? "Preparing JSON…" : "Download JSON"}
            </button>
          ) : null}
          <button
            type="button"
            aria-label={`Delete eval run ${summary.id}`}
            onClick={onDelete}
            disabled={deleting}
            className={`${ACTION_BTN} text-text-3 hover:border-danger/40 hover:bg-danger/[0.08] hover:text-danger`}
          >
            {deleting ? "Deleting…" : "Delete"}
          </button>
        </div>
      </div>

      {downloadError ? (
        <div className="mx-5 mt-4 rounded-sm border border-danger/30 bg-danger/[0.06] px-2 py-1 text-[12px] text-danger">
          Download failed: {downloadError}
        </div>
      ) : null}
      {retryError ? (
        <div
          role="status"
          data-testid="eval-retry-error"
          className="mx-5 mt-4 rounded-sm border border-danger/30 bg-danger/[0.06] px-2 py-1 text-[12px] text-danger"
        >
          {isRerun ? "Rerun failed" : "Retry failed"}: {retryError}
        </div>
      ) : null}

      <DecisionTape decisions={decisionTape} />

      {/* Equity / run chart */}
      <div className="px-5 pt-4">
        {chartPending ? (
          <div className="text-text-3 text-[13px] text-center py-6">Loading chart…</div>
        ) : chartError ? (
          <div className="text-danger text-[13px] text-center py-6">
            Chart unavailable: {chartError}
          </div>
        ) : chartNode ? (
          chartNode
        ) : (
          <div className="text-text-3 text-[13px] text-center py-6">No chart data.</div>
        )}
      </div>

      {/* Stat grid */}
      <div
        className="mt-4 grid grid-cols-2 md:grid-cols-4"
        style={{ borderTop: "1px solid var(--border-soft)" }}
      >
        <Stat
          label="TOTAL PNL"
          value={fmtPnlUsd(totalPnl)}
          sub={`${fmtPct(summary.total_return_pct)} · ${pnlSplitSub(realizedPnl, unrealizedPnl)}`}
          tone={pnlTone(totalPnl)}
        />
        <Stat
          label="MAX DRAWDOWN"
          value={fmtPct(summary.max_drawdown_pct)}
          sub={summary.completed_at ? `@ ${fmtTime(summary.completed_at)}` : "in progress"}
          tone={drawdownMetricTone(summary.max_drawdown_pct) === "neg" ? "neg" : "neu"}
        />
        <Stat label="SHARPE" value={fmtNumber(summary.sharpe)} sub={unrealizedPnl != null ? `${fmtPnlUsd(unrealizedPnl)} unrealized` : "annualized"} tone="neu" />
        <Stat
          label="NET %"
          value={summary.net_return_pct != null ? fmtPct(summary.net_return_pct) : "—"}
          sub={`token cost ${formatSpendUsd(displayedCostUsd)}`}
          tone={
            summary.net_return_pct == null
              ? "neu"
              : summary.net_return_pct > 0
                ? "gold"
                : summary.net_return_pct < 0
                  ? "neg"
                  : "neu"
          }
          titleValue={
            displayedCostUsd != null && Number.isFinite(displayedCostUsd)
              ? formatCostUsdPrecise(displayedCostUsd)
              : undefined
          }
        />
      </div>

      {/* Live/forward-test delay counters */}
      {(summary.skipped_dispatches > 0 || summary.delayed_decisions > 0) && (
        <div
          className="mx-5 flex items-center gap-5 py-2"
          style={{ borderTop: "1px solid var(--border-soft)" }}
        >
          {summary.skipped_dispatches > 0 && (
            <div className="flex items-center gap-2">
              <span className="text-[11px] text-text-3">Skipped dispatches</span>
              <span className="text-[12px] text-text-2 tabular-nums">{summary.skipped_dispatches}</span>
            </div>
          )}
          {summary.delayed_decisions > 0 && (
            <div className="flex items-center gap-2">
              <span className="text-[11px] text-text-3">Delayed decisions</span>
              <span className="text-[12px] text-warn tabular-nums">{summary.delayed_decisions}</span>
            </div>
          )}
        </div>
      )}

      <div className="px-5 pb-5 pt-3">
        <RunSummaryPanel error={summary.error} />
      </div>
    </div>
  );
}

function DecisionTape({ decisions }: { decisions: TimelineDecision[] }) {
  if (decisions.length === 0) return null;
  return (
    <div
      data-testid="eval-decision-tape"
      className="mx-5 mt-4 rounded-sm border border-border-soft bg-surface-elev px-3 py-2"
    >
      <div className="mb-2 flex items-center justify-between gap-3">
        <div className="text-[10px] font-mono tracking-[0.18em] text-text-3 uppercase">
          Decision tape
        </div>
        <div className="text-[11px] text-text-3">
          first engaged decisions
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {decisions.map((decision) => (
          <div
            key={`${decision.i}:${decision.action}:${decision.asset}`}
            className="inline-flex items-center gap-2 rounded-sm border border-border-soft bg-surface-card px-2 py-1"
          >
            {decision.action ? <ActionPill action={decision.action} /> : null}
            <span className="font-mono text-[11px] text-text-2">
              {shortAsset(decision.asset)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * Renders a live stat value with a subtle scale pop whenever it changes.
 * The span is keyed on `value`, so React remounts it on each new value and the
 * CSS-mount-only `xvn-num-pop` animation fires — but stays put across polls that
 * return the same value. The global reduced-motion block collapses the pop.
 */
function NumPop({ value, className }: { value: string; className?: string }) {
  return (
    <span
      key={value}
      className={["xvn-num-pop", className].filter(Boolean).join(" ")}
    >
      {value}
    </span>
  );
}

function Stat({
  label,
  value,
  sub,
  tone,
  titleValue,
}: {
  label: string;
  value: string;
  sub?: string;
  tone: "pos" | "neg" | "neu" | "gold";
  titleValue?: string;
}) {
  const color =
    tone === "neg"
      ? "var(--danger)"
      : tone === "gold" || tone === "pos"
        ? "var(--gold)"
        : "var(--text)";
  return (
    <div className="px-5 py-4" style={{ borderRight: "1px solid var(--border-soft)" }}>
      <div className="text-[10px] font-mono tracking-[0.18em] text-text-3 uppercase">{label}</div>
      <div
        className="mt-1 text-[24px] font-mono tabular-nums leading-tight"
        style={{ color, fontWeight: 500 }}
        title={titleValue}
      >
        <NumPop value={value} />
      </div>
      {sub && <div className="text-[10px] font-mono text-text-3 mt-0.5">{sub}</div>}
    </div>
  );
}

function DecisionsCard({
  rows,
  filterSummaries,
  status,
  errorMsg,
}: {
  rows: DecisionRowDto[];
  filterSummaries: FilterSummary[];
  status: string;
  errorMsg: string | null | undefined;
}) {
  const [focusedIdx, setFocusedIdx] = useState<number | null>(null);
  const decisions = useMemo(() => toTimelineDecisions(rows), [rows]);

  if (rows.length === 0) {
    if (!isTerminalStatus(status)) {
      return (
        <div className="bg-surface-card border border-border rounded-card px-6 py-12 text-center text-text-2">
          <div className="font-sans text-[22px] text-text-3 mb-2" style={{ fontWeight: 600 }}>
            No decisions
          </div>
          <p className="m-0 text-[13px]">
            This run hasn't recorded any decisions yet — likely still queued or
            running.
          </p>
        </div>
      );
    }
    // Terminal status with no decisions — show error message if present
    return (
      <div className="bg-surface-card border border-border rounded-card px-6 py-12 text-center text-text-2">
        {status === "failed" && errorMsg ? (
          <>
            <div
              className="font-sans text-[22px] mb-2 text-danger"
              style={{ fontWeight: 600 }}
            >
              Run failed
            </div>
            <p className="m-0 mb-2 text-[13px]">
              This run did not record any decisions.
            </p>
            <pre
              role="alert"
              className="mt-3 mx-auto max-w-lg rounded border border-danger/30 bg-danger/[0.06] px-4 py-3 text-left font-mono text-[11px] text-danger overflow-x-auto"
            >
              {errorMsg}
            </pre>
          </>
        ) : (
          <>
            <div className="font-sans text-[22px] text-text-3 mb-2" style={{ fontWeight: 600 }}>
              No decisions
            </div>
            <p className="m-0 text-[13px]">
              This run finished with no recorded decisions.
            </p>
          </>
        )}
      </div>
    );
  }

  // Local focus is the page-level decision-jump handler. The trace-dock
  // decision filter is not wired yet (see TODO in agent-run observability), so
  // jumping highlights the row + density tick in place rather than cross-
  // filtering the dock. Clicking the focused row again clears the focus.
  const onJump = (i: number) => setFocusedIdx((cur) => (cur === i ? null : i));

  return (
    <DecisionsTable
      decisions={decisions}
      focusedIdx={focusedIdx}
      onJump={onJump}
      filterSummaries={filterSummaries}
    />
  );
}

function MetaCard({
  summary,
  totalCostUsd,
}: {
  summary: RunSummary;
  totalCostUsd: number | null;
}) {
  const displayedCostUsd = displayCost(summary, totalCostUsd);
  // The design mock lists seed/region/commit, but those fields don't exist on
  // the real run wire shape — synthesizing them would be misleading. We keep
  // the design's "right-rail config key/value list that does NOT duplicate the
  // id/strategy/scenario/agent already shown in the H1 + MetaChips" intent and
  // populate it with the run-config fields the engine actually reports.
  const rows: [string, string][] = [
    ["mode", summary.mode],
    ["status", summary.status],
    ["token cost", formatSpendUsd(displayedCostUsd)],
    ["tokens", fmtTokens(summary)],
    ["started", fmtTime(summary.started_at)],
    ["completed", summary.completed_at ? fmtTime(summary.completed_at) : "—"],
    ["duration", durationLabel(summary)],
  ];
  return (
    <Card>
      <div
        className="flex items-baseline gap-3 px-5 pt-4 pb-3"
        style={{ borderBottom: "1px solid var(--border-soft)" }}
      >
        <h2 className="m-0 font-sans text-[22px] tracking-tight text-text" style={{ fontWeight: 600 }}>
          Meta
        </h2>
        <span className="text-[11px] font-mono text-text-3">run config</span>
      </div>
      {/*
        QA30: META was a tall vertical stack rendered in a 4/12 right
        sidebar. Inlined above the center column as a horizontal row of
        chips so it doesn't shrink the chart or eat vertical space.
      */}
      <div className="p-4 text-[11px] font-mono flex flex-wrap gap-x-6 gap-y-2">
        {rows.map(([k, v]) => (
          <div key={k} className="flex items-baseline gap-2">
            <span className="text-[10px] uppercase tracking-[0.14em] text-text-3">
              {k}
            </span>
            <span className="text-text tabular-nums break-all">{v}</span>
          </div>
        ))}
      </div>

      {summary.mode === 'live' && summary.skipped_dispatches > 0 && (
        <p className="text-[11px] text-text-3 mt-1 px-4 pb-3">
          {summary.skipped_dispatches} bar{summary.skipped_dispatches !== 1 ? 's' : ''} skipped while agent was processing prior decision.
        </p>
      )}
    </Card>
  );
}

// ────────────────────────────────────────────────────────────────────────────

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
        <div className="font-sans text-[24px] text-text-3 mb-3" style={{ fontWeight: 600 }}>
          Run not found
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
      <div className="font-sans text-[24px] text-danger mb-3" style={{ fontWeight: 600 }}>
        Couldn't load run
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
    (summary.actual_input_tokens ?? 0) + (summary.actual_output_tokens ?? 0);
  return total > 0 ? `${total.toLocaleString()} tok` : "—";
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

function durationLabel(summary: RunSummary): string {
  if (!summary.completed_at) return "in progress";
  const ms =
    new Date(summary.completed_at).getTime() -
    new Date(summary.started_at).getTime();
  if (Number.isNaN(ms) || ms < 0) return "—";
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60_000);
  const s = Math.floor((ms % 60_000) / 1000);
  return `${m}m ${s}s`;
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

function realizedPnlUsd(decisions: ReadonlyArray<DecisionRowDto>): number | null {
  let total = 0;
  let count = 0;
  for (const row of decisions) {
    if (row.pnl_realized != null) {
      total += row.pnl_realized;
      count += 1;
    }
  }
  return count > 0 ? total : null;
}

function unrealizedPnlUsd(
  totalPnl: number | null,
  realizedPnl: number | null,
): number | null {
  if (totalPnl == null || realizedPnl == null) return null;
  return totalPnl - realizedPnl;
}

function pnlSplitSub(
  realizedPnl: number | null,
  unrealizedPnl: number | null,
): string {
  return `${pnlComponent("Realized", realizedPnl)} · ${pnlComponent("Unrealized", unrealizedPnl)}`;
}

function pnlComponent(label: string, pnl: number | null): string {
  return `${label} ${pnl == null ? "unavailable" : fmtPnlUsd(pnl)}`;
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

function pnlTone(pnl: number | null): "pos" | "neg" | "neu" {
  if (pnl == null) return "neu";
  if (pnl > 0) return "pos";
  if (pnl < 0) return "neg";
  return "neu";
}

// ────────────────────────────────────────────────────────────────────────────
// Per-asset rollup panel — D4 (multi-asset feature)
// Groups decisions by `asset`, sorted alphabetically for stable display.
// Shows: asset symbol, total decisions, trades opened (long_open/short_open),
// and sum of realized PnL for each asset.
// Rendered inline above the decisions table — no popup/overlay.

type AssetRollup = {
  asset: string;
  decisions: number;
  tradesOpened: number;
  pnlRealized: number | null;
};

// `pnl_realized` is set on every filled decision but its meaning differs by
// action type:
//   • long_open / short_open from a flat position: realized = 0, so
//     pnl_realized = 0 - fee = -fee (just the entry cost, not a return).
//   • flat (close): pnl_realized = position_gain - fee (the actual trade return).
//   • long_open / short_open that REVERSES an existing position: pnl_realized
//     includes the close-leg gain too, but we cannot distinguish this from a
//     pure open using the action field alone.
//
// To avoid showing only entry fees as "Realized PnL", we only accumulate
// pnl_realized for "flat" decisions (clean closes). Reversal opens are
// counted separately in tradesOpened but their realized component is omitted
// here — a known conservative undercount that is still far more accurate
// than the previous sum-all-actions approach which showed only negative fee
// noise (e.g. -$2.50 per trade when the portfolio had +$315 total equity).
//
// TOTAL PNL (the equity-curve delta) includes mark-to-market unrealized gains
// from positions still open at backtest end, which is why it can be much
// larger than the sum of closed-trade returns here.
function buildAssetRollups(decisions: DecisionRowDto[]): AssetRollup[] {
  const map = new Map<string, AssetRollup>();
  for (const row of decisions) {
    const key = row.asset;
    if (!map.has(key)) {
      map.set(key, { asset: key, decisions: 0, tradesOpened: 0, pnlRealized: null });
    }
    const entry = map.get(key)!;
    entry.decisions += 1;
    if (row.action === "long_open" || row.action === "short_open") {
      entry.tradesOpened += 1;
    }
    // Only accumulate realized PnL on flat (close) decisions. Open decisions
    // carry pnl_realized = -fee (entry cost only, no gain/loss yet), which
    // would make this column show nothing but small negative fee noise.
    if (row.action === "flat" && row.pnl_realized != null) {
      entry.pnlRealized = (entry.pnlRealized ?? 0) + row.pnl_realized;
    }
  }
  // Sort alphabetically for a stable, predictable order.
  return [...map.values()].sort((a, b) => a.asset.localeCompare(b.asset));
}

function AssetRollupPanel({ decisions }: { decisions: DecisionRowDto[] }) {
  // Only render when there's data — empty runs show nothing here.
  const rollups = useMemo(() => buildAssetRollups(decisions), [decisions]);
  if (decisions.length === 0) return null;

  // Single-asset runs still get one row — that's intentional and fine.
  return (
    <Card className="mb-3 !border-border-soft overflow-x-auto">
      <div className="px-4 py-2.5 border-b border-border-soft">
        <span className="text-[11px] font-mono tracking-[0.15em] text-text-3 uppercase">
          By asset
        </span>
      </div>
      <table className="w-full min-w-[480px]">
        <thead>
          <tr className="text-left text-text-3 text-[11px] border-b border-border-soft">
            <th className="font-normal py-2 px-4">Asset</th>
            <th className="font-normal py-2 px-3 text-right">Decisions</th>
            <th className="font-normal py-2 px-3 text-right">Trades opened</th>
            <th className="font-normal py-2 px-3 text-right">Closed trade PnL</th>
          </tr>
        </thead>
        <tbody>
          {rollups.map((r) => (
            <tr
              key={r.asset}
              className="border-b border-border-soft last:border-b-0 hover:bg-surface-hover transition-colors"
            >
              <td className="py-2 px-4 font-mono text-text text-[13px]">{r.asset}</td>
              <td className="py-2 px-3 text-right font-mono text-text-2 text-[12px]">
                {r.decisions}
              </td>
              <td className="py-2 px-3 text-right font-mono text-text-2 text-[12px]">
                {r.tradesOpened}
              </td>
              <td className={`py-2 px-3 text-right font-mono text-[12px] ${pnlClass(r.pnlRealized)}`}>
                {r.pnlRealized != null ? fmtPnlUsd(r.pnlRealized) : "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  );
}

function pnlClass(n: number | null | undefined): string {
  if (n == null) return "text-text-3";
  if (n > 0) return "text-gold";
  if (n < 0) return "text-danger";
  return "text-text-2";
}

// ────────────────────────────────────────────────────────────────────────────
// UI2 — Filter activity. Groups the two filter v1 panels under one inline
// section so the operator reads gate checks / fires / blocked-by-reason in one
// place instead of two disconnected cards. The child panels are unchanged and
// still read the same data keys (`bars_scanned`, `wakeups`,
// `suppressed_in_position`, `filter_events`, …); this is grouping + heading +
// copy only. Single full-width inline section, no sidebar, no popups.

function FilterActivitySection({
  summaries,
  events,
}: {
  summaries: FilterSummary[];
  events: FilterEventV1[];
}) {
  // Both child panels self-hide on empty input. When neither has data (EveryBar
  // runs, or runs that errored before the filter loop) render nothing so the
  // section doesn't leave a bare heading.
  if (summaries.length === 0 && events.length === 0) return null;

  return (
    <section data-testid="filter-activity" className="space-y-3">
      <div className="flex items-baseline gap-2">
        <h2
          className="m-0 font-sans text-[22px] tracking-tight text-text"
          style={{ fontWeight: 600 }}
        >
          Filter activity
        </h2>
        <span className="text-[11px] font-mono text-text-3">
          gate checks · fires · blocked
        </span>
      </div>

      {/* Gate checks + fires + blocked-by-reason (bars scanned, wake-ups,
          suppression breakdown). */}
      <FilterSummaryPanel summaries={summaries} />

      {/* Per-bar fire / blocked timeline — each tick is one gate check, colored
          by fired vs blocked-by-reason. */}
      <FilterEventTimeline events={events} title="Per-bar gate checks" />
    </section>
  );
}

function traceRunId(summary: RunSummary): string {
  const withTraceId = summary as RunSummary & { agent_run_id?: string | null };
  return withTraceId.agent_run_id ?? summary.id;
}

// Defensive viewport check: when matchMedia is absent (jsdom, SSR), default to
// desktop so existing tests keep targeting the desktop layout.
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
