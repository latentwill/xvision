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
  mode: DockMode;
  /** Last non-collapsed height — restored by toggle(). */
  lastOpenHeight: DockHeight;
  streamingState: StreamingState;
};

type Actions = {
  setHeight: (h: DockHeight) => void;
  setHeightPx: (px: number) => void;
  toggle: () => void;
  minimize: () => void;
  setSelectedSpan: (id: string | null) => void;
  setActiveRun: (id: string | null, mode: DockMode) => void;
  markSpanActive: (spanId: string, meta?: ActiveSpanMeta) => void;
  markSpanInactive: (spanId: string) => void;
  appendDelta: (spanId: string, len: number, text?: string) => void;
  recordLag: (n: number) => void;
  applyStreamEvent: (ev: AgentRunStreamEvent) => void;
  resetStreamingState: () => void;
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

export const useTraceDock = create<State & Actions>((set, get) => ({
  height: "collapsed",
  heightPx: readPersistedHeightPx(),
  selectedSpanId: null,
  activeRunId: null,
  mode: "post-hoc",
  lastOpenHeight: "working",
  streamingState: EMPTY_STREAMING,
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
    }),
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
      case "tool_call_cancelled": {
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
      case "run_started":
      case "tool_call_started":
      case "sidecar_error":
      case "checkpoint_written":
      case "supervisor_note":
      case "artifact_written":
      case "span":
      case "summary":
        return;
    }
  },
}));
