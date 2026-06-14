// frontend/web/src/stores/trace-dock.ts
import { create } from "zustand";
import type {
  AgentRunStreamEvent,
  SpanKind,
} from "@/api/types-agent-runs";

export type DockHeight = "collapsed" | "peek" | "working" | "full";
export type DockMode = "post-hoc" | "live";

/**
 * Trace surface the dock state belongs to. The dashboard runs two
 * independent trace contexts — the eval surfaces (`/eval-runs/*`, the
 * standalone `/agent-runs/:runId`) and the live surfaces (`/live*`) —
 * and each owns its own active run / selection / mode. Splitting the
 * store per-scope kills the "capsule floats/flickers/follows to other
 * pages" bug: nulling one scope on unmount no longer clobbers the
 * other surface's run, and the floating capsule only renders for the
 * scope that matches the current route.
 *
 * WS-11a adds a third scope, `opti` — the autooptimizer *cycle* projected onto
 * the trace dock on the `/optimizer` route. It owns `byScope.opti`; the
 * eval/live routes must not touch it (and vice versa). The cycle rows
 * themselves are derived from the existing cycle SSE stream by
 * `features/autooptimizer/opti-trace-reducer.ts` — the slice here only tracks
 * the active cycle id, selection, mode, and cost override, mirroring eval/live.
 */
export type TraceScope = "eval" | "live" | "opti";

/**
 * Per-scope dock slice. One of these exists for each {@link TraceScope}.
 * The fields here were previously top-level on the store; they moved
 * under `byScope` so the eval and live surfaces can't trample each
 * other's run/selection state.
 */
type ScopeState = {
  activeRunId: string | null;
  selectedSpanId: string | null;
  mode: DockMode;
  costOverrideUsd: number | null;
};

/**
 * Metadata captured from `span_started` / snapshot frames so consumers
 * (e.g. RunStatusStrip) can render an active-span chip without needing
 * a second query for span details.
 */
export type ActiveSpanMeta = {
  name: string;
  started_at: string;
  kind: SpanKind;
};

/**
 * Live-stream slice. Populated by `applyStreamEvent` as the SSE feed
 * arrives. Reset on `setActiveRun` so per-run state never leaks between
 * runs.
 *
 * Why metadata lives here: contract acceptance asks RunStatusStrip to
 * show the currently-active span (highest started_at) with elapsed
 * time. That requires `name` + `started_at` + `kind` per active span;
 * the strip only receives `summary` as a prop. Caching the metadata
 * in the streaming slice keeps the strip self-sufficient.
 */
export type StreamingState = {
  activeSpanIds: Set<string>;
  activeSpanMeta: Record<string, ActiveSpanMeta>;
  deltaCharsBySpan: Record<string, number>;
  /**
   * Accumulated assistant body per span from `assistant_text_delta.delta_text`
   * frames. SpanInspector reads this for the live STREAMING pull-quote so
   * the operator sees the response body grow in real time. Empty when the
   * wire only carries `delta_len` (older sidecars / non-streaming
   * dispatchers).
   */
  bodiesBySpan: Record<string, string>;
  droppedEvents: number;
};

type State = {
  height: DockHeight;
  /**
   * User-controlled pixel height for the dock when not collapsed. The drag
   * handle on the dock's top edge writes this slice; the named `height`
   * enum is retained for back-compat with callers that still poke a preset
   * ("peek" / "working" / "full"), but the rendered height comes from
   * `heightPx`. Persisted under `xvision.trace-dock.height` in localStorage.
   */
  heightPx: number;
  /**
   * Per-scope run / selection / mode / cost-override state. The eval and
   * live surfaces each own one slice; route owners write only their own
   * scope and the floating capsule reads the slice for the current route
   * (see `useCurrentTraceScope`). This is the core of the WS-2 reshape —
   * a single global `activeRunId` used to leak across surfaces.
   */
  byScope: Record<TraceScope, ScopeState>;
  /**
   * Chat-rail session the dock is bound to, when the active surface is a
   * chat session rather than a standalone agent run. When set, the dock's
   * span view projects from the unified `session-events` store (one stream,
   * two projections — Phase 1.2/1.4) instead of the agent-run SSE wire.
   * `null` keeps the existing agent-run path untouched.
   *
   * Shared (not per-scope): the chat rail is a single global surface.
   */
  activeSessionId: string | null;
  /** Last non-collapsed height — restored by toggle(). */
  lastOpenHeight: DockHeight;
  /**
   * Live-stream slice. SHARED across scopes — only one live SSE stream
   * runs at a time, so the streaming actions stay scope-free (operator
   * decision; per-scope streaming is deferred to a later WU).
   */
  streamingState: StreamingState;
  /**
   * Trace-view density. `false` = Simple (default): hide instrumentation
   * spans (`tool.validate_input` / `tool.validate_output` /
   * `state.transition`) and collapse the SpanInspector attribute bag to
   * a one-line summary. `true` = Advanced: show every span and the full
   * attribute grid. Persisted under
   * `xvision.trace-dock.advanced-view` in localStorage.
   *
   * Added by F-7 (`trace-dock-simple-advanced-toggle`). The new F-4 span
   * kinds + F-2 populated attribute bag would otherwise make a real run
   * unreadable for triage — Simple mode is the operator's everyday view;
   * Advanced is the forensics view.
   */
  advanced_view: boolean;
  /**
   * Set of span ids whose subtree is COLLAPSED in the structured span-tree
   * view (WS-16). A collapsed parent hides its entire descendant subtree
   * and renders a one-line rollup; expanding restores the subtree. SHARED
   * across scopes (it's a UI pref keyed by span id, and only one trace is
   * inspected at a time), and persisted under
   * `xvision.trace-dock.collapsed-spans` in localStorage so the operator's
   * collapse choices survive a reload. Span ids not present in the run are
   * simply inert — a stale persisted id costs nothing.
   */
  collapsedSpanIds: Set<string>;
};

type Actions = {
  setHeight: (h: DockHeight) => void;
  setHeightPx: (px: number) => void;
  toggle: () => void;
  minimize: () => void;
  setSelectedSpan: (scope: TraceScope, id: string | null) => void;
  /**
   * Point `scope`'s dock at `id` in `mode`. Resets that scope's
   * selection + cost override (so per-run state never leaks across
   * runs) AND resets the SHARED streaming slice (preserving the
   * existing reset-on-run-switch behavior). The OTHER scope and the
   * shared `activeSessionId` are left untouched.
   */
  setActiveRun: (scope: TraceScope, id: string | null, mode: DockMode) => void;
  /** Bind (or clear) the chat-rail session whose unified log feeds the dock. */
  setActiveSession: (sessionId: string | null) => void;
  markSpanActive: (spanId: string, meta?: ActiveSpanMeta) => void;
  markSpanInactive: (spanId: string) => void;
  appendDelta: (spanId: string, len: number, text?: string) => void;
  recordLag: (n: number) => void;
  applyStreamEvent: (ev: AgentRunStreamEvent) => void;
  resetStreamingState: () => void;
  setAdvancedView: (v: boolean) => void;
  setCostOverrideUsd: (scope: TraceScope, v: number | null) => void;
  /** Flip a single node's collapsed state in the span-tree view. */
  toggleSpanCollapsed: (spanId: string) => void;
  /** Collapse every supplied node id (typically all nodes with children). */
  collapseAllSpans: (spanIds: string[]) => void;
  /** Expand every node (clear the collapsed set). */
  expandAllSpans: () => void;
  /** Replace the collapsed set wholesale (used by tests / rehydration). */
  setCollapsedSpanIds: (spanIds: string[]) => void;
};

const EMPTY_STREAMING: StreamingState = {
  activeSpanIds: new Set<string>(),
  activeSpanMeta: {},
  deltaCharsBySpan: {},
  bodiesBySpan: {},
  droppedEvents: 0,
};

function freshStreaming(): StreamingState {
  return {
    activeSpanIds: new Set<string>(),
    activeSpanMeta: {},
    deltaCharsBySpan: {},
    bodiesBySpan: {},
    droppedEvents: 0,
  };
}

/** Empty per-scope slice — used for store init and to clear a scope. */
function freshScopeState(): ScopeState {
  return {
    activeRunId: null,
    selectedSpanId: null,
    mode: "post-hoc",
    costOverrideUsd: null,
  };
}

export const DOCK_HEIGHT_STORAGE_KEY = "xvision.trace-dock.height";
export const DOCK_ADVANCED_VIEW_STORAGE_KEY = "xvision.trace-dock.advanced-view";
export const DOCK_COLLAPSED_SPANS_STORAGE_KEY =
  "xvision.trace-dock.collapsed-spans";
export const DOCK_MIN_PX = 96;
export const DEFAULT_DOCK_PX = 480;

export function dockMaxPx(): number {
  if (typeof window === "undefined") return 800;
  return Math.floor(window.innerHeight * 0.9);
}

export function clampDockPx(px: number): number {
  if (!Number.isFinite(px)) return DEFAULT_DOCK_PX;
  return Math.max(DOCK_MIN_PX, Math.min(dockMaxPx(), Math.round(px)));
}

function readPersistedHeightPx(): number {
  if (typeof window === "undefined") return DEFAULT_DOCK_PX;
  try {
    const raw = window.localStorage.getItem(DOCK_HEIGHT_STORAGE_KEY);
    if (!raw) return DEFAULT_DOCK_PX;
    const n = Number.parseInt(raw, 10);
    if (!Number.isFinite(n)) return DEFAULT_DOCK_PX;
    return clampDockPx(n);
  } catch {
    return DEFAULT_DOCK_PX;
  }
}

function writePersistedHeightPx(px: number): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(DOCK_HEIGHT_STORAGE_KEY, String(px));
  } catch {
    // Best effort only — Safari private-mode etc.
  }
}

/**
 * Read the persisted Simple/Advanced toggle. Returns `false` (Simple)
 * for any missing / malformed value so first-time visitors land in
 * Simple mode per the F-7 default.
 */
function readPersistedAdvancedView(): boolean {
  if (typeof window === "undefined") return false;
  try {
    const raw = window.localStorage.getItem(DOCK_ADVANCED_VIEW_STORAGE_KEY);
    return raw === "true";
  } catch {
    return false;
  }
}

function writePersistedAdvancedView(v: boolean): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(DOCK_ADVANCED_VIEW_STORAGE_KEY, v ? "true" : "false");
  } catch {
    // Best effort only — Safari private-mode etc.
  }
}

/**
 * Read the persisted set of collapsed span ids (WS-16). Stored as a JSON
 * array of span-id strings. Returns an empty set for any missing or
 * malformed value so a corrupt entry can never crash the dock — the worst
 * case is "everything renders expanded", the safe default.
 */
export function readPersistedCollapsedSpanIds(): Set<string> {
  if (typeof window === "undefined") return new Set();
  try {
    const raw = window.localStorage.getItem(DOCK_COLLAPSED_SPANS_STORAGE_KEY);
    if (!raw) return new Set();
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((x): x is string => typeof x === "string"));
  } catch {
    return new Set();
  }
}

function writePersistedCollapsedSpanIds(ids: Set<string>): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(
      DOCK_COLLAPSED_SPANS_STORAGE_KEY,
      JSON.stringify([...ids]),
    );
  } catch {
    // Best effort only — Safari private-mode etc.
  }
}

export const useTraceDock = create<State & Actions>((set, get) => ({
  height: "collapsed",
  heightPx: readPersistedHeightPx(),
  byScope: {
    eval: freshScopeState(),
    live: freshScopeState(),
    opti: freshScopeState(),
  },
  activeSessionId: null,
  lastOpenHeight: "working",
  streamingState: EMPTY_STREAMING,
  advanced_view: readPersistedAdvancedView(),
  collapsedSpanIds: readPersistedCollapsedSpanIds(),
  setAdvancedView: (v) => {
    writePersistedAdvancedView(v);
    set({ advanced_view: v });
  },
  toggleSpanCollapsed: (spanId) =>
    set((s) => {
      const next = new Set(s.collapsedSpanIds);
      if (next.has(spanId)) next.delete(spanId);
      else next.add(spanId);
      writePersistedCollapsedSpanIds(next);
      return { collapsedSpanIds: next };
    }),
  collapseAllSpans: (spanIds) =>
    set((s) => {
      const next = new Set(s.collapsedSpanIds);
      for (const id of spanIds) next.add(id);
      writePersistedCollapsedSpanIds(next);
      return { collapsedSpanIds: next };
    }),
  expandAllSpans: () => {
    const next = new Set<string>();
    writePersistedCollapsedSpanIds(next);
    set({ collapsedSpanIds: next });
  },
  setCollapsedSpanIds: (spanIds) => {
    const next = new Set(spanIds);
    writePersistedCollapsedSpanIds(next);
    set({ collapsedSpanIds: next });
  },
  setCostOverrideUsd: (scope, v) =>
    set((s) => ({
      byScope: {
        ...s.byScope,
        [scope]: { ...s.byScope[scope], costOverrideUsd: v },
      },
    })),
  setHeight: (h) =>
    set((s) => ({
      height: h,
      lastOpenHeight: h === "collapsed" ? s.lastOpenHeight : h,
    })),
  setHeightPx: (px) => {
    const next = clampDockPx(px);
    writePersistedHeightPx(next);
    set({ heightPx: next });
  },
  toggle: () => {
    const s = get();
    set({
      height: s.height === "collapsed" ? s.lastOpenHeight : "collapsed",
    });
  },
  minimize: () => set({ height: "collapsed" }),
  setSelectedSpan: (scope, id) =>
    set((s) => ({
      byScope: {
        ...s.byScope,
        [scope]: { ...s.byScope[scope], selectedSpanId: id },
      },
    })),
  setActiveRun: (scope, id, mode) =>
    set((s) => ({
      // Only the targeted scope's slice is rebuilt; the other scope is
      // preserved by reference so nulling one surface on unmount can't
      // clobber the other. Selection + cost override reset with the run.
      byScope: {
        ...s.byScope,
        [scope]: {
          activeRunId: id,
          selectedSpanId: null,
          mode,
          costOverrideUsd: null,
        },
      },
      // Streaming stays shared (one live stream at a time) but still
      // resets on every run switch — same behavior as before the reshape.
      streamingState: freshStreaming(),
    })),
  setActiveSession: (sessionId) => set({ activeSessionId: sessionId }),
  markSpanActive: (spanId, meta) =>
    set((s) => {
      const next = new Set(s.streamingState.activeSpanIds);
      next.add(spanId);
      const metaMap = meta
        ? { ...s.streamingState.activeSpanMeta, [spanId]: meta }
        : s.streamingState.activeSpanMeta;
      return {
        streamingState: {
          ...s.streamingState,
          activeSpanIds: next,
          activeSpanMeta: metaMap,
        },
      };
    }),
  markSpanInactive: (spanId) =>
    set((s) => {
      if (!s.streamingState.activeSpanIds.has(spanId)) return {};
      const next = new Set(s.streamingState.activeSpanIds);
      next.delete(spanId);
      const { [spanId]: _omitted, ...metaRest } = s.streamingState.activeSpanMeta;
      return {
        streamingState: {
          ...s.streamingState,
          activeSpanIds: next,
          activeSpanMeta: metaRest,
        },
      };
    }),
  appendDelta: (spanId, len, text) =>
    set((s) => {
      const nextChars = {
        ...s.streamingState.deltaCharsBySpan,
        [spanId]: (s.streamingState.deltaCharsBySpan[spanId] ?? 0) + len,
      };
      const nextBodies = text
        ? {
            ...s.streamingState.bodiesBySpan,
            [spanId]: (s.streamingState.bodiesBySpan[spanId] ?? "") + text,
          }
        : s.streamingState.bodiesBySpan;
      return {
        streamingState: {
          ...s.streamingState,
          deltaCharsBySpan: nextChars,
          bodiesBySpan: nextBodies,
        },
      };
    }),
  recordLag: (n) =>
    set((s) => ({
      streamingState: {
        ...s.streamingState,
        droppedEvents: s.streamingState.droppedEvents + n,
      },
    })),
  resetStreamingState: () =>
    set({ streamingState: freshStreaming() }),
  applyStreamEvent: (ev) => {
    const actions = get();
    switch (ev.event) {
      // WS-8 Part 2 B2: the real LIVE tail arrives as `UnifiedEvent` frames.
      // Drive the SAME streaming-slice indicators (active spans / assistant
      // deltas / lag) the legacy per-`RunEvent`-name arms below maintained,
      // reading off the unified payload. Span DETAIL is reconstructed by the
      // dock's `live-stream-reducer` (the cache mutation), not here.
      case "unified": {
        const p = ev.data.payload;
        switch (p.kind) {
          case "span_started":
            actions.markSpanActive(p.data.span_id, {
              name: p.data.name,
              started_at: p.data.started_at,
              kind: p.data.kind as SpanKind,
            });
            return;
          case "span_finished":
          case "model_call_finished":
          case "tool_finished":
          case "tool_failed":
          case "tool_cancelled":
          case "broker_call_finished":
            // Terminal events on a span — drop it from the active set if an
            // explicit span_finished hasn't already closed it.
            actions.markSpanInactive(p.data.span_id);
            return;
          case "assistant_token_delta":
            // The unified delta carries text but no per-span length count; the
            // dock's live response pull-quote appends the text and tracks its
            // length. The envelope's `span_id` scopes it.
            if (ev.data.span_id) {
              actions.appendDelta(ev.data.span_id, p.data.text.length, p.data.text);
            }
            return;
          case "backpressure_dropped":
            actions.recordLag(p.data.dropped);
            if (typeof console !== "undefined") {
              console.warn(
                `[trace-dock] backpressure: dropped ${p.data.dropped} stream event(s)`,
              );
            }
            return;
          case "run_finished":
          case "run_interrupted":
            // Run-terminal: any still-active spans are now stale. Clear the
            // streaming indicators so consumers stop showing a live chip.
            set((s) => ({
              streamingState: {
                ...s.streamingState,
                activeSpanIds: new Set<string>(),
                activeSpanMeta: {},
              },
            }));
            return;
          default:
            // Lifecycle / informational unified payloads — no streaming-slice
            // side effect (engine_event rows, memory, broker_call_started, …).
            return;
        }
      }
      case "snapshot": {
        // Authoritative resync. A reconnect snapshot must REPLACE the
        // active set, not merge into it — a span that finished while we
        // were disconnected must not stay active just because nothing
        // explicitly cleared it.
        const nextIds = new Set<string>();
        const nextMeta: Record<string, ActiveSpanMeta> = {};
        for (const span of ev.data.spans) {
          if (span.finished_at == null) {
            nextIds.add(span.span_id);
            nextMeta[span.span_id] = {
              name: span.name,
              started_at: span.started_at,
              kind: span.kind,
            };
          }
        }
        set((s) => ({
          streamingState: {
            ...s.streamingState,
            activeSpanIds: nextIds,
            activeSpanMeta: nextMeta,
          },
        }));
        return;
      }
      case "span_started": {
        actions.markSpanActive(ev.data.span_id, {
          name: ev.data.name,
          started_at: ev.data.started_at,
          kind: ev.data.kind,
        });
        return;
      }
      case "span_finished": {
        actions.markSpanInactive(ev.data.span_id);
        return;
      }
      case "model_call_finished":
      case "tool_call_finished":
      case "tool_call_failed":
      case "tool_call_cancelled":
      case "broker_call_finished": {
        // Terminal events on a span — drop it from the active set if it
        // hasn't already been closed by an explicit span_finished frame.
        actions.markSpanInactive(ev.data.span_id);
        return;
      }
      case "assistant_text_delta": {
        actions.appendDelta(
          ev.data.span_id,
          ev.data.delta_len,
          ev.data.delta_text,
        );
        return;
      }
      case "lagged":
      case "backpressure_dropped": {
        actions.recordLag(ev.data.dropped);
        if (typeof console !== "undefined") {
          console.warn(
            `[trace-dock] backpressure: dropped ${ev.data.dropped} stream event(s)`,
          );
        }
        return;
      }
      case "run_finished":
      case "run_interrupted": {
        // Run-terminal events: any still-active spans are now stale.
        // Clear streaming indicators so consumers stop showing a live
        // chip / streaming dots on a finished run.
        set((s) => ({
          streamingState: {
            ...s.streamingState,
            activeSpanIds: new Set<string>(),
            activeSpanMeta: {},
          },
        }));
        return;
      }
      // Lifecycle / informational arms — no streaming-state side effect.
      //
      // F43 (`trace-dock-emitters`) note: the engine eval executor now
      // emits bar-level lifecycle events (`decision_started`,
      // `decision_completed`, `fill_attempted`, `guardrail_fired`,
      // `early_stop_triggered`, `flat_skip_fired`, `broker_rule_violation`,
      // `tool_call_completed`) as `EngineEvent` rows in the migration-018
      // `events` table. These are surfaced through the post-hoc
      // `/api/agent-runs/<id>` projection, not the live SSE stream —
      // the dock's live streaming slice doesn't need a switch arm for
      // them. Full rendering of live `engine_event` SSE frames is owned
      // by WS-8; for now the arm is a no-op so the exhaustive switch
      // compiles cleanly.
      case "engine_event":
      case "run_started":
      case "tool_call_started":
      case "broker_call_started":
      case "sidecar_error":
      case "checkpoint_written":
      case "supervisor_note":
      case "artifact_written":
      case "memory_recall":
      case "memory_write":
      case "span":
      case "summary":
        return;
    }
  },
}));
