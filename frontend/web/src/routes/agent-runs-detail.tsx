// frontend/web/src/routes/agent-runs-detail.tsx
import { useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { agentRunKeys, getAgentRun } from "@/api/agent-runs";
import { formatCostUsd, formatCostUsdPrecise } from "@/lib/format";
import { AgentRunIndentedTimeline } from "@/features/agent-runs/AgentRunIndentedTimeline";
import { SpanInspector } from "@/features/agent-runs/SpanInspector";
import { FilterBar } from "@/features/agent-runs/FilterBar";
import { useSpanFilter } from "@/features/agent-runs/use-span-filter";
import { deriveDecisions } from "@/features/agent-runs/decisions";
import { useTraceDock } from "@/stores/trace-dock";
import { TrajectoryModeBadge } from "@/features/agent-runs/TrajectoryModeBadge";

export function AgentRunDetailRoute() {
  const { runId = "" } = useParams<{ runId: string }>();
  const q = useQuery({
    queryKey: agentRunKeys.run(runId),
    queryFn: () => getAgentRun(runId),
    enabled: runId.length > 0,
  });
  const [selectedSpanId, setSelectedSpanId] = useState<string | null>(null);

  const filter = useSpanFilter({
    runId,
    spans: q.data?.spans ?? [],
  });

  // F-7 — trace-dock density toggle. Shared store with the in-app
  // dock so flipping in either surface persists across both.
  const advancedView = useTraceDock((s) => s.advanced_view);
  const setAdvancedView = useTraceDock((s) => s.setAdvancedView);

  // Hidden-in-Simple kinds. See TraceDock.tsx for the canonical list
  // and the reason for keeping recovery.attempt visible in both modes.
  const SIMPLE_HIDDEN_KINDS = useMemo(
    () =>
      new Set<string>([
        "tool.validate_input",
        "tool.validate_output",
        "state.transition",
        "context.assemble",
        "prompt.render",
      ]),
    [],
  );

  const displaySpans = useMemo(
    () =>
      advancedView
        ? filter.filtered
        : filter.filtered.filter((s) => !SIMPLE_HIDDEN_KINDS.has(s.kind)),
    [advancedView, filter.filtered, SIMPLE_HIDDEN_KINDS],
  );

  const simpleHiddenCount = useMemo(
    () => filter.filtered.filter((s) => SIMPLE_HIDDEN_KINDS.has(s.kind)).length,
    [filter.filtered, SIMPLE_HIDDEN_KINDS],
  );

  const selectedSpan = useMemo(
    () => filter.filtered.find((s) => s.span_id === selectedSpanId) ?? displaySpans[0] ?? null,
    [filter.filtered, displaySpans, selectedSpanId],
  );

  const selectedSpanHiddenInSimple =
    selectedSpan != null &&
    !advancedView &&
    SIMPLE_HIDDEN_KINDS.has(selectedSpan.kind);

  const decisions = useMemo(() => deriveDecisions(q.data?.spans ?? []), [q.data]);

  useEffect(() => {
    if (q.data) {
      useTraceDock.getState().setActiveRun(
        "eval",
        q.data.summary.run_id,
        q.data.summary.status === "running" ? "live" : "post-hoc",
      );
    }
  }, [q.data?.summary.run_id, q.data?.summary.status]);

  // Unconditional unmount cleanup (WS-2): the standalone agent-run route
  // is an eval-scope surface, so it nulls eval on the way out and keeps
  // the capsule from bleeding onto the next page. The live scope is left
  // untouched.
  useEffect(() => {
    return () => {
      useTraceDock.getState().setActiveRun("eval", null, "post-hoc");
    };
  }, []);

  if (q.isPending) {
    return (
      <>
        <Topbar
          title="Agent run"
          sub={runId || "Loading…"}
          back={{ to: "/eval-runs", label: "Back to runs" }}
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
        ? `agent run ${runId} not found`
        : String(q.error);
    return (
      <>
        <Topbar
          title="Agent run"
          sub={runId}
          back={{ to: "/eval-runs", label: "Back to runs" }}
        />
        <Card className="p-6 text-text-2">{message}</Card>
      </>
    );
  }

  const detail = q.data;
  const isLive = detail.summary.status === "running";

  return (
    <>
      <Topbar
        title="Agent run"
        back={{ to: "/eval-runs", label: "Back to runs" }}
        sub={
          <>
            <span
              className="font-mono text-[12px] text-text-3 break-all select-all"
              aria-label={`Agent run id ${detail.summary.run_id}`}
            >
              {detail.summary.run_id}
            </span>
            {detail.summary.objective ? (
              <>
                <span className="mx-1.5 text-text-3">·</span>
                <span>{detail.summary.objective}</span>
              </>
            ) : null}
          </>
        }
      />
      <Link
        to="/agent-runs"
        className="inline-flex items-center gap-1.5 text-[12px] text-text-2 hover:text-text mb-3"
      >
        ← Back to agent runs
      </Link>
      <Card className="p-5 mb-4 flex flex-wrap items-center gap-4">
        <div
          className="font-mono text-[12px] text-text-3 break-all select-all"
          aria-label={`Agent run id ${detail.summary.run_id}`}
        >
          {detail.summary.run_id}
        </div>
        <Pill tone={detail.summary.error_count > 0 ? "danger" : "default"}>{detail.summary.status}</Pill>
        <TrajectoryModeBadge summary={detail.summary} />
        <span className="font-mono text-[12px] text-text-2">spans: {detail.summary.span_count}</span>
        <span
          className="font-mono text-[12px] text-text-2"
          title={formatCostUsdPrecise(detail.summary.total_cost_usd)}
        >
          cost: {formatCostUsd(detail.summary.total_cost_usd)}
        </span>
        <span className="font-mono text-[12px] text-text-2">
          {detail.summary.total_input_tokens.toLocaleString()} in · {detail.summary.total_output_tokens.toLocaleString()} out
        </span>
      </Card>

      <Card className="mb-3 overflow-x-auto overflow-y-hidden">
        <div className="flex items-center gap-3">
          <FilterBar
            query={filter.query} setQuery={filter.setQuery}
            kinds={filter.kinds} toggleKind={filter.toggleKind}
            status={filter.status} setStatus={filter.setStatus}
            decisionFilter={filter.decisionFilter} setDecisionFilter={filter.setDecisionFilter}
            decisions={decisions}
            total={filter.summary.total} filtered={filter.summary.filtered}
          />
          <div
            role="group"
            aria-label="Trace density"
            data-testid="agent-run-density-toggle"
            className="ml-auto flex items-center gap-0.5"
          >
            <button
              type="button"
              aria-pressed={!advancedView}
              onClick={() => setAdvancedView(false)}
              title={
                simpleHiddenCount > 0
                  ? `Simple — hide ${simpleHiddenCount} instrumentation span${simpleHiddenCount === 1 ? "" : "s"}, collapse attribute bag`
                  : "Simple — hide instrumentation spans, collapse attribute bag"
              }
              className="h-6 px-1.5 text-[10px] font-mono tracking-[0.14em] flex items-center gap-1"
              style={{
                background: !advancedView ? "var(--surface-card)" : "transparent",
                border: `1px solid ${!advancedView ? "var(--text-2)" : "var(--border)"}`,
                color: !advancedView ? "var(--text)" : "var(--text-3)",
                borderRadius: 4,
              }}
            >
              SIMPLE
              {simpleHiddenCount > 0 ? (
                <span className="text-text-3" style={{ fontVariantNumeric: "tabular-nums" }}>
                  −{simpleHiddenCount}
                </span>
              ) : null}
            </button>
            <button
              type="button"
              aria-pressed={advancedView}
              onClick={() => setAdvancedView(true)}
              title="Advanced — show every span and the full attribute grid"
              className="h-6 px-1.5 text-[10px] font-mono tracking-[0.14em] flex items-center"
              style={{
                background: advancedView ? "var(--surface-card)" : "transparent",
                border: `1px solid ${advancedView ? "var(--text-2)" : "var(--border)"}`,
                color: advancedView ? "var(--text)" : "var(--text-3)",
                borderRadius: 4,
              }}
            >
              ADVANCED
            </button>
          </div>
        </div>
      </Card>

      <div className="grid grid-cols-1 gap-3 xl:grid-cols-[minmax(0,1fr)_400px] xl:h-[70vh]">
        <Card className="overflow-hidden min-h-[320px] xl:min-h-0 xl:max-h-none">
          <AgentRunIndentedTimeline
            spans={displaySpans}
            selectedSpanId={selectedSpan?.span_id ?? null}
            onSelect={setSelectedSpanId}
          />
        </Card>
        {selectedSpan ? (
          <Card className="overflow-hidden min-h-[420px] xl:min-h-0">
            <SpanInspector
              span={selectedSpan}
              isLive={isLive}
              simpleMode={!advancedView}
              hiddenInSimpleMode={selectedSpanHiddenInSimple}
              onRequestAdvanced={() => setAdvancedView(true)}
              runSummary={detail.summary}
              onRerun={(spanId) => {
                // Phase 4 stub — checkpoint design pending.
                console.warn("[agent-runs] rerun-from-here — pending checkpoint design", { spanId });
              }}
              onJumpToDecision={() => { /* Phase 2.5.4: cross-link to eval-runs-detail */ }}
            />
          </Card>
        ) : null}
      </div>
    </>
  );
}
