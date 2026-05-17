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
  droppedEvents: number;
};

type State = {
  height: DockHeight;
  selectedSpanId: string | null;
  activeRunId: string | null;
  mode: DockMode;
  /** Last non-collapsed height — restored by toggle(). */
  lastOpenHeight: DockHeight;
  streamingState: StreamingState;
};

type Actions = {
  setHeight: (h: DockHeight) => void;
  toggle: () => void;
  minimize: () => void;
  setSelectedSpan: (id: string | null) => void;
  setActiveRun: (id: string | null, mode: DockMode) => void;
  markSpanActive: (spanId: string, meta?: ActiveSpanMeta) => void;
  markSpanInactive: (spanId: string) => void;
  appendDelta: (spanId: string, len: number) => void;
  recordLag: (n: number) => void;
  applyStreamEvent: (ev: AgentRunStreamEvent) => void;
  resetStreamingState: () => void;
};

const EMPTY_STREAMING: StreamingState = {
  activeSpanIds: new Set<string>(),
  activeSpanMeta: {},
  deltaCharsBySpan: {},
  droppedEvents: 0,
};

function freshStreaming(): StreamingState {
  return {
    activeSpanIds: new Set<string>(),
    activeSpanMeta: {},
    deltaCharsBySpan: {},
    droppedEvents: 0,
  };
}

export const useTraceDock = create<State & Actions>((set, get) => ({
  height: "collapsed",
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
  appendDelta: (spanId, len) =>
    set((s) => ({
      streamingState: {
        ...s.streamingState,
        deltaCharsBySpan: {
          ...s.streamingState.deltaCharsBySpan,
          [spanId]: (s.streamingState.deltaCharsBySpan[spanId] ?? 0) + len,
        },
      },
    })),
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
        // Seed activeSpanIds from any in-flight spans in the snapshot so
        // a late subscriber sees the live chip without waiting for the
        // next `span_started` event.
        for (const span of ev.data.spans) {
          if (span.finished_at == null) {
            actions.markSpanActive(span.span_id, {
              name: span.name,
              started_at: span.started_at,
              kind: span.kind,
            });
          }
        }
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
        actions.appendDelta(ev.data.span_id, ev.data.delta_len);
        return;
      }
      case "lagged": {
        actions.recordLag(ev.data.dropped);
        // Surface a quiet console warning so a developer notices in
        // devtools without us painting a popup.
        if (typeof console !== "undefined") {
          console.warn(
            `[trace-dock] backpressure: dropped ${ev.data.dropped} stream event(s)`,
          );
        }
        return;
      }
      // Lifecycle / informational arms — no streaming-state side effect.
      case "run_started":
      case "run_finished":
      case "run_interrupted":
      case "tool_call_started":
      case "sidecar_error":
      case "span":
      case "summary":
        return;
    }
  },
}));
