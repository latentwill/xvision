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
import type { BrokerCallDetail } from "@/api/types-agent-runs";
import { decisionIdxFromIdempotencyKey } from "@/features/agent-runs/decision-idx";
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
  // ── Inspector fidelity (WS-8 Part 2 Part B) ──────────────────────────────
  //
  // The raw agent-run dock reaches full inspector detail by refetching the
  // canonical `AgentRunExport` (model bodies/tokens/cost/hashes, broker fill,
  // tool I/O, decision index, error). The unified projection must populate the
  // SAME fields off the rich `UnifiedPayload` variants — which wrap the exact
  // event structs the raw path uses — so a future LIVE-wire flip onto the one
  // envelope renders identical inspector content (the never-go-dark contract).
  // All optional: a span that never saw a terminal model/tool/broker event
  // leaves them undefined, exactly like the raw path.
  /** Model: provider id (from `model_call_finished`). */
  provider?: string;
  /** Model: model id. */
  model?: string;
  /** Model: prompt token count. */
  tokensIn?: number;
  /** Model: completion token count. */
  tokensOut?: number;
  /** Model: cost in USD. */
  cost?: number;
  /** Model: `prompt_hash` (surfaced as `RunSpan.hash`). */
  promptHash?: string;
  /** Model: `response_hash`. */
  responseHash?: string;
  /** Model: full plaintext prompt body. */
  prompt?: string;
  /** Model: full plaintext response body. */
  response?: string;
  /** Model: blob-store ref for the prompt body. */
  promptPayloadRef?: string;
  /** Model: blob-store ref for the response body. */
  responsePayloadRef?: string;
  /** Tool: parsed input args (from `tool_requested.input_text`). */
  args?: unknown;
  /** Tool: parsed output result (from `tool_finished.output_text`). */
  result?: unknown;
  /** Broker: full fill/reject detail folded from started+finished events. */
  brokerCall?: BrokerCallDetail;
  /** Per-decision-cycle index (parsed off the broker idempotency key). */
  decisionIdx?: number;
  /** Human-readable error message (from `tool_failed` / `span_finished` error). */
  errorMessage?: string;
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
 * Parse a tool input/output `_text` body for the inspector's TOOL ARGS /
 * TOOL RESULT pull-quotes. The recorder ships these as JSON strings (args
 * object / result object); we parse to a value so the inspector renders the
 * structured form, falling back to the raw string for non-JSON bodies so a
 * plain-text tool output is still shown verbatim. `undefined` for empty/absent
 * bodies → the inspector elides the section.
 */
function parseToolText(raw: string | null | undefined): unknown {
  if (typeof raw !== "string" || raw.length === 0) return undefined;
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

/**
 * Extract a human-readable error message from an observability `error_json`
 * payload. Mirrors `parseErrorJson` in `api/agent-runs.ts` (the raw path) so
 * the unified projection surfaces the SAME message: the recorder writes
 * `JSON.stringify({ message })` for `ToolCallFailed` + `SpanFinished(Error)`,
 * but some emitters write a bare string. Accept either; fall back to the raw
 * string when JSON parse fails. `undefined` for null/empty so the field drops.
 */
function parseErrorJson(raw: string | null | undefined): string | undefined {
  if (typeof raw !== "string" || raw.length === 0) return undefined;
  try {
    const parsed = JSON.parse(raw);
    if (typeof parsed === "string") return parsed;
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      const obj = parsed as Record<string, unknown>;
      const msg = obj.message ?? obj.error ?? obj.detail;
      if (typeof msg === "string" && msg.length > 0) return msg;
    }
  } catch {
    // Bare string — fall through to the raw value.
  }
  return raw;
}

/** Coerce a `number | null | undefined` wire field to `number | undefined`. */
function num(v: number | null | undefined): number | undefined {
  return typeof v === "number" && Number.isFinite(v) ? v : undefined;
}

/** Coerce a `string | null | undefined` wire field to `string | undefined`
 * (drops empty so the inspector elides the field). */
function str(v: string | null | undefined): string | undefined {
  return typeof v === "string" && v.length > 0 ? v : undefined;
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
    case "span_finished": {
      // Carry a human-readable error message off the span's `error_json`
      // (qa-trace-error-surfacing parity) so a failed span shows WHY it failed
      // in the inspector instead of a bare "error" status.
      const errorMessage = parseErrorJson(p.data.error_json);
      return upsert(p.data.span_id, (s) => ({
        ...s,
        finishedAt: ev.ts,
        status:
          p.data.status === "error"
            ? "error"
            : p.data.status === "cancelled"
              ? "cancelled"
              : "ok",
        ...(errorMessage ? { errorMessage } : {}),
      }));
    }
    case "model_call_finished": {
      // Fold the per-call provider/model/tokens/cost/hashes/bodies onto the
      // model span — the SAME fields `normalizeAgentRunExport` projects from
      // the `model_calls[]` table onto a `model.call`/`decision.model` span.
      const d = p.data;
      return upsert(d.span_id, (s) => ({
        ...s,
        status: s.status === "in_progress" ? "ok" : s.status,
        finishedAt: s.finishedAt ?? ev.ts,
        provider: str(d.provider) ?? s.provider,
        model: str(d.model) ?? s.model,
        tokensIn: num(d.input_token_count) ?? s.tokensIn,
        tokensOut: num(d.output_token_count) ?? s.tokensOut,
        cost: num(d.cost_usd) ?? s.cost,
        promptHash: str(d.prompt_hash) ?? s.promptHash,
        responseHash: str(d.response_hash) ?? s.responseHash,
        prompt: str(d.prompt_text) ?? s.prompt,
        response: str(d.response_text) ?? s.response,
        promptPayloadRef: str(d.prompt_payload_ref) ?? s.promptPayloadRef,
        responsePayloadRef: str(d.response_payload_ref) ?? s.responsePayloadRef,
      }));
    }
    case "tool_finished": {
      // Surface the tool output body (TOOL RESULT pull-quote) when retained.
      const result = parseToolText(p.data.output_text);
      return upsert(p.data.span_id, (s) => ({
        ...s,
        status: s.status === "in_progress" ? "ok" : s.status,
        finishedAt: s.finishedAt ?? ev.ts,
        ...(result !== undefined ? { result } : {}),
      }));
    }
    case "broker_call_finished": {
      // Merge the fill/reject detail onto the broker span's reconstructed
      // `brokerCall` (started carried side/symbol/qty/…; finished carries the
      // outcome/fill/fee/error) — the SAME shape the raw export folds onto
      // `attributes_json.broker_call` and projects to `RunSpan.broker_call`.
      const d = p.data;
      return upsert(d.span_id, (s) => {
        const prev = s.brokerCall;
        const brokerCall: BrokerCallDetail = {
          // Defaults mirror `extractBrokerCall` in api/agent-runs.ts so a
          // finished-before-started ordering still yields a coherent row.
          side: prev?.side ?? "buy",
          symbol: prev?.symbol ?? "",
          qty: prev?.qty ?? 0,
          intended_price: prev?.intended_price ?? null,
          order_type: prev?.order_type ?? "market",
          venue: prev?.venue ?? "unknown",
          idempotency_key: prev?.idempotency_key ?? null,
          outcome: (d.outcome as BrokerCallDetail["outcome"]) ?? prev?.outcome ?? null,
          fill_price: num(d.fill_price) ?? null,
          fill_qty: num(d.fill_qty) ?? null,
          fee: num(d.fee) ?? null,
          broker_order_id: str(d.broker_order_id) ?? null,
          error_class: str(d.error_class) ?? null,
          error_message: str(d.error_message) ?? null,
          severity:
            d.severity === "warn" || d.severity === "error" ? d.severity : null,
        };
        return {
          ...s,
          status: s.status === "in_progress" ? "ok" : s.status,
          finishedAt: s.finishedAt ?? ev.ts,
          brokerCall,
        };
      }, { kind: "broker.call" });
    }
    case "tool_failed": {
      const errorMessage = parseErrorJson(p.data.error_json);
      return upsert(p.data.span_id, (s) => ({
        ...s,
        status: s.status === "in_progress" ? "error" : s.status,
        finishedAt: s.finishedAt ?? ev.ts,
        ...(errorMessage ? { errorMessage } : {}),
      }));
    }
    case "tool_cancelled":
      return close(p.data.span_id, "cancelled");
    case "tool_requested": {
      // Surface the tool input args (TOOL ARGS pull-quote) when retained.
      const args = parseToolText(p.data.input_text);
      return upsert(p.data.span_id, (s) => ({
        ...s,
        ...(args !== undefined ? { args } : {}),
      }));
    }
    case "tool_started":
      return upsert(p.data.span_id, (s) => s);
    case "broker_call_started": {
      // Seed the broker span's `brokerCall` from the submit detail + parse the
      // per-decision-cycle index off the `<run_id>-<decision_idx>` idempotency
      // key (the SAME carrier `decisionIdxFromAttributes` reads on the raw path).
      const d = p.data;
      const decisionIdx = decisionIdxFromIdempotencyKey(d.idempotency_key);
      return upsert(d.span_id, (s) => {
        const prev = s.brokerCall;
        const brokerCall: BrokerCallDetail = {
          side: (d.side as BrokerCallDetail["side"]) ?? prev?.side ?? "buy",
          symbol: str(d.symbol) ?? prev?.symbol ?? "",
          qty: num(d.qty) ?? prev?.qty ?? 0,
          intended_price: num(d.intended_price) ?? prev?.intended_price ?? null,
          order_type: str(d.order_type) ?? prev?.order_type ?? "market",
          venue: str(d.venue) ?? prev?.venue ?? "unknown",
          idempotency_key: str(d.idempotency_key) ?? prev?.idempotency_key ?? null,
          outcome: prev?.outcome ?? null,
          fill_price: prev?.fill_price ?? null,
          fill_qty: prev?.fill_qty ?? null,
          fee: prev?.fee ?? null,
          broker_order_id: prev?.broker_order_id ?? null,
          error_class: prev?.error_class ?? null,
          error_message: prev?.error_message ?? null,
          severity: prev?.severity ?? null,
        };
        return {
          ...s,
          brokerCall,
          ...(decisionIdx != null ? { decisionIdx } : {}),
        };
      }, { kind: "broker.call" });
    }
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
