// frontend/web/src/features/agent-runs/StripDockSlot.tsx
import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { ApiError } from "@/api/client";
import { agentRunKeys, getAgentRun } from "@/api/agent-runs";
import { useTraceDock } from "@/stores/trace-dock";
import { RunStatusStrip } from "./RunStatusStrip";
import { TraceDock } from "./TraceDock";

function deriveTone(
  summary: { status: string; error_count: number },
  mode: "live" | "post-hoc",
): "completed" | "live" | "warn" | "error" {
  if (summary.status === "failed" || summary.error_count > 0) return "error";
  // Only show the pulsing LIVE tone when the active inspector still
  // considers the run inflight. A backend-lag scenario (eval cancelled,
  // agent-run summary still reports `running`) must not keep the LIVE
  // dot pulsing — fall through to a frozen terminal tone instead.
  if (summary.status === "running" && mode === "live") return "live";
  if (summary.status === "cancelled" || summary.status === "running") return "warn";
  return "completed";
}

export function StripDockSlot() {
  const { activeRunId, height, setHeight, mode } = useTraceDock();
  const navigate = useNavigate();

  const q = useQuery({
    queryKey: activeRunId ? agentRunKeys.run(activeRunId) : ["agent-runs", "noop"],
    queryFn: () => getAgentRun(activeRunId!),
    enabled: !!activeRunId,
    retry: (failureCount, error) =>
      !(error instanceof ApiError && error.code === "not_found") && failureCount < 2,
  });

  // The strip is "live" only when BOTH the agent-run summary says
  // running AND the active inspector has declared the run live. Without
  // the `mode` intersection, a freshly-cancelled eval whose agent-run
  // summary hasn't propagated `cancelled` yet would keep ticking — the
  // eval inspector is authoritative about whether the run is still
  // in-flight (it flips mode to "post-hoc" the moment status leaves
  // the inflight set).
  const isLive = q.data?.summary.status === "running" && mode === "live";

  useEffect(() => {
    if (!activeRunId) return;
    if (q.error instanceof ApiError && q.error.code === "not_found") {
      useTraceDock.getState().setActiveRun(null, "post-hoc");
    }
  }, [activeRunId, q.error]);

  // Tick once per second so the strip's m:ss duration refreshes while live.
  useEffect(() => {
    if (!isLive) return;
    const id = window.setInterval(() => useTraceDock.setState((s) => ({ ...s })), 1000);
    return () => window.clearInterval(id);
  }, [isLive]);

  if (!activeRunId || !q.data) return null;

  if (height === "collapsed") {
    const summary = q.data.summary;
    const startedMs = new Date(summary.started_at).getTime();
    const liveDurationSec = Math.max(0, Math.floor((Date.now() - startedMs) / 1000));
    // Post-hoc duration freeze: prefer the backend's `duration_ms`; if
    // it hasn't been written yet (e.g. cancel landed before the
    // agent-run summary was flushed) fall back to
    // `finished_at - started_at`. Keeps the cancelled capsule from
    // showing "—" while still freezing at the cancel moment.
    const frozenSummary =
      summary.duration_ms == null && summary.finished_at != null
        ? {
            ...summary,
            duration_ms: Math.max(
              0,
              new Date(summary.finished_at).getTime() - startedMs,
            ),
          }
        : summary;
    return (
      <RunStatusStrip
        summary={frozenSummary}
        currentSpan={null /* Phase 3 will compute newest inflight leaf */}
        isLive={isLive}
        liveDurationSec={liveDurationSec}
        tone={deriveTone(summary, mode)}
        onExpand={() => setHeight("working")}
        onPopOut={() => navigate(`/agent-runs/${activeRunId}`)}
      />
    );
  }

  return <TraceDock />;
}
