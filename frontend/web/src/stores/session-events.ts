// frontend/web/src/stores/session-events.ts
//
// The ONE source of truth for a chat-rail session's unified event log.
//
// Phase 1.2/1.4 contract: a single `UnifiedEvent` stream
// (`GET /api/chat-rail/sessions/:id/stream`) drives BOTH the chat rail
// rows AND the trace dock. This store owns the raw event log per session
// and the derived `MessageRow[]` projection; the rail and the dock are two
// projections of the SAME log — one stream, one event log, two surfaces.
//
//   ingest(sessionId, ev)  → idempotent (dedupe by event_id), keeps the log
//                            ordered by seq, recomputes rows via the
//                            already-committed `reduceRows` reducer.
//   rowsFor(sessionId)     → MessageRow[] for the rail (and any row consumer).
//   spansFor(sessionId)    → SpanProjection[] for the trace dock (span/run
//                            lifecycle events the row reducer intentionally
//                            does not project to rows).
//
// No reducer is duplicated here: rows come straight from
// `stores/message-row-reducer.ts`. The span projection below covers the
// run/span lifecycle the dock needs and which the row reducer passes
// through unchanged (by design — see its `assertHandledOrPassthrough`).

import { create } from "zustand";

import type { UnifiedEvent } from "@/api/unified-events";
import {
  type MessageRow,
  reduceRows,
} from "./message-row-reducer";

// ─── Span projection (trace-dock side) ──────────────────────────────────────
//
// The dock renders an active-span view. The unified stream carries
// span_started / span_finished and the terminal model/tool/broker events;
// we project them into a flat per-span record the dock can consume without
// re-walking the raw log. This mirrors what `trace-dock.ts.applyStreamEvent`
// derives from the agent-run SSE wire — but sourced from the unified log so
// a chat session and an agent run share the projection shape.

export type SpanProjectionStatus = "in_progress" | "ok" | "error" | "cancelled";

export type SpanProjection = {
  spanId: string;
  runId: string | null;
  parentSpanId: string | null;
  kind: string;
  name: string;
  startedAt: string;
  finishedAt: string | null;
  status: SpanProjectionStatus;
};

// ─── Per-session slice ──────────────────────────────────────────────────────

type SessionSlice = {
  /** Raw event log, ordered by seq (ascending). Dedicated by event_id. */
  events: UnifiedEvent[];
  /** Derived rows — folded from `events` via `reduceRows`. */
  rows: MessageRow[];
  /** Derived span projection for the trace dock. */
  spans: SpanProjection[];
  /** Set of event_ids already ingested — drives idempotent ingestion. */
  seenEventIds: Set<string>;
  /** Highest seq applied. Drives stream resume (`after_seq`). */
  lastSeq: number;
};

type State = {
  sessions: Record<string, SessionSlice>;
};

type Actions = {
  /** Ingest one unified event into a session's log. Idempotent by event_id. */
  ingest: (sessionId: string, ev: UnifiedEvent) => void;
  /** Ingest a batch (replay backfill) in one state update. */
  ingestMany: (sessionId: string, events: UnifiedEvent[]) => void;
  /** Clear a session's log (e.g. New chat / switch session). */
  reset: (sessionId: string) => void;
  /** Row projection for a session (empty array when unseen). */
  rowsFor: (sessionId: string) => MessageRow[];
  /** Span projection for a session (empty array when unseen). */
  spansFor: (sessionId: string) => SpanProjection[];
  /** Highest seq seen for a session (-1 when unseen → replay from start). */
  lastSeqFor: (sessionId: string) => number;
};

const EMPTY_ROWS: MessageRow[] = [];
const EMPTY_SPANS: SpanProjection[] = [];

function freshSlice(): SessionSlice {
  return {
    events: [],
    rows: [],
    spans: [],
    seenEventIds: new Set<string>(),
    lastSeq: -1,
  };
}

// ─── Span projection helpers ────────────────────────────────────────────────

const SPAN_KIND_BY_PAYLOAD: Record<string, string> = {
  // Best-effort kind labels when a span is first observed via a terminal
  // event rather than span_started. WS-17: a `model_call_finished` event
  // (the SSE event name is unchanged — it carries model data, not a span
  // kind) projects onto the renamed `decision.model` span kind.
  model_call_finished: "decision.model",
  tool_requested: "tool.call",
  tool_started: "tool.call",
  tool_finished: "tool.call",
  tool_failed: "tool.call",
  tool_cancelled: "tool.call",
  broker_call_started: "broker.call",
  broker_call_finished: "broker.call",
};

/**
 * Fold one event onto the span projection. Span/run lifecycle events update
 * a per-span record; terminal events close the span. Returns a new array
 * only when something changed (reference-stable otherwise).
 */
function projectSpan(
  spans: SpanProjection[],
  ev: UnifiedEvent,
): SpanProjection[] {
  const p = ev.payload;
  const runId = ev.run_id ?? null;

  const upsert = (
    spanId: string,
    update: (s: SpanProjection) => SpanProjection,
    seed?: Partial<SpanProjection>,
  ): SpanProjection[] => {
    const idx = spans.findIndex((s) => s.spanId === spanId);
    if (idx === -1) {
      const base: SpanProjection = {
        spanId,
        runId,
        parentSpanId: seed?.parentSpanId ?? null,
        kind: seed?.kind ?? SPAN_KIND_BY_PAYLOAD[p.kind] ?? "span",
        name: seed?.name ?? spanId,
        startedAt: seed?.startedAt ?? ev.ts,
        finishedAt: seed?.finishedAt ?? null,
        status: seed?.status ?? "in_progress",
      };
      return [...spans, update(base)];
    }
    return spans.map((s, i) => (i === idx ? update(s) : s));
  };

  const close = (
    spanId: string,
    status: SpanProjectionStatus,
  ): SpanProjection[] =>
    upsert(spanId, (s) => ({
      ...s,
      // Don't regress an already-terminal status.
      status: s.status === "in_progress" ? status : s.status,
      finishedAt: s.finishedAt ?? ev.ts,
    }));

  switch (p.kind) {
    case "span_started":
      return upsert(
        p.data.span_id,
        (s) => ({
          ...s,
          parentSpanId: p.data.parent_span_id ?? s.parentSpanId,
          kind: p.data.kind || s.kind,
          name: p.data.name || s.name,
          startedAt: p.data.started_at || s.startedAt,
        }),
        {
          parentSpanId: p.data.parent_span_id ?? null,
          kind: p.data.kind,
          name: p.data.name,
          startedAt: p.data.started_at,
        },
      );
    case "span_finished":
      return upsert(p.data.span_id, (s) => ({
        ...s,
        finishedAt: ev.ts,
        status:
          p.data.status === "error"
            ? "error"
            : p.data.status === "cancelled"
              ? "cancelled"
              : "ok",
      }));
    case "model_call_finished":
    case "tool_finished":
    case "broker_call_finished":
      return close(p.data.span_id, "ok");
    case "tool_failed":
      return close(p.data.span_id, "error");
    case "tool_cancelled":
      return close(p.data.span_id, "cancelled");
    case "tool_requested":
    case "tool_started":
      return upsert(p.data.span_id, (s) => s);
    case "broker_call_started":
      return upsert(p.data.span_id, (s) => s, {
        kind: "broker.call",
      });
    default:
      return spans;
  }
}

// ─── Insert-in-seq-order helper ─────────────────────────────────────────────

/** Insert `ev` into a seq-ordered log, preserving ascending seq order. */
function insertOrdered(events: UnifiedEvent[], ev: UnifiedEvent): UnifiedEvent[] {
  // Fast path: the common case is monotonic append.
  if (events.length === 0 || ev.seq >= events[events.length - 1]!.seq) {
    return [...events, ev];
  }
  const next = [...events];
  let lo = 0;
  let hi = next.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (next[mid]!.seq <= ev.seq) lo = mid + 1;
    else hi = mid;
  }
  next.splice(lo, 0, ev);
  return next;
}

// ─── Store ──────────────────────────────────────────────────────────────────

export const useSessionEvents = create<State & Actions>((set, get) => ({
  sessions: {},

  ingest: (sessionId, ev) => {
    set((state) => {
      const slice = state.sessions[sessionId] ?? freshSlice();
      // Idempotent: same event_id never applies twice.
      if (slice.seenEventIds.has(ev.event_id)) return state;

      const seen = new Set(slice.seenEventIds);
      seen.add(ev.event_id);

      const events = insertOrdered(slice.events, ev);
      // Rows: reduceRows is itself idempotent + out-of-order safe, so we
      // fold the single event onto the existing rows rather than recomputing
      // the whole log every ingest.
      const rows = reduceRows(slice.rows, ev);
      const spans = projectSpan(slice.spans, ev);

      const nextSlice: SessionSlice = {
        events,
        rows,
        spans,
        seenEventIds: seen,
        lastSeq: Math.max(slice.lastSeq, ev.seq),
      };
      return {
        sessions: { ...state.sessions, [sessionId]: nextSlice },
      };
    });
  },

  ingestMany: (sessionId, incoming) => {
    if (incoming.length === 0) return;
    set((state) => {
      const slice = state.sessions[sessionId] ?? freshSlice();
      const seen = new Set(slice.seenEventIds);
      let events = slice.events;
      let rows = slice.rows;
      let spans = slice.spans;
      let lastSeq = slice.lastSeq;
      let changed = false;

      for (const ev of incoming) {
        if (seen.has(ev.event_id)) continue;
        seen.add(ev.event_id);
        events = insertOrdered(events, ev);
        rows = reduceRows(rows, ev);
        spans = projectSpan(spans, ev);
        lastSeq = Math.max(lastSeq, ev.seq);
        changed = true;
      }
      if (!changed) return state;

      return {
        sessions: {
          ...state.sessions,
          [sessionId]: { events, rows, spans, seenEventIds: seen, lastSeq },
        },
      };
    });
  },

  reset: (sessionId) => {
    set((state) => {
      if (!state.sessions[sessionId]) return state;
      const next = { ...state.sessions };
      delete next[sessionId];
      return { sessions: next };
    });
  },

  rowsFor: (sessionId) => get().sessions[sessionId]?.rows ?? EMPTY_ROWS,
  spansFor: (sessionId) => get().sessions[sessionId]?.spans ?? EMPTY_SPANS,
  lastSeqFor: (sessionId) => get().sessions[sessionId]?.lastSeq ?? -1,
}));

// ─── Selector hooks ─────────────────────────────────────────────────────────
//
// Subscribe to a single session's projection. Returns the stable empty
// array for an unseen session so consumers don't churn on every render.

export function useSessionRows(sessionId: string | null): MessageRow[] {
  return useSessionEvents((s) =>
    sessionId ? (s.sessions[sessionId]?.rows ?? EMPTY_ROWS) : EMPTY_ROWS,
  );
}

export function useSessionSpans(sessionId: string | null): SpanProjection[] {
  return useSessionEvents((s) =>
    sessionId ? (s.sessions[sessionId]?.spans ?? EMPTY_SPANS) : EMPTY_SPANS,
  );
}
