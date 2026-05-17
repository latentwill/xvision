// frontend/web/src/routes/agent-runs-detail.tsx
import { useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { agentRunKeys, getAgentRun } from "@/api/agent-runs";
import { AgentRunRailTree } from "@/features/agent-runs/AgentRunRailTree";
import { AgentRunIndentedTimeline } from "@/features/agent-runs/AgentRunIndentedTimeline";
import { SpanInspector } from "@/features/agent-runs/SpanInspector";

export function AgentRunDetailRoute() {
  const { runId = "" } = useParams<{ runId: string }>();
  const q = useQuery({
    queryKey: agentRunKeys.run(runId),
    queryFn: () => getAgentRun(runId),
    enabled: runId.length > 0,
  });
  const [selectedSpanId, setSelectedSpanId] = useState<string | null>(null);

  const selectedSpan = useMemo(
    () => q.data?.spans.find((s) => s.span_id === selectedSpanId) ?? q.data?.spans[0] ?? null,
    [q.data, selectedSpanId],
  );

  if (q.isPending) {
    return (
      <>
        <Topbar title="Agent run" sub={runId || "Loading…"} />
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
        <Topbar title="Agent run" sub={runId} />
        <Card className="p-6 text-text-2">{message}</Card>
      </>
    );
  }

  const detail = q.data;
  const isLive = detail.summary.status === "running";

  return (
    <>
      <Topbar
        title={`Run ${detail.summary.run_id}`}
        sub={detail.summary.objective}
      />
      <Card className="p-5 mb-4 flex flex-wrap items-center gap-4">
        <div className="font-mono text-[12px] text-text-3">{detail.summary.run_id}</div>
        <Pill tone={detail.summary.error_count > 0 ? "danger" : "default"}>{detail.summary.status}</Pill>
        <span className="font-mono text-[12px] text-text-2">spans: {detail.summary.span_count}</span>
        <span className="font-mono text-[12px] text-text-2">cost: ${detail.summary.total_cost_usd.toFixed(4)}</span>
        <span className="font-mono text-[12px] text-text-2">
          {detail.summary.total_input_tokens.toLocaleString()} in · {detail.summary.total_output_tokens.toLocaleString()} out
        </span>
      </Card>

      <div className="grid grid-cols-[220px_1fr_400px] gap-3 h-[70vh]">
        <Card className="overflow-hidden">
          <AgentRunRailTree
            spans={detail.spans}
            selectedSpanId={selectedSpan?.span_id ?? null}
            onSelect={setSelectedSpanId}
          />
        </Card>
        <Card className="overflow-hidden">
          <AgentRunIndentedTimeline
            spans={detail.spans}
            selectedSpanId={selectedSpan?.span_id ?? null}
            onSelect={setSelectedSpanId}
          />
        </Card>
        {selectedSpan ? (
          <SpanInspector
            span={selectedSpan}
            isLive={isLive}
            onRerun={(spanId) => {
              // Phase 4 stub — checkpoint design pending.
              window.alert(`rerun-from-here pending checkpoint design (span ${spanId})`);
            }}
            onJumpToDecision={() => { /* Phase 2.5.4: cross-link to eval-runs-detail */ }}
          />
        ) : null}
      </div>
    </>
  );
}
