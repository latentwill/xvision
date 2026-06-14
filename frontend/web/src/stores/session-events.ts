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
  /**
   * Carrier bag for projected `engine.event` rows (WS-8 Part 2). The raw
   * agent-run path folds each `EngineEvent` onto a `RunSpan` whose
   * `attributes.engine_event_kind` drives the family/label/color resolution
   * in `span-colors.ts` / `engine-event-kinds.ts`. The unified projection must
   * carry the same bag so a chat-session (or, post-convergence, an agent-run)
   * trace renders identical engine-event rows. Empty `{}` for ordinary
   * lifecycle spans — they resolve their family off `kind` alone.
   */
  attributes: Record<string, unknown>;
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
 * Engine-event kinds that are payload CARRIERS for a span (their body is folded
 * onto the matching model/tool span elsewhere), not standalone lifecycle
 * signals. Projecting them as `engine.event` rows would duplicate the
 * model/tool span — so they're skipped. Mirrors `ENGINE_EVENT_CARRIER_KINDS`
 * in `api/agent-runs.ts` so the unified path drops exactly what the raw path
 * drops. WS-8 Part 2.
 */
const ENGINE_EVENT_CARRIER_KINDS: ReadonlySet<string> = new Set([
  "model_call_payload",
  "tool_call_payload",
]);

/**
 * Project ONE engine-event-shaped signal onto an `engine.event` SpanProjection,
 * carrying the raw kind in `attributes.engine_event_kind` so the render layer
 * (`span-colors.ts` / `engine-event-kinds.ts`) resolves its family/label —
 * EXACTLY the row the raw path's `engineEventFrameToSpan` (api/agent-runs.ts)
 * produces. Returns the unchanged `spans` for carrier/kindless signals (which
 * the raw path also drops), so the two paths stay byte-equivalent. WS-8 Part 2.
 *
 * `engineKind` is the `EngineEvent.kind` (e.g. `risk_veto`, `memory_recall`).
 * `scopeSpanId` is the event's scoping span (nests the row under the
 * decision/model/broker it fired against); a run-scoped event (`null`) becomes
 * a top-level lifecycle row — never dropped. `payload` is the optional inspector
 * body (parsed JSON or a structured value), preserved verbatim so the dock shows
 * what the operator sees on the raw path.
 */
function projectEngineEventRow(
  spans: SpanProjection[],
  ev: UnifiedEvent,
  engineKind: string,
  scopeSpanId: string | null,
  payload: unknown,
): SpanProjection[] {
  if (!engineKind || ENGINE_EVENT_CARRIER_KINDS.has(engineKind)) return spans;
  const attributes: Record<string, unknown> = { engine_event_kind: engineKind };
  if (payload !== undefined && payload !== null) {
    attributes.engine_event_payload = payload;
  }
  // Synthetic id mirroring the raw live path
  // (`engine_event:<kind>:<createdAt>:<parent>`) so replays/reconnects dedupe
  // a re-delivered engine event onto the same row rather than duplicating it.
  const spanId = `engine_event:${engineKind}:${ev.ts}:${scopeSpanId ?? "run"}`;
  const idx = spans.findIndex((s) => s.spanId === spanId);
  if (idx !== -1) return spans;
  return [
    ...spans,
    {
      spanId,
      runId: ev.run_id ?? null,
      parentSpanId: scopeSpanId,
      kind: "engine.event",
      name: engineKind,
      startedAt: ev.ts,
      // Engine events are point-in-time signals, not bracketed intervals —
      // the tree renders them as zero-duration rows (matching the raw path).
      finishedAt: ev.ts,
      status: "ok",
      attributes,
    },
  ];
}

/** Parse a `payload_json` STRING into a value, falling back to the raw string
 * when it isn't valid JSON — mirrors the raw live path's lenient handling so no
 * inspector body is lost. */
function parseEnginePayloadJson(raw: string | null | undefined): unknown {
  if (typeof raw !== "string" || raw.length === 0) return undefined;
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

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
        attributes: seed?.attributes ?? {},
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
    // ── Engine lifecycle signals (WS-8 Part 2 — convergence parity) ────────
    // These are the rows WS-8 Part 1 made first-class on the raw dock path.
    // The unified projection must produce the SAME `engine.event` rows so a
    // future stream flip never makes the panel go dark.
    case "engine_event":
      // The engine event's own `span_id` is its scoping span (the raw path
      // uses the same field); a run-scoped event has `span_id: null` and
      // becomes a top-level lifecycle row.
      return projectEngineEventRow(
        spans,
        ev,
        p.data.kind,
        p.data.span_id ?? null,
        parseEnginePayloadJson(p.data.payload_json),
      );
    case "memory_recall":
    case "memory_write":
      // The dedicated unified memory payloads map onto the SAME `engine.event`
      // row the raw EXPORT path surfaces — the recorder writes these to the
      // `events` table with `kind = "memory_recall"|"memory_write"` and
      // `span_id NULL`, so they project as run-scoped engine-event rows whose
      // family resolves to `memory` off `engine_event_kind`. The structured
      // payload is preserved as the inspector body.
      return projectEngineEventRow(spans, ev, p.kind, null, p.data);
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
