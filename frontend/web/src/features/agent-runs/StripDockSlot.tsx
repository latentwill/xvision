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
import { useCurrentTraceScope } from "./use-trace-scope";
import { formatSpendUsd } from "@/lib/format";
import { shortTag } from "@/lib/short-tag";
import { spanColor } from "./span-colors";
import {
  EvalCapsule,
  type EvalCapsuleCurrentSpan,
  type EvalCapsuleFocused,
  type EvalCapsuleRow,
  type EvalCapsuleStatus,
} from "./EvalCapsule";
import { LiveCapsule } from "../live/LiveCapsule";
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
 * Format a `RunSummary.total_return_pct` value (percent, e.g. `1.42`) into
 * the capsule's compact PnL string. `null` / unavailable values render as
 * "—" so the row still has a slot for them.
 *
 * Leading sign is preserved (`+`, `-`) so the capsule can colour-tone by
 * sign — see `EvalCapsule#pnlTone`.
 */
function formatPnlPct(pct: number | null): string {
  if (pct == null || !Number.isFinite(pct)) return "—";
  const sign = pct > 0 ? "+" : "";
  return `${sign}${pct.toFixed(2)}%`;
}

/**
 * How long a freshly-failed sibling should stay in the capsule after its
 * `completed_at` timestamp. Past this window the row drops out — the
 * operator has either acknowledged it or navigated away. Keeps the
 * capsule from accumulating every historical failure on the cluster.
 */
const RECENT_FAILURE_WINDOW_MS = 120_000;

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
  // The capsule is scoped to the surface the operator is currently on:
  // eval routes read the eval slice, live routes the live slice. This
  // is what stops the capsule from following navigation onto unrelated
  // pages — when the route's scope has no active run, this returns null.
  const scope = useCurrentTraceScope();
  // WS-11a: the `opti` surface (the autooptimizer cycle trace) is owned by the
  // dedicated OptiCapsule mounted on the `/optimizer` route — it does not flow
  // through this agent-run capsule (cycle ids are not agent-run ids, and the
  // agent-run query below would 404 on them). Bail before any agent-run work.
  const activeRunId = useTraceDock((s) =>
    scope === "opti" ? null : s.byScope[scope].activeRunId,
  );
  const mode = useTraceDock((s) => s.byScope[scope].mode);
  const height = useTraceDock((s) => s.height);
  const setHeight = useTraceDock((s) => s.setHeight);
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

  // Concurrent siblings — other eval-runs of interest on the cluster.
  //
  // Scope is intentionally eval-only: we require the focused agent-run to
  // be linked to an eval-run (`financial_eval_id`) before polling or
  // rendering any siblings. Live-strategy multi-run observability is a
  // separate problem with a different grouping key and is deferred — the
  // capsule simply renders a single row when there's no eval link.
  //
  // We poll two slices of the eval-runs list and merge them:
  //   * `running`  — currently in flight, the bread-and-butter sibling set
  //   * `failed`   — pulled so freshly-errored siblings can auto-promote
  //                  and force-open the capsule. Without this slice the
  //                  errored eval would drop out of the running set on
  //                  failure and never surface in the capsule, defeating
  //                  the design's auto-open-on-error behavior.
  //
  // The failed slice is recency-filtered client-side to runs that
  // completed within the last `RECENT_FAILURE_WINDOW_MS`, so we don't
  // render every historical failure ever recorded on the cluster.
  // Queued runs are NOT polled — they aren't running concurrently with
  // the focused eval, just waiting; surfacing them adds noise without
  // changing operator action.
  const focusedEvalId = q.data?.summary.financial_eval_id ?? null;
  const runningQ = useQuery({
    queryKey: [...evalKeys.runs({ status: "running" }), "capsule-siblings-running"] as const,
    queryFn: () => listRuns({ status: "running" }),
    enabled: !!focusedEvalId,
    refetchInterval: 4000,
    staleTime: 2000,
  });
  const failedQ = useQuery({
    queryKey: [...evalKeys.runs({ status: "failed" }), "capsule-siblings-failed"] as const,
    queryFn: () => listRuns({ status: "failed" }),
    enabled: !!focusedEvalId,
    refetchInterval: 4000,
    staleTime: 2000,
  });

  // Agent + scenario name lookups. Cached aggressively because names rarely
  // change and we just need them to render the `strategy·scenario` short
  // tag. Falls back to id-slice when a row's name hasn't loaded yet.
  const agentsQ = useQuery({
    queryKey: agentKeys.list(undefined),
    queryFn: () => listAgents(),
    enabled: !!focusedEvalId,
    staleTime: 60_000,
  });
  const scenariosQ = useQuery({
    queryKey: scenarioKeys.list(undefined),
    queryFn: () => listScenarios(),
    enabled: !!focusedEvalId,
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

  // NOTE: no 404 self-clear here. A not_found agent-run must render an
  // explicit empty/error state (handled by the `!q.data` guard below) —
  // it must NOT mutate run ownership. The old self-clear caused the
  // capsule to flicker as a sibling-eval 404 wiped the active run that
  // the route owner had just set. Cleanup is now the route owner's
  // job (unconditional unmount effect), not the dock slot's.

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

  // Live-money discriminator for the capsule prefix + pop-out target.
  // `is_live_money` is THE backend signal: parent eval run `venue_label=live`
  // (real money) and non-terminal. Forward-test runs (mode=live, venue=paper/
  // testnet) are NOT live money, so they never wear the LIVE prefix.
  const isLiveMoney = summary.is_live_money === true;

  const focusedAgentId = summary.agent_id ?? summary.strategy_id ?? "agent";
  const focusedScenarioId = focusedEvalQ.data?.summary.scenario_id ?? "scenario";
  const focusedTone = deriveFocusedTone(summary, mode);
  // QA30: surface the focused eval's PnL (% return) so the capsule
  // shows "is this making money" at a glance. Pulled off the linked
  // eval-run summary because the agent-run summary doesn't carry it.
  const focusedPnl = formatPnlPct(
    focusedEvalQ.data?.summary.total_return_pct ?? null,
  );
  const focused: EvalCapsuleFocused = {
    id: summary.run_id,
    kind: isLiveMoney ? "live" : "eval",
    short: shortTag(
      agentNameById.get(focusedAgentId) ?? null,
      scenarioNameById.get(focusedScenarioId) ?? null,
      focusedAgentId,
      focusedScenarioId,
    ),
    status: focusedTone,
    spans: summary.span_count,
    elapsed: isLive ? fmtElapsedSec(liveDurationSec) : fmtPostHoc(frozenDurationMs),
    cost: formatSpendUsd(summary.total_cost_usd),
    pnl: focusedPnl,
    currentSpan: liveChip,
  };

  // Merge the polled slices. Siblings are eval-only (gated above), so when
  // there's no focused eval-link the lists are empty by construction.
  // `failed` is recency-filtered so we surface freshly-errored siblings
  // (the auto-promote / auto-open behavior the design promises) without
  // dragging in historical failures.
  const nowMs = Date.now();
  const runningSiblings = runningQ.data ?? [];
  const failedSiblings = (failedQ.data ?? []).filter((r) => {
    if (!r.completed_at) return false;
    const finished = new Date(r.completed_at).getTime();
    if (!Number.isFinite(finished)) return false;
    return nowMs - finished <= RECENT_FAILURE_WINDOW_MS;
  });
  const siblings: EvalCapsuleRow[] = [...runningSiblings, ...failedSiblings]
    .filter((r) =>
      // Exclude the focused eval-run from its own sibling list. Guarded
      // above on `focusedEvalId != null`, so the inner branch is the
      // only one that runs in practice, but we keep the legacy fallback
      // for safety in case the gate is loosened later.
      focusedEvalId == null ? r.id !== summary.run_id : r.id !== focusedEvalId,
    )
    .map((r) => {
      const sibStartedMs = new Date(r.started_at).getTime();
      const sibElapsedSec = Number.isFinite(sibStartedMs)
        ? Math.max(0, Math.floor((nowMs - sibStartedMs) / 1000))
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
        // QA30: sibling rows previously hardcoded "—" for spans + cost.
        // Span counts live on the AgentRunSummary (one-per-eval lookup);
        // making that hop is a follow-on task because `RunSummary` (the
        // wire shape the eval-runs list returns) doesn't carry
        // `agent_run_id`. As a partial improvement, surface the LLM
        // inference cost already on `RunSummary` so the operator sees a
        // running cost figure for each concurrent eval at a glance.
        spans: "—",
        elapsed: fmtElapsedSec(sibElapsedSec),
        cost: formatSpendUsd(r.inference_cost_quote_total ?? null),
        pnl: formatPnlPct(r.total_return_pct ?? null),
      };
    });

  // Live routes get the dedicated LiveCapsule: a single focused run row plus a
  // compact orders section listing the run's `broker.call` spans. There's no
  // sibling stack (live is single-run), so the eval-only sibling polling above
  // simply yields an empty list for these runs. Eval routes keep the eval
  // capsule (focused row + concurrent-cluster sibling stack) exactly as before.
  if (scope === "live") {
    const brokerCallSpans = q.data.spans.filter((s) => s.kind === "broker.call");
    return (
      <LiveCapsule
        run={focused}
        brokerSpans={brokerCallSpans}
        retentionMode={summary.retention_mode}
        onExpandDock={() => setHeight("working")}
        onPopOut={() => navigate(`/live/runs/${activeRunId}`)}
      />
    );
  }

  return (
    <EvalCapsule
      focused={focused}
      siblings={siblings}
      retentionMode={summary.retention_mode}
      onSwitchFocus={(run) => navigate(`/eval-runs/${encodeURIComponent(run.id)}`)}
      onExpandDock={() => setHeight("working")}
      onPopOut={() =>
        navigate(
          isLiveMoney
            ? `/live/runs/${activeRunId}`
            : `/agent-runs/${activeRunId}`,
        )
      }
    />
  );
}
