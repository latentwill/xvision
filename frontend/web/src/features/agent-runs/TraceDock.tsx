// frontend/web/src/features/agent-runs/TraceDock.tsx
import { useEffect, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { agentRunKeys, getAgentRun, openAgentRunStream } from "@/api/agent-runs";
import type { AgentRunDetail } from "@/api/types-agent-runs";
import { useTraceDock, type DockHeight } from "@/stores/trace-dock";
import { FlameGraph } from "./FlameGraph";
import { SpanInspector } from "./SpanInspector";
import { HaltStrategyButton } from "./HaltStrategyButton";
import { FilterBar } from "./FilterBar";
import { useSpanFilter } from "./use-span-filter";
import { deriveDecisions } from "./decisions";
import { TraceDownloadButton } from "./TraceDownloadButton";

function heightPx(h: DockHeight): number {
  if (h === "collapsed") return 0;
  if (h === "peek") return 240;
  if (h === "working") return 480;
  // full
  if (typeof window !== "undefined") return Math.floor(window.innerHeight * 0.8);
  return 600;
}

export function TraceDock() {
  const { height, activeRunId, mode, selectedSpanId, minimize, setHeight, setSelectedSpan } =
    useTraceDock();
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
  });

  const filter = useSpanFilter({
    runId: activeRunId ?? "",
    spans: q.data?.spans ?? [],
  });

  const selectedSpan = useMemo(
    () => filter.filtered.find((s) => s.span_id === selectedSpanId) ?? filter.filtered[0] ?? null,
    [filter.filtered, selectedSpanId],
  );

  // Decisions derived from spans that carry a decision_idx, deduped and sorted.
  const decisions = useMemo(() => deriveDecisions(q.data?.spans ?? []), [q.data]);

  const qc = useQueryClient();
  useEffect(() => {
    if (!activeRunId || mode !== "live") return;
    const close = openAgentRunStream(activeRunId, (ev) => {
      if (ev.event === "summary") {
        qc.setQueryData<AgentRunDetail>(agentRunKeys.run(activeRunId), (prev) =>
          prev ? { ...prev, summary: ev.data } : prev,
        );
      }
      if (ev.event === "span") {
        qc.setQueryData<AgentRunDetail>(agentRunKeys.run(activeRunId), (prev) =>
          prev ? { ...prev, spans: [...prev.spans, ev.data] } : prev,
        );
      }
    });
    return close;
  }, [activeRunId, mode, qc]);

  if (!activeRunId) return null;
  if (height === "collapsed") return null;

  const summary = q.data?.summary;
  const isLive = mode === "live";

  return (
    <div
      data-testid="trace-dock"
      className="fixed bottom-0 left-0 right-0 z-30 bg-bg border-t border-border shadow-2xl flex flex-col"
      style={{ height: heightPx(height) }}
    >
      <div className="flex items-center gap-3 px-3 h-8 border-b border-border text-[11px] font-mono">
        <span className="text-text-2">TRACE</span>
        {summary ? (
          <>
            <span aria-hidden className="opacity-60">▓▒░</span>
            <span>{summary.span_count} spans</span>
            <span className="opacity-40">·</span>
            <span>{summary.model_call_count} model</span>
            <span className="opacity-40">·</span>
            <span>${summary.total_cost_usd.toFixed(4)}</span>
            {isLive ? <span className="text-blue-300 ml-2 animate-pulse">● LIVE</span> : null}
          </>
        ) : (
          <span className="text-text-3">loading…</span>
        )}
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
          {(["peek", "working", "full"] as const).map((h) => (
            <button
              key={h}
              type="button"
              onClick={() => setHeight(h)}
              aria-pressed={height === h}
              className={`px-1.5 py-0.5 border rounded-sm ${height === h ? "border-text" : "border-border"}`}
            >
              {h}
            </button>
          ))}
          <button
            type="button"
            aria-label="pop out to dedicated view"
            onClick={() => navigate(`/agent-runs/${activeRunId}`)}
            className="px-2 hover:opacity-80"
          >
            ⤡
          </button>
          <button
            type="button"
            aria-label="minimize dock"
            onClick={minimize}
            className="px-2 hover:opacity-80"
          >
            ⤓
          </button>
        </div>
      </div>
      {height !== "peek" ? (
        <FilterBar
          query={filter.query} setQuery={filter.setQuery}
          kinds={filter.kinds} toggleKind={filter.toggleKind}
          status={filter.status} setStatus={filter.setStatus}
          decisionFilter={filter.decisionFilter} setDecisionFilter={filter.setDecisionFilter}
          decisions={decisions}
          total={filter.summary.total} filtered={filter.summary.filtered}
        />
      ) : null}
      <div data-testid="trace-dock-body" className="flex flex-1 min-h-0">
        <div className={`min-w-0 ${height === "peek" ? "flex-1" : "flex-1 border-r border-border"}`}>
          {q.data ? (
            <FlameGraph
              spans={filter.filtered}
              selectedSpanId={selectedSpan?.span_id ?? null}
              onSelect={setSelectedSpan}
            />
          ) : null}
        </div>
        {height !== "peek" && selectedSpan ? (
          <div className="w-[400px] min-w-0">
            <SpanInspector
              span={selectedSpan}
              isLive={isLive}
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
