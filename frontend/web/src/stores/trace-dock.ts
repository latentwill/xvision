// frontend/web/src/stores/trace-dock.ts
import { create } from "zustand";
import type {
  AgentRunStreamEvent,
  SpanKind,
} from "@/api/types-agent-runs";

export type DockHeight = "collapsed" | "peek" | "working" | "full";
export type DockMode = "post-hoc" | "live";

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
  selectedSpanId: string | null;
  activeRunId: string | null;
  /**
   * Chat-rail session the dock is bound to, when the active surface is a
   * chat session rather than a standalone agent run. When set, the dock's
   * span view projects from the unified `session-events` store (one stream,
   * two projections — Phase 1.2/1.4) instead of the agent-run SSE wire.
   * `null` keeps the existing agent-run path untouched.
   */
  activeSessionId: string | null;
  mode: DockMode;
  /** Last non-collapsed height — restored by toggle(). */
  lastOpenHeight: DockHeight;
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
   * Eval-side cost override pushed by `eval-runs-detail` so the floating
   * capsule renders the same number as the page meta strip. The eval's
   * `inference_cost_quote_total` is the authoritative aggregate; the
   * agent-run rollup (`summary.total_cost_usd`) can lag or stay at zero
   * for runs whose pricing data lives only in the eval table. When
   * `null` the capsule falls back to the agent-run summary value. Reset
   * to `null` on every `setActiveRun` so it never leaks across runs.
   */
  costOverrideUsd: number | null;
};

type Actions = {
  setHeight: (h: DockHeight) => void;
  setHeightPx: (px: number) => void;
  toggle: () => void;
  minimize: () => void;
  setSelectedSpan: (id: string | null) => void;
  setActiveRun: (id: string | null, mode: DockMode) => void;
  /** Bind (or clear) the chat-rail session whose unified log feeds the dock. */
  setActiveSession: (sessionId: string | null) => void;
  markSpanActive: (spanId: string, meta?: ActiveSpanMeta) => void;
  markSpanInactive: (spanId: string) => void;
  appendDelta: (spanId: string, len: number, text?: string) => void;
  recordLag: (n: number) => void;
  applyStreamEvent: (ev: AgentRunStreamEvent) => void;
  resetStreamingState: () => void;
  setAdvancedView: (v: boolean) => void;
  setCostOverrideUsd: (v: number | null) => void;
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

export const DOCK_HEIGHT_STORAGE_KEY = "xvision.trace-dock.height";
export const DOCK_ADVANCED_VIEW_STORAGE_KEY = "xvision.trace-dock.advanced-view";
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

export const useTraceDock = create<State & Actions>((set, get) => ({
  height: "collapsed",
  heightPx: readPersistedHeightPx(),
  selectedSpanId: null,
  activeRunId: null,
  activeSessionId: null,
  mode: "post-hoc",
  lastOpenHeight: "working",
  streamingState: EMPTY_STREAMING,
  advanced_view: readPersistedAdvancedView(),
  costOverrideUsd: null,
  setAdvancedView: (v) => {
    writePersistedAdvancedView(v);
    set({ advanced_view: v });
  },
  setCostOverrideUsd: (v) => set({ costOverrideUsd: v }),
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
  setSelectedSpan: (id) => set({ selectedSpanId: id }),
  setActiveRun: (id, mode) =>
    set({
      activeRunId: id,
      mode,
      selectedSpanId: null,
      streamingState: freshStreaming(),
      costOverrideUsd: null,
    }),
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
      // them. When the SSE wire is later extended to forward
      // `engine_event` frames (separate contract), add the arm here.
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
