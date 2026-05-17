// frontend/web/src/features/agent-runs/StripDockSlot.tsx
import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { agentRunKeys, getAgentRun } from "@/api/agent-runs";
import { useTraceDock } from "@/stores/trace-dock";
import { RunStatusStrip } from "./RunStatusStrip";
// TraceDock added in Phase 3 — until then this is a placeholder.

function deriveTone(summary: { status: string; error_count: number }): "completed" | "live" | "warn" | "error" {
  if (summary.status === "failed" || summary.error_count > 0) return "error";
  if (summary.status === "running") return "live";
  if (summary.status === "cancelled") return "warn";
  return "completed";
}

export function StripDockSlot() {
  const { activeRunId, height, mode, setHeight } = useTraceDock();
  const navigate = useNavigate();

  // Tick once per second so the strip's m:ss duration refreshes while live.
  useEffect(() => {
    if (mode !== "live") return;
    const id = window.setInterval(() => useTraceDock.setState((s) => ({ ...s })), 1000);
    return () => window.clearInterval(id);
  }, [mode]);

  const q = useQuery({
    queryKey: activeRunId ? agentRunKeys.run(activeRunId) : ["agent-runs", "noop"],
    queryFn: () => getAgentRun(activeRunId!),
    enabled: !!activeRunId,
  });

  if (!activeRunId || !q.data) return null;

  if (height === "collapsed") {
    const summary = q.data.summary;
    const startedMs = new Date(summary.started_at).getTime();
    const liveDurationSec = Math.max(0, Math.floor((Date.now() - startedMs) / 1000));
    return (
      <RunStatusStrip
        summary={summary}
        currentSpan={null /* Phase 3 will compute newest inflight leaf */}
        isLive={mode === "live"}
        liveDurationSec={liveDurationSec}
        tone={deriveTone(summary)}
        onExpand={() => setHeight("working")}
        onPopOut={() => navigate(`/agent-runs/${activeRunId}`)}
      />
    );
  }

  // Phase 3 replaces this placeholder with <TraceDock />.
  return <div data-testid="trace-dock-placeholder" />;
}
