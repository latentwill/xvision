// frontend/web/src/features/agent-runs/TraceDock.tsx
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ApiError } from "@/api/client";
import {
  agentRunKeys,
  engineEventFrameToSpan,
  getAgentRun,
  openAgentRunStream,
} from "@/api/agent-runs";
import type { AgentRunDetail, RunSpan } from "@/api/types-agent-runs";
import {
  applyUnifiedToDetail,
  freshLiveStreamState,
  type LiveStreamState,
} from "./live-stream-reducer";
import { formatCostUsd, formatCostUsdPrecise } from "@/lib/format";
import { useTraceDock } from "@/stores/trace-dock";
import { useCurrentTraceScope } from "./use-trace-scope";
import {
  type SpanProjection,
  useSessionSpans,
} from "@/stores/session-events";
import { DockResizeHandle } from "./DockResizeHandle";
import { FlameGraph } from "./FlameGraph";
import { SpanTree } from "./SpanTree";
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

/**
 * Kinds hidden in BOTH Simple and Advanced views. These are spans that
 * carry no operator-actionable information and exist only as
 * machine-readable OTel markers. The Advanced toggle is for revealing
 * useful instrumentation that's noisy in Simple — not for surfacing
 * empty stubs.
 *
 * `state.transition` — emitted at run start (`(start) → running`) and
 * at each terminal transition. Carries only the from/to label; the
 * supervisor-category coloring made it look important in the trace
 * tree but it never had a payload worth inspecting. Hiding it
 * everywhere declutters the trace without dropping the OTel marker
 * (the engine still emits it; only the UI suppresses it).
 */
const ALWAYS_HIDDEN_KINDS: ReadonlySet<string> = new Set(["state.transition"]);

/**
 * Project a unified `SpanProjection` (from the shared `session-events` store)
 * onto the dock's `RunSpan` model so a chat session and a standalone agent
 * run feed the SAME flame-graph / inspector surface — one event log, two
 * projections (Phase 1.2/1.4). Kinds that aren't `SpanKind` variants fall
 * back to `agent.run` so the trace tree still renders.
 */
const KNOWN_SPAN_KINDS: ReadonlySet<string> = new Set<SpanKindLike>([
  "agent.run",
  "agent.plan",
  "agent.decision",
  // WS-17 span taxonomy (+ `model.call`/`model.reasoning` legacy aliases).
  "decision.model",
  "decision.reasoning",
  "model.call",
  "model.reasoning",
  "tool.call",
  "tool.validate_input",
  "tool.validate_output",
  "approval.request",
  "approval.response",
  "sandbox.exec",
  "supervisor.review",
  "financial.eval",
  "artifact.write",
  "ipc.notification",
  "skill.invoke",
  "broker.call",
  "recovery.attempt",
  "state.transition",
  // WS-8 Part 2: the synthetic kind for projected engine lifecycle signals
  // (risk veto / regime / order / memory …). Keeping it in the known set means
  // `projectionToRunSpan` preserves it instead of flattening to `agent.run`;
  // the carried `attributes.engine_event_kind` then drives the family/label.
  "engine.event",
]);
type SpanKindLike = RunSpan["kind"];

export function projectionToRunSpan(p: SpanProjection): RunSpan {
  const kind = (
    KNOWN_SPAN_KINDS.has(p.kind) ? p.kind : "agent.run"
  ) as RunSpan["kind"];
  return {
    span_id: p.spanId,
    parent_span_id: p.parentSpanId,
    name: p.name,
    kind,
    started_at: p.startedAt,
    finished_at: p.finishedAt,
    status:
      p.status === "in_progress"
        ? "in_progress"
        : p.status === "error"
          ? "error"
          : "ok",
    // Carry the projection's attribute bag through (WS-8 Part 2): an
    // `engine.event` row resolves its family/label off
    // `attributes.engine_event_kind`, so dropping it would re-blank the row.
    attributes: p.attributes,
    // Inspector fidelity (WS-8 Part 2 Part B): the unified projection populates
    // these off the rich payload variants; carry each onto `RunSpan` so the
    // SpanInspector renders the SAME model body/tokens/cost, broker fill, tool
    // I/O, decision index, and error the raw agent-run path shows. Spread only
    // the present fields so an undefined never overwrites a fixture value and
    // the wire shape stays minimal.
    ...(p.provider !== undefined ? { provider: p.provider } : {}),
    ...(p.model !== undefined ? { model: p.model } : {}),
    ...(p.tokensIn !== undefined ? { tokens_in: p.tokensIn } : {}),
    ...(p.tokensOut !== undefined ? { tokens_out: p.tokensOut } : {}),
    ...(p.cost !== undefined ? { cost: p.cost } : {}),
    ...(p.promptHash !== undefined ? { hash: p.promptHash } : {}),
    ...(p.responseHash !== undefined ? { response_hash: p.responseHash } : {}),
    ...(p.prompt !== undefined ? { prompt: p.prompt } : {}),
    ...(p.response !== undefined ? { response: p.response } : {}),
    ...(p.promptPayloadRef !== undefined ? { prompt_payload_ref: p.promptPayloadRef } : {}),
    ...(p.responsePayloadRef !== undefined ? { response_payload_ref: p.responsePayloadRef } : {}),
    ...(p.args !== undefined ? { args: p.args } : {}),
    ...(p.result !== undefined ? { result: p.result } : {}),
    ...(p.brokerCall !== undefined ? { broker_call: p.brokerCall } : {}),
    ...(p.decisionIdx !== undefined ? { decision_idx: p.decisionIdx } : {}),
    ...(p.errorMessage !== undefined ? { error_message: p.errorMessage } : {}),
  };
}

export function TraceDock() {
  const scope = useCurrentTraceScope();
  const {
    height,
    heightPx,
    activeSessionId,
    minimize,
    advanced_view,
    setAdvancedView,
  } = useTraceDock();
  // Per-scope slice for the current route. `setSelectedSpan` is now
  // scope-aware, so we curry the scope in at the call site.
  const activeRunId = useTraceDock((s) => s.byScope[scope].activeRunId);
  const selectedSpanId = useTraceDock((s) => s.byScope[scope].selectedSpanId);
  const costOverrideUsd = useTraceDock((s) => s.byScope[scope].costOverrideUsd);
  const setSelectedSpan = (id: string | null) =>
    useTraceDock.getState().setSelectedSpan(scope, id);
  const navigate = useNavigate();

  // Structured-view selector. The collapsible span tree (WS-16) is the new
  // default — it lets a DECISION collapse to one line and expand to its full
  // subtree. The FlameGraph remains available behind the toggle (timeline /
  // overlap view); neither replaces the other.
  const [structuredView, setStructuredView] = useState<"tree" | "flame">("tree");

  // Unified-session span projection — the chat-session path. When a chat
  // session is bound (and no standalone agent run is active), the dock
  // projects from the shared session-events store instead of the agent-run
  // SSE wire. The agent-run path below is left untouched.
  const sessionSpans = useSessionSpans(
    activeRunId ? null : activeSessionId,
  );
  const sessionRunSpans = useMemo(
    () => sessionSpans.map(projectionToRunSpan),
    [sessionSpans],
  );

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

  // NOTE: no 404 self-clear here (WS-2). A not_found agent-run renders
  // the dock's empty branch (`!activeRunId && !hasSessionTrace` guard
  // below); it must NOT null the active run. Ownership/cleanup belongs
  // to the route owner's unconditional unmount effect, not the dock.

  // The dock has one span source at a time: the agent-run query when a run is
  // active, else the unified session projection when a chat session is bound.
  const sourceSpans: RunSpan[] = activeRunId
    ? (q.data?.spans ?? [])
    : sessionRunSpans;

  const filter = useSpanFilter({
    runId: activeRunId ?? activeSessionId ?? "",
    spans: sourceSpans,
  });

  // Simple mode hides instrumentation kinds at the render boundary
  // (F-7). Selected span lookups still consult the unfiltered
  // `filter.filtered` so flipping the toggle does not auto-clear a
  // selection that lives in a hidden kind — the operator can switch
  // to Advanced and stay on the same span.
  //
  // `ALWAYS_HIDDEN_KINDS` applies in both modes: empty stubs the
  // Advanced toggle shouldn't surface.
  const displaySpans: RunSpan[] = useMemo(
    () => {
      const base = filter.filtered.filter((s) => !ALWAYS_HIDDEN_KINDS.has(s.kind));
      return advanced_view ? base : base.filter((s) => !SIMPLE_HIDDEN_KINDS.has(s.kind));
    },
    [advanced_view, filter.filtered],
  );

  // Count of spans Simple mode would hide. Surfaced on the SIMPLE toggle
  // so operators can see the toggle is doing work; without this the
  // button reads as inert when the run has few instrumentation spans.
  const simpleHiddenCount = useMemo(
    () => filter.filtered.filter((s) => SIMPLE_HIDDEN_KINDS.has(s.kind)).length,
    [filter.filtered],
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
  const decisions = useMemo(() => deriveDecisions(sourceSpans), [sourceSpans]);

  // F-7 (qa round 7): the Trade quick-filter is enabled iff the run
  // carries at least one broker.call span. When the executor stage
  // hasn't emitted a broker submit (briefing-only runs, planning runs,
  // or any cycle that never reached the trader's APPROVED branch) the
  // button is rendered disabled — the affordance still exists so the
  // operator knows it's a first-class concept, but a click would be a
  // no-op.
  const brokerCallSpans = useMemo(
    () => sourceSpans.filter((s) => s.kind === "broker.call"),
    [sourceSpans],
  );
  const tradeAvailable = brokerCallSpans.length > 0;
  const onShowTrade = () => {
    if (!tradeAvailable) return;
    // Filter the dock to the trade-flow kinds (`broker` + `model`)
    // and select the first broker.call so the inspector immediately
    // shows BrokerCall + the trader's decision context. Operator can
    // step through subsequent brokers via flame-graph clicks; the
    // model.call rows alongside surface the TraderDecision summaries
    // via F-5's PAYLOAD REF labels.
    filter.setKinds(["broker", "model"]);
    setSelectedSpan(brokerCallSpans[0].span_id);
  };

  const summary = q.data?.summary;
  const isLive = summary?.status === "running";

  const qc = useQueryClient();
  useEffect(() => {
    if (!activeRunId || !isLive) return;
    const key = agentRunKeys.run(activeRunId);
    // WS-8 Part 2 B2: per-stream projection accumulator. Holds the folded
    // `SpanProjection[]` across `unified` frames so the dock reconstructs span
    // detail (model body/tokens/cost, broker fill, tool I/O, engine rows,
    // error) WITHOUT a per-frame export refetch. Reset on every `snapshot`
    // (the authoritative resync / reconnect seed).
    let liveState: LiveStreamState = freshLiveStreamState();
    const close = openAgentRunStream(activeRunId, (ev) => {
      switch (ev.event) {
        // ── WS-8 Part 2 B2: the converged LIVE tail ────────────────────────
        // Every live frame is a `UnifiedEvent`. Fold it onto the cached detail
        // via the shared fidelity-complete projection. The reducer asks for a
        // refetch ONLY on a terminal frame (canonical run-level aggregates);
        // span DETAIL never triggers a refetch — that's the whole point of B2.
        case "unified": {
          const prev = qc.getQueryData<AgentRunDetail>(key) ?? null;
          const out = applyUnifiedToDetail(prev, liveState, ev.data);
          liveState = out.state;
          // Only write when we actually have a cached detail to mutate (the
          // snapshot seeds it first; a frame that races ahead is a no-op until
          // then). `applyUnifiedToDetail` is reference-stable on no-op.
          if (out.detail && out.detail !== prev) {
            qc.setQueryData<AgentRunDetail>(key, out.detail);
          }
          if (out.requestRefetch) {
            // Single aggregate refetch on terminal — pulls canonical
            // span_count / model_call_count / total_cost / retention the
            // stream doesn't carry. NOT a per-detail-frame refetch.
            qc.invalidateQueries({ queryKey: key });
          }
          return;
        }
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
          // Authoritative resync — replaces the cached detail wholesale AND
          // resets the per-stream live projection so the next `unified` frames
          // fold onto the fresh seed (reconnect must not double-count).
          liveState = freshLiveStreamState();
          qc.setQueryData<AgentRunDetail>(key, ev.data);
          return;
        // ── Legacy raw-`RunEvent`-name arms (back-compat only) ─────────────
        // The current backend emits ONLY `snapshot` + `unified` (+ `lagged`),
        // so these never fire against it. Retained so a stale backend or the
        // integration shim that still emits raw frames keeps rendering rather
        // than going dark. They DO still refetch on terminal/detail frames —
        // that's acceptable for the legacy path; the converged path above does
        // not.
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
        case "engine_event": {
          // WS-8: project the live engine event onto an engine.event row and
          // append it to the cached detail so the trace surfaces lifecycle
          // signals (risk veto, regime transition, order state, …) in real
          // time instead of dropping them. Carrier kinds + kindless frames
          // project to null and are skipped.
          const projected = engineEventFrameToSpan(ev.data);
          if (!projected) return;
          qc.setQueryData<AgentRunDetail>(key, (prev) => {
            if (!prev) return prev;
            if (prev.spans.some((s) => s.span_id === projected.span_id)) {
              return prev;
            }
            return { ...prev, spans: [...prev.spans, projected] };
          });
          return;
        }
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
        case "memory_recall":
        case "memory_write":
        case "lagged":
          return;
      }
    });
    return close;
  }, [activeRunId, isLive, qc]);

  // The dock renders for a standalone agent run, OR for a chat session that
  // has produced trace-worthy spans in the unified log (one source, two
  // projections). A bound session with no spans yet keeps the dock dormant
  // so it never pops open uninvited (NO-POPUP rule).
  const hasSessionTrace = !activeRunId && sessionRunSpans.length > 0;
  if (!activeRunId && !hasSessionTrace) return null;
  if (height === "collapsed") return null;

  return (
    <div
      data-testid="trace-dock"
      className="fixed bottom-0 left-0 right-0 z-30 bg-bg border-t border-border shadow-2xl flex flex-col"
      style={{ height: heightPx, maxHeight: "calc(100vh - 60px)" }}
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
            <span
              data-testid="trace-dock-cost"
              title={formatCostUsdPrecise(costOverrideUsd ?? summary.total_cost_usd)}
            >
              {formatCostUsd(costOverrideUsd ?? summary.total_cost_usd)}
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
            title={
              simpleHiddenCount > 0
                ? `Simple — hide ${simpleHiddenCount} instrumentation span${simpleHiddenCount === 1 ? "" : "s"}, collapse attribute bag`
                : "Simple — hide instrumentation spans, collapse attribute bag"
            }
            className="h-6 px-1.5 text-[10px] font-mono tracking-[0.14em] flex items-center gap-1"
            style={{
              background: !advanced_view ? "var(--surface-card)" : "transparent",
              border: `1px solid ${!advanced_view ? "var(--text-2)" : "var(--border)"}`,
              color: !advanced_view ? "var(--text)" : "var(--text-3)",
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
        {/*
          WS-16: structured-view selector. TREE is the collapsible nested
          span tree (default); FLAME is the timeline/overlap flame graph.
          Both read the same `displaySpans` so Simple/Advanced + filters
          apply identically to either view.
        */}
        <div
          role="group"
          aria-label="Structured view"
          data-testid="trace-dock-view-toggle"
          className="ml-2 flex items-center gap-0.5"
        >
          <button
            type="button"
            aria-pressed={structuredView === "tree"}
            onClick={() => setStructuredView("tree")}
            title="Tree — collapsible nested spans"
            className="h-6 px-1.5 text-[10px] font-mono tracking-[0.14em] flex items-center"
            style={{
              background: structuredView === "tree" ? "var(--surface-card)" : "transparent",
              border: `1px solid ${structuredView === "tree" ? "var(--text-2)" : "var(--border)"}`,
              color: structuredView === "tree" ? "var(--text)" : "var(--text-3)",
              borderRadius: 4,
            }}
          >
            TREE
          </button>
          <button
            type="button"
            aria-pressed={structuredView === "flame"}
            onClick={() => setStructuredView("flame")}
            title="Flame — timeline / overlap view"
            className="h-6 px-1.5 text-[10px] font-mono tracking-[0.14em] flex items-center"
            style={{
              background: structuredView === "flame" ? "var(--surface-card)" : "transparent",
              border: `1px solid ${structuredView === "flame" ? "var(--text-2)" : "var(--border)"}`,
              color: structuredView === "flame" ? "var(--text)" : "var(--text-3)",
              borderRadius: 4,
            }}
          >
            FLAME
          </button>
        </div>
        <div className="ml-auto flex items-center gap-1">
          {/*
            F-7 (qa round 7): Trade quick-filter. Investigation: broker.call
            spans are emitted by `xvision-engine/src/eval/executor/paper.rs`
            (lines 1073/1103/1145, `emit_broker_call_{started,finished}`)
            and projected onto `RunSpan.broker_call` by
            `frontend/web/src/api/agent-runs.ts:178`. Trade events DO reach
            the trace; the gap operators noticed is that BROKR was not a
            first-class filter (KIND_ORDER omitted it) and there was no
            one-click view for the trade flow. The button below sets the
            kind filter to `broker` + `model` (the trader→broker pair)
            and selects the first broker.call span so the SpanInspector
            renders BROKER CALL detail immediately.
          */}
          <button
            type="button"
            data-testid="trace-dock-trade-button"
            disabled={!tradeAvailable}
            onClick={onShowTrade}
            title={
              tradeAvailable
                ? `Jump to the first broker submit (${brokerCallSpans.length} total) and filter to trader→broker spans`
                : "No broker.call spans in this run — the executor stage never reached a broker submit"
            }
            aria-label="trade view"
            className="h-6 px-1.5 text-[10px] font-mono tracking-[0.14em] flex items-center gap-1 rounded"
            style={{
              background: tradeAvailable ? "var(--surface-elev)" : "transparent",
              border: `1px solid ${tradeAvailable ? "var(--border)" : "var(--border)"}`,
              color: tradeAvailable ? "var(--text)" : "var(--text-4)",
              cursor: tradeAvailable ? "pointer" : "not-allowed",
            }}
          >
            <span aria-hidden style={{ color: "#f472b6" }}>$</span>
            TRADE
            {tradeAvailable ? (
              <span className="text-text-3 ml-0.5">{brokerCallSpans.length}</span>
            ) : null}
          </button>
          <span aria-hidden className="opacity-30 px-1">|</span>
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
          {activeRunId ? (
            <>
              <div data-testid="trace-dock-export" className="flex items-center gap-1">
                <TraceDownloadButton runId={activeRunId} />
              </div>
              <span aria-hidden className="opacity-30 px-1">|</span>
              <button
                type="button"
                aria-label="pop out to dedicated view"
                title="Open in dedicated route"
                onClick={() => {
                  // QA22 / `trace-capsule-fullscreen-minimize`: the
                  // dedicated agent-run route shows the same trace at full
                  // size, so the dock + the route would be redundant.
                  // Minimize the dock as we navigate.
                  minimize();
                  navigate(`/agent-runs/${activeRunId}`);
                }}
                className="h-7 w-8 inline-flex items-center justify-center rounded text-text-3 hover:text-text hover:bg-surface-elev"
              >
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                  <path d="M6 3h7v7M13 3l-7 7M3 8v5h5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </button>
            </>
          ) : null}
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
          {q.data || hasSessionTrace ? (
            structuredView === "tree" ? (
              <SpanTree
                spans={displaySpans}
                selectedSpanId={selectedSpan?.span_id ?? null}
                onSelect={setSelectedSpan}
              />
            ) : (
              <FlameGraph
                spans={displaySpans}
                selectedSpanId={selectedSpan?.span_id ?? null}
                onSelect={setSelectedSpan}
              />
            )
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
              runSummary={q.data?.summary}
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
