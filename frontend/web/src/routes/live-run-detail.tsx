// frontend/web/src/routes/live-run-detail.tsx
//
// Live Strategy Inspector (`/live/runs/:runId`) — the live-money variant of
// the agent-run detail page. Composes the SAME pieces (indented timeline,
// span inspector, filter bar, decisions) but with a live header strip:
//
//   * LIVE badge (gold) instead of the bare status pill, + paused /
//     flatten-pending chips from the run summary.
//   * Deployment name + PnL from the linked eval run (for live runs the
//     eval scenario is synthesized from `live_config`, so its display
//     name IS the deployment's `live_config.display_name`).
//   * Back-link to /live (not the eval-runs list).
//   * No backtest affordances: SpanInspector is mounted with
//     `isLive={true}` so "Rerun from here" stays locked on a live run.
//
// NOT shown: `live_config.venue_label`, stop-policy, and capital — the
// eval run detail API (`RunDetail`/`RunSummary`) does not expose
// `live_config`, so they are unreachable from the SPA today. When the
// backend surfaces them, render them as chips in the header strip.
//
// Single full-width column (no 4th column) per the repo layout rule.

import { useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { agentRunKeys, getAgentRun } from "@/api/agent-runs";
import { evalKeys, getRun as getEvalRun } from "@/api/eval";
import { formatCostUsd, formatCostUsdPrecise } from "@/lib/format";
import { AgentRunIndentedTimeline } from "@/features/agent-runs/AgentRunIndentedTimeline";
import { SpanInspector } from "@/features/agent-runs/SpanInspector";
import { FilterBar } from "@/features/agent-runs/FilterBar";
import { useSpanFilter } from "@/features/agent-runs/use-span-filter";
import { deriveDecisions } from "@/features/agent-runs/decisions";
import { useTraceDock } from "@/stores/trace-dock";

function formatPnlPct(pct: number | null | undefined): string | null {
  if (pct == null || !Number.isFinite(pct)) return null;
  const sign = pct > 0 ? "+" : "";
  return `${sign}${pct.toFixed(2)}%`;
}

export function LiveRunDetailRoute() {
  const { runId = "" } = useParams<{ runId: string }>();
  const q = useQuery({
    queryKey: agentRunKeys.run(runId),
    queryFn: () => getAgentRun(runId),
    enabled: runId.length > 0,
    // The run is (usually) live — keep the header fresh.
    refetchInterval: 10_000,
  });
  const [selectedSpanId, setSelectedSpanId] = useState<string | null>(null);

  // Linked eval run → deployment display name + PnL. For live runs the
  // scenario record is synthesized from live_config, so `scenario
  // .display_name` is the deployment name the operator typed at launch.
  const financialEvalId = q.data?.summary.financial_eval_id ?? null;
  const evalQ = useQuery({
    queryKey: financialEvalId ? evalKeys.run(financialEvalId) : ["eval", "noop"],
    queryFn: () => getEvalRun(financialEvalId!),
    enabled: !!financialEvalId,
    refetchInterval: 10_000,
  });

  const filter = useSpanFilter({
    runId,
    spans: q.data?.spans ?? [],
  });

  const decisions = useMemo(() => deriveDecisions(q.data?.spans ?? []), [q.data]);

  const selectedSpan = useMemo(
    () =>
      filter.filtered.find((s) => s.span_id === selectedSpanId) ??
      filter.filtered[0] ??
      null,
    [filter.filtered, selectedSpanId],
  );

  useEffect(() => {
    if (q.data) {
      useTraceDock.getState().setActiveRun(
        q.data.summary.run_id,
        q.data.summary.status === "running" ? "live" : "post-hoc",
      );
    }
  }, [q.data?.summary.run_id, q.data?.summary.status]);

  if (q.isPending) {
    return (
      <>
        <Topbar
          title="Live run"
          sub={runId || "Loading…"}
          back={{ to: "/live", label: "Back to live trading" }}
        />
        <Card className="p-6 animate-pulse">
          <div className="h-5 w-72 bg-surface-elev rounded mb-3" />
        </Card>
      </>
    );
  }

  if (q.isError || !q.data) {
    const message =
      q.error instanceof ApiError && q.error.code === "not_found"
        ? `live run ${runId} not found`
        : String(q.error);
    return (
      <>
        <Topbar
          title="Live run"
          sub={runId}
          back={{ to: "/live", label: "Back to live trading" }}
        />
        <Card className="p-6 text-text-2">{message}</Card>
      </>
    );
  }

  const detail = q.data;
  const summary = detail.summary;
  const evalSummary = evalQ.data?.summary;
  const deploymentName =
    evalSummary?.scenario?.display_name ||
    summary.objective ||
    summary.run_id.slice(0, 8);
  const pnl = formatPnlPct(evalSummary?.total_return_pct);
  const isRunning = summary.status === "running";
  // Defensive: a deep link can land on a non-live agent run; say so
  // instead of dressing a backtest up as live money.
  const isLiveMoney =
    summary.is_live_money === true || summary.eval_mode === "live";

  return (
    <>
      <Topbar
        title="Live run"
        back={{ to: "/live", label: "Back to live trading" }}
        sub={
          <>
            <span
              className="font-mono text-[12px] text-text-3 break-all select-all"
              aria-label={`Live run id ${summary.run_id}`}
            >
              {summary.run_id}
            </span>
            <span className="mx-1.5 text-text-3">·</span>
            <span>{deploymentName}</span>
          </>
        }
      />

      {/* Live header strip — full-width inline row, no side boxes. */}
      <Card
        className="p-5 mb-4 flex flex-wrap items-center gap-4"
        data-testid="live-run-header"
      >
        {isLiveMoney ? (
          <Pill tone="gold" animated={isRunning} data-testid="live-badge">
            LIVE
          </Pill>
        ) : (
          <Pill tone="default" data-testid="not-live-badge">
            NOT LIVE
          </Pill>
        )}
        <Pill tone={summary.error_count > 0 ? "danger" : "default"}>
          {summary.status}
        </Pill>
        {summary.paused === true ? (
          <Pill tone="warn" data-testid="live-paused-pill">
            paused
          </Pill>
        ) : null}
        {summary.flatten_requested === true ? (
          <Pill tone="warn" data-testid="live-flatten-pill">
            flattening positions…
          </Pill>
        ) : null}
        {pnl ? (
          <span
            className="font-mono text-[12px] tabular-nums"
            style={{ color: pnl.startsWith("-") ? "var(--danger)" : "var(--gold)" }}
            data-testid="live-pnl"
          >
            pnl {pnl}
          </span>
        ) : null}
        <span className="font-mono text-[12px] text-text-2">
          spans: {summary.span_count}
        </span>
        <span
          className="font-mono text-[12px] text-text-2"
          title={formatCostUsdPrecise(summary.total_cost_usd)}
        >
          cost: {formatCostUsd(summary.total_cost_usd)}
        </span>
        <span className="font-mono text-[12px] text-text-2">
          {summary.total_input_tokens.toLocaleString()} in ·{" "}
          {summary.total_output_tokens.toLocaleString()} out
        </span>
        {financialEvalId ? (
          <Link
            to={`/eval-runs/${encodeURIComponent(financialEvalId)}`}
            className="ml-auto text-[12px] text-text-3 hover:text-text"
          >
            eval record →
          </Link>
        ) : null}
      </Card>

      <Card className="mb-3 overflow-x-auto overflow-y-hidden">
        <FilterBar
          query={filter.query} setQuery={filter.setQuery}
          kinds={filter.kinds} toggleKind={filter.toggleKind}
          status={filter.status} setStatus={filter.setStatus}
          decisionFilter={filter.decisionFilter} setDecisionFilter={filter.setDecisionFilter}
          decisions={decisions}
          total={filter.summary.total} filtered={filter.summary.filtered}
        />
      </Card>

      <div className="grid grid-cols-1 gap-3 xl:grid-cols-[minmax(0,1fr)_400px] xl:h-[70vh]">
        <Card className="overflow-hidden min-h-[320px] xl:min-h-0 xl:max-h-none">
          <AgentRunIndentedTimeline
            spans={filter.filtered}
            selectedSpanId={selectedSpan?.span_id ?? null}
            onSelect={setSelectedSpanId}
          />
        </Card>
        {selectedSpan ? (
          <Card className="overflow-hidden min-h-[420px] xl:min-h-0">
            <SpanInspector
              span={selectedSpan}
              // Always true here: keeps "Rerun from here" locked — a
              // re-run affordance has no meaning on a live-money run.
              isLive={true}
              runSummary={summary}
              onRerun={() => {
                /* locked on live runs */
              }}
              onJumpToDecision={() => {
                /* cross-link pending, parity with agent-runs-detail */
              }}
            />
          </Card>
        ) : null}
      </div>
    </>
  );
}
