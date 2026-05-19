// frontend/web/src/features/agent-runs/StripDockSlot.tsx
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { ApiError } from "@/api/client";
import { agentRunKeys, getAgentRun } from "@/api/agent-runs";
import { agentKeys, listAgents } from "@/api/agents";
import { scenarioKeys, listScenarios } from "@/api/scenarios";
import { evalKeys, getRun as getEvalRun, listRuns } from "@/api/eval";
import { useTraceDock } from "@/stores/trace-dock";
import { formatCostUsd } from "@/lib/format";
import { shortTag } from "@/lib/short-tag";
import { spanColor } from "./span-colors";
import {
  EvalCapsule,
  type EvalCapsuleCurrentSpan,
  type EvalCapsuleFocused,
  type EvalCapsuleRow,
  type EvalCapsuleStatus,
} from "./EvalCapsule";
import { TraceDock } from "./TraceDock";

function deriveFocusedTone(
  summary: { status: string; error_count: number },
  mode: "live" | "post-hoc",
): EvalCapsuleStatus {
  if (summary.status === "failed" || summary.error_count > 0) return "error";
  // Only show the pulsing LIVE tone when the active inspector still considers
  // the run in-flight. A backend-lag scenario (eval cancelled, agent-run
  // summary still reports `running`) must not keep the pulse on — fall
  // through to a frozen terminal tone instead.
  if (summary.status === "running" && mode === "live") return "eval";
  if (summary.status === "cancelled" || summary.status === "running") return "warn";
  return "pass";
}

function deriveSiblingTone(status: string): EvalCapsuleStatus {
  switch (status) {
    case "running":
      return "eval";
    case "queued":
      return "queued";
    case "completed":
      return "pass";
    case "failed":
    case "agent_failure":
      return "error";
    case "cancelled":
    case "interrupted":
      return "warn";
    default:
      return "eval";
  }
}

function fmtPostHoc(ms: number | null): string {
  if (ms == null) return "—";
  return `${(ms / 1000).toFixed(1)}s`;
}

function fmtElapsedSec(totalSec: number): string {
  if (!Number.isFinite(totalSec) || totalSec < 0) return "—";
  const mins = Math.floor(totalSec / 60);
  const secs = totalSec % 60;
  return `${mins}:${String(secs).padStart(2, "0")}`;
}

/**
 * Compute the focused-row "current span" chip from the trace-dock streaming
 * slice while live. Mirrors the legacy `useLiveActiveSpanChip` hook so the
 * capsule keeps the same active-span behavior as the old strip.
 */
function useLiveActiveSpanChip(isLive: boolean): EvalCapsuleCurrentSpan | null {
  const activeMeta = useTraceDock((s) => s.streamingState.activeSpanMeta);
  const [nowMs, setNowMs] = useState<number>(() => Date.now());
  const hasActive = isLive && Object.keys(activeMeta).length > 0;
  useEffect(() => {
    if (!hasActive) return;
    setNowMs(Date.now());
    const id = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [hasActive]);

  return useMemo<EvalCapsuleCurrentSpan | null>(() => {
    if (!isLive) return null;
    const ids = Object.keys(activeMeta);
    if (ids.length === 0) return null;
    let best: { id: string; startedMs: number } | null = null;
    for (const id of ids) {
      const meta = activeMeta[id]!;
      const startedMs = new Date(meta.started_at).getTime();
      if (!Number.isFinite(startedMs)) continue;
      if (best == null || startedMs > best.startedMs) {
        best = { id, startedMs };
      }
    }
    if (best == null) return null;
    const meta = activeMeta[best.id]!;
    const color = spanColor(meta.kind);
    return {
      color: color.hex,
      label: color.label,
      name: meta.name,
      elapsed: `${Math.max(0, nowMs - best.startedMs)}ms`,
    };
  }, [activeMeta, isLive, nowMs]);
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

  // The capsule is "live" only when BOTH the agent-run summary says running
  // AND the active inspector has declared the run live. Without the `mode`
  // intersection, a freshly-cancelled eval whose agent-run summary hasn't
  // propagated `cancelled` yet would keep ticking — the eval inspector is
  // authoritative about whether the run is still in-flight (it flips mode
  // to "post-hoc" the moment status leaves the inflight set).
  const isLive = q.data?.summary.status === "running" && mode === "live";

  // Concurrent siblings — other in-flight eval-runs on the cluster. Only
  // polled while the capsule is mounted (i.e. there's a focused run). We
  // include the focused eval-run-id in the dedupe key so the focused row
  // never appears as its own sibling.
  const focusedEvalId = q.data?.summary.financial_eval_id ?? null;
  const siblingsQ = useQuery({
    queryKey: [...evalKeys.runs({ status: "running" }), "capsule-siblings"] as const,
    queryFn: () => listRuns({ status: "running" }),
    enabled: !!activeRunId,
    refetchInterval: 4000,
    staleTime: 2000,
  });

  // Agent + scenario name lookups. Cached aggressively because names rarely
  // change and we just need them to render the `strategy·scenario` short
  // tag. Falls back to id-slice when a row's name hasn't loaded yet.
  const agentsQ = useQuery({
    queryKey: agentKeys.list(undefined),
    queryFn: () => listAgents(),
    enabled: !!activeRunId,
    staleTime: 60_000,
  });
  const scenariosQ = useQuery({
    queryKey: scenarioKeys.list(undefined),
    queryFn: () => listScenarios(),
    enabled: !!activeRunId,
    staleTime: 60_000,
  });

  // The focused row needs a `scenario_id` for the short tag, but the
  // agent-run summary only carries `financial_eval_id`. Resolve the eval-run
  // (cached on the same key the eval-runs route uses) to recover it. Skipped
  // when there's no linked eval — fall back to the agent_id-only short tag.
  const focusedEvalQ = useQuery({
    queryKey: focusedEvalId ? evalKeys.run(focusedEvalId) : ["eval", "noop"],
    queryFn: () => getEvalRun(focusedEvalId!),
    enabled: !!focusedEvalId,
    staleTime: 30_000,
  });

  useEffect(() => {
    if (!activeRunId) return;
    if (q.error instanceof ApiError && q.error.code === "not_found") {
      useTraceDock.getState().setActiveRun(null, "post-hoc");
    }
  }, [activeRunId, q.error]);

  // Tick once per second so the m:ss duration refreshes while live.
  useEffect(() => {
    if (!isLive) return;
    const id = window.setInterval(() => useTraceDock.setState((s) => ({ ...s })), 1000);
    return () => window.clearInterval(id);
  }, [isLive]);

  const liveChip = useLiveActiveSpanChip(!!isLive);

  if (!activeRunId || !q.data) return null;

  if (height !== "collapsed") {
    return <TraceDock />;
  }

  const summary = q.data.summary;
  const startedMs = new Date(summary.started_at).getTime();
  const liveDurationSec = Math.max(0, Math.floor((Date.now() - startedMs) / 1000));

  // Post-hoc duration freeze: prefer the backend's `duration_ms`; if it hasn't
  // been written yet (e.g. cancel landed before the agent-run summary was
  // flushed) fall back to `finished_at - started_at`. Keeps the cancelled
  // capsule from showing "—" while still freezing at the cancel moment.
  const frozenDurationMs =
    summary.duration_ms == null && summary.finished_at != null
      ? Math.max(0, new Date(summary.finished_at).getTime() - startedMs)
      : summary.duration_ms;

  // Build lookup maps for agent + scenario names. Cached queries; null
  // values are valid (just fall through to id-slice).
  const agentNameById = new Map<string, string>(
    (agentsQ.data ?? []).map((a) => [a.agent_id, a.name]),
  );
  const scenarioNameById = new Map<string, string>(
    (scenariosQ.data ?? []).map((s) => [s.id, s.display_name]),
  );

  const focusedAgentId = summary.agent_id ?? summary.strategy_id ?? "agent";
  const focusedScenarioId = focusedEvalQ.data?.summary.scenario_id ?? "scenario";
  const focusedTone = deriveFocusedTone(summary, mode);
  const focused: EvalCapsuleFocused = {
    id: summary.run_id,
    short: shortTag(
      agentNameById.get(focusedAgentId) ?? null,
      scenarioNameById.get(focusedScenarioId) ?? null,
      focusedAgentId,
      focusedScenarioId,
    ),
    status: focusedTone,
    spans: summary.span_count,
    elapsed: isLive ? fmtElapsedSec(liveDurationSec) : fmtPostHoc(frozenDurationMs),
    cost: formatCostUsd(summary.total_cost_usd),
    currentSpan: liveChip,
  };

  const rawSiblings = siblingsQ.data ?? [];
  const siblings: EvalCapsuleRow[] = rawSiblings
    .filter((r) =>
      // Exclude the focused eval-run from its own sibling list.
      focusedEvalId == null ? r.id !== summary.run_id : r.id !== focusedEvalId,
    )
    .map((r) => {
      const sibStartedMs = new Date(r.started_at).getTime();
      const sibElapsedSec = Number.isFinite(sibStartedMs)
        ? Math.max(0, Math.floor((Date.now() - sibStartedMs) / 1000))
        : 0;
      return {
        id: r.id,
        short: shortTag(
          agentNameById.get(r.agent_id) ?? null,
          scenarioNameById.get(r.scenario_id) ?? null,
          r.agent_id,
          r.scenario_id,
        ),
        status: deriveSiblingTone(r.status),
        spans: "—",
        elapsed: fmtElapsedSec(sibElapsedSec),
        cost: "—",
      };
    });

  return (
    <EvalCapsule
      focused={focused}
      siblings={siblings}
      onSwitchFocus={(run) => navigate(`/eval-runs/${encodeURIComponent(run.id)}`)}
      onExpandDock={() => setHeight("working")}
      onPopOut={() => navigate(`/agent-runs/${activeRunId}`)}
    />
  );
}
