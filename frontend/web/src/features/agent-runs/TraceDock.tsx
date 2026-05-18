// frontend/web/src/features/agent-runs/TraceDock.tsx
import { useEffect, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ApiError } from "@/api/client";
import { agentRunKeys, getAgentRun, openAgentRunStream } from "@/api/agent-runs";
import type { AgentRunDetail, RunSpan } from "@/api/types-agent-runs";
import { formatCostUsd, formatCostUsdPrecise } from "@/lib/format";
import { useTraceDock } from "@/stores/trace-dock";
import { DockResizeHandle } from "./DockResizeHandle";
import { FlameGraph } from "./FlameGraph";
import { SpanInspector } from "./SpanInspector";
import { HaltStrategyButton } from "./HaltStrategyButton";
import { FilterBar } from "./FilterBar";
import { useSpanFilter } from "./use-span-filter";
import { deriveDecisions } from "./decisions";
import { TraceDownloadButton } from "./TraceDownloadButton";

/**
 * Span kinds hidden in Simple-mode trace views. Recovery spans are
 * deliberately NOT in this list — the F-7 intake calls out that
 * recovery.attempt always matters and stays visible in both modes.
 *
 * `context.assemble` / `prompt.render` don't exist as SpanKind variants
 * today; they're noted in the audit as nice-to-haves. Listing them
 * defensively means if they're added later the toggle hides them
 * without a separate change here.
 */
const SIMPLE_HIDDEN_KINDS: ReadonlySet<string> = new Set([
  "tool.validate_input",
  "tool.validate_output",
  "state.transition",
  "context.assemble",
  "prompt.render",
]);

export function TraceDock() {
  const {
    height,
    heightPx,
    activeRunId,
    selectedSpanId,
    minimize,
    setSelectedSpan,
    advanced_view,
    setAdvancedView,
  } = useTraceDock();
  const navigate = useNavigate();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F12") {
        e.preventDefault();
        useTraceDock.getState().toggle();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const q = useQuery({
    queryKey: activeRunId ? agentRunKeys.run(activeRunId) : ["agent-runs", "noop"],
    queryFn: () => getAgentRun(activeRunId!),
    enabled: !!activeRunId,
    retry: (failureCount, error) =>
      !(error instanceof ApiError && error.code === "not_found") && failureCount < 2,
  });

  useEffect(() => {
    if (!activeRunId) return;
    if (q.error instanceof ApiError && q.error.code === "not_found") {
      useTraceDock.getState().setActiveRun(null, "post-hoc");
    }
  }, [activeRunId, q.error]);

  const filter = useSpanFilter({
    runId: activeRunId ?? "",
    spans: q.data?.spans ?? [],
  });

  // Simple mode hides instrumentation kinds at the render boundary
  // (F-7). Selected span lookups still consult the unfiltered
  // `filter.filtered` so flipping the toggle does not auto-clear a
  // selection that lives in a hidden kind — the operator can switch
  // to Advanced and stay on the same span.
  const displaySpans: RunSpan[] = useMemo(
    () =>
      advanced_view
        ? filter.filtered
        : filter.filtered.filter((s) => !SIMPLE_HIDDEN_KINDS.has(s.kind)),
    [advanced_view, filter.filtered],
  );

  const selectedSpan = useMemo(
    () => filter.filtered.find((s) => s.span_id === selectedSpanId) ?? displaySpans[0] ?? null,
    [filter.filtered, displaySpans, selectedSpanId],
  );

  const selectedSpanHiddenInSimple =
    selectedSpan != null &&
    !advanced_view &&
    SIMPLE_HIDDEN_KINDS.has(selectedSpan.kind);

  // Decisions derived from spans that carry a decision_idx, deduped and sorted.
  const decisions = useMemo(() => deriveDecisions(q.data?.spans ?? []), [q.data]);

  const summary = q.data?.summary;
  const isLive = summary?.status === "running";

  const qc = useQueryClient();
  useEffect(() => {
    if (!activeRunId || !isLive) return;
    const key = agentRunKeys.run(activeRunId);
    const close = openAgentRunStream(activeRunId, (ev) => {
      switch (ev.event) {
        // Legacy mock-branch arms — kept for test/dev MODE.
        case "summary":
          qc.setQueryData<AgentRunDetail>(key, (prev) =>
            prev ? { ...prev, summary: ev.data } : prev,
          );
          return;
        case "span":
          qc.setQueryData<AgentRunDetail>(key, (prev) =>
            prev ? { ...prev, spans: [...prev.spans, ev.data] } : prev,
          );
          return;
        // Real-wire arms.
        case "snapshot":
          // Authoritative resync — replaces the cached detail wholesale.
          qc.setQueryData<AgentRunDetail>(key, ev.data);
          return;
        case "span_started": {
          const partial: RunSpan = {
            span_id: ev.data.span_id,
            parent_span_id: ev.data.parent_span_id ?? null,
            name: ev.data.name,
            kind: ev.data.kind,
            started_at: ev.data.started_at,
            finished_at: null,
            status: "in_progress",
            attributes: {},
          };
          qc.setQueryData<AgentRunDetail>(key, (prev) => {
            if (!prev) return prev;
            if (prev.spans.some((s) => s.span_id === partial.span_id)) return prev;
            return { ...prev, spans: [...prev.spans, partial] };
          });
          return;
        }
        case "span_finished": {
          qc.setQueryData<AgentRunDetail>(key, (prev) => {
            if (!prev) return prev;
            let mutated = false;
            const spans = prev.spans.map((s) => {
              if (s.span_id !== ev.data.span_id) return s;
              mutated = true;
              return {
                ...s,
                finished_at: ev.data.ended_at,
                status: ev.data.status,
              };
            });
            return mutated ? { ...prev, spans } : prev;
          });
          return;
        }
        case "run_finished":
        case "run_interrupted": {
          qc.setQueryData<AgentRunDetail>(key, (prev) => {
            if (!prev) return prev;
            const finished_at =
              "finished_at" in ev.data ? ev.data.finished_at : prev.summary.finished_at;
            const nextStatus =
              ev.event === "run_finished"
                ? ev.data.status
                : ("interrupted" as const);
            return {
              ...prev,
              summary: { ...prev.summary, status: nextStatus, finished_at },
            };
          });
          // Pull canonical aggregates (cost, span/model counts, terminal
          // statuses on spans) on the next tick so the trace dock isn't
          // left guessing at totals from event-only deltas.
          qc.invalidateQueries({ queryKey: key });
          return;
        }
        case "model_call_finished":
        case "tool_call_finished":
        case "tool_call_failed":
        case "tool_call_cancelled":
        case "broker_call_finished":
          // These carry detail (tokens, cost, output hashes,
          // broker fill/error) we don't reconstruct from the event
          // payload alone; refetch the canonical detail to keep
          // aggregates honest.
          qc.invalidateQueries({ queryKey: key });
          return;
        // Lifecycle / informational arms — no cache side effect.
        case "run_started":
        case "tool_call_started":
        case "broker_call_started":
        case "assistant_text_delta":
        case "sidecar_error":
        case "checkpoint_written":
        case "supervisor_note":
        case "artifact_written":
        case "backpressure_dropped":
        case "lagged":
          return;
      }
    });
    return close;
  }, [activeRunId, isLive, qc]);

  if (!activeRunId) return null;
  if (height === "collapsed") return null;

  return (
    <div
      data-testid="trace-dock"
      className="fixed bottom-0 left-0 right-0 z-30 bg-bg border-t border-border shadow-2xl flex flex-col"
      style={{ height: heightPx }}
    >
      <DockResizeHandle />
      <div className="flex items-center gap-3 px-3 h-8 border-b border-border text-[11px] font-mono">
        <span className="text-text-2">TRACE</span>
        {summary ? (
          <>
            <span aria-hidden className="opacity-60">▓▒░</span>
            <span>{summary.span_count} spans</span>
            <span className="opacity-40">·</span>
            <span>{summary.model_call_count} model</span>
            <span className="opacity-40">·</span>
            <span title={formatCostUsdPrecise(summary.total_cost_usd)}>
              {formatCostUsd(summary.total_cost_usd)}
            </span>
            {isLive ? <span className="text-blue-300 ml-2 animate-pulse">● LIVE</span> : null}
          </>
        ) : (
          <span className="text-text-3">loading…</span>
        )}
        <div
          role="group"
          aria-label="Trace density"
          data-testid="trace-dock-density-toggle"
          className="ml-3 flex items-center gap-0.5"
        >
          <button
            type="button"
            aria-pressed={!advanced_view}
            onClick={() => setAdvancedView(false)}
            title="Simple — hide instrumentation spans, collapse attribute bag"
            className="h-6 px-1.5 text-[10px] font-mono tracking-[0.14em] flex items-center"
            style={{
              background: !advanced_view ? "var(--surface-card)" : "transparent",
              border: `1px solid ${!advanced_view ? "var(--text-2)" : "var(--border)"}`,
              color: !advanced_view ? "var(--text)" : "var(--text-3)",
              borderRadius: 4,
            }}
          >
            SIMPLE
          </button>
          <button
            type="button"
            aria-pressed={advanced_view}
            onClick={() => setAdvancedView(true)}
            title="Advanced — show every span and the full attribute grid"
            className="h-6 px-1.5 text-[10px] font-mono tracking-[0.14em] flex items-center"
            style={{
              background: advanced_view ? "var(--surface-card)" : "transparent",
              border: `1px solid ${advanced_view ? "var(--text-2)" : "var(--border)"}`,
              color: advanced_view ? "var(--text)" : "var(--text-3)",
              borderRadius: 4,
            }}
          >
            ADVANCED
          </button>
        </div>
        <div className="ml-auto flex items-center gap-1">
          {isLive && summary?.strategy_id ? (
            <HaltStrategyButton
              strategyName={summary.strategy_id}
              onHalt={() => console.warn("[agent-runs] halt-strategy — pending checkpoint design", { strategyId: summary.strategy_id })}
            />
          ) : null}
          {/*
            Export region — disjoint from the height/pop-out/minimize cluster
            to leave room for sibling tracks (`qa-eval-trace-fidelity` and
            `qa-trace-error-surfacing`) to add adjacent controls without a
            merge conflict. Keep new export-style controls inside this group.
          */}
          <div data-testid="trace-dock-export" className="flex items-center gap-1">
            <TraceDownloadButton runId={activeRunId} />
          </div>
          <span aria-hidden className="opacity-30 px-1">|</span>
          <button
            type="button"
            aria-label="pop out to dedicated view"
            title="Open in dedicated route"
            onClick={() => navigate(`/agent-runs/${activeRunId}`)}
            className="h-7 w-8 inline-flex items-center justify-center rounded text-text-3 hover:text-text hover:bg-surface-elev"
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M6 3h7v7M13 3l-7 7M3 8v5h5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
          <button
            type="button"
            aria-label="minimize dock"
            title="Minimize trace dock (F12)"
            onClick={minimize}
            className="h-7 w-8 inline-flex items-center justify-center rounded text-text-3 hover:text-text hover:bg-surface-elev"
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M3 6l5 5 5-5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
        </div>
      </div>
      <FilterBar
        query={filter.query} setQuery={filter.setQuery}
        kinds={filter.kinds} toggleKind={filter.toggleKind}
        status={filter.status} setStatus={filter.setStatus}
        decisionFilter={filter.decisionFilter} setDecisionFilter={filter.setDecisionFilter}
        decisions={decisions}
        total={filter.summary.total} filtered={filter.summary.filtered}
      />
      <div data-testid="trace-dock-body" className="flex flex-1 min-h-0">
        <div className="min-w-0 flex-1 border-r border-border">
          {q.data ? (
            <FlameGraph
              spans={displaySpans}
              selectedSpanId={selectedSpan?.span_id ?? null}
              onSelect={setSelectedSpan}
            />
          ) : null}
        </div>
        {selectedSpan ? (
          <div className="w-[400px] min-w-0">
            <SpanInspector
              span={selectedSpan}
              isLive={isLive}
              simpleMode={!advanced_view}
              hiddenInSimpleMode={selectedSpanHiddenInSimple}
              onRequestAdvanced={() => setAdvancedView(true)}
              onRerun={(spanId) => {
                // Phase 4 stub — checkpoint design pending.
                console.warn("[agent-runs] rerun-from-here — pending checkpoint design", { spanId });
              }}
              onJumpToDecision={() => {
                // Phase 2.5.4 will wire cross-link to eval-runs-detail.
              }}
            />
          </div>
        ) : null}
      </div>
    </div>
  );
}
