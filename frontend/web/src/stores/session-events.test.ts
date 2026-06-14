// frontend/web/src/stores/session-events.test.ts
//
// The session-events store is the ONE source of truth for a chat-rail
// session's unified event log. These tests assert the Phase 1.2/1.4
// contract: a single `UnifiedEvent` sequence ingested ONCE produces BOTH a
// rail tool row (via `reduceRows`) AND a trace-dock span projection — one
// source, two projections — and that ingestion is idempotent (same
// event_id twice = one row / one span).

import { beforeEach, describe, expect, it } from "vitest";

import type { UnifiedEvent, UnifiedPayload } from "@/api/unified-events";
import { useSessionEvents } from "./session-events";
import type { ToolRow } from "./message-row-reducer";

// ─── Event builder (mirrors message-row-reducer.test.ts) ────────────────────

function ev(
  payload: UnifiedPayload,
  overrides: Partial<UnifiedEvent> = {},
): UnifiedEvent {
  const seq = overrides.seq ?? 0;
  return {
    event_id: overrides.event_id ?? `ev_${seq}`,
    session_id: overrides.session_id ?? "sess_1",
    run_id: overrides.run_id ?? "run_1",
    span_id: overrides.span_id ?? null,
    parent_event_id: null,
    seq,
    ts: overrides.ts ?? "2026-05-24T12:00:00Z",
    scope: { kind: "strategy", id: "strat_abc" },
    actor: overrides.actor ?? "agent",
    source: overrides.source ?? "chat_rail",
    blob_hash: null,
    payload,
  };
}

function resetStore() {
  useSessionEvents.setState({ sessions: {} });
}

// A single tool lifecycle, plus a span_started/finished pair so the dock
// projection has something to render alongside the rail tool row.
const SESSION = "sess_tool";
const SPAN = "sp_tool";

function toolLifecycle(): UnifiedEvent[] {
  return [
    ev(
      {
        kind: "span_started",
        data: {
          span_id: SPAN,
          run_id: "run_1",
          parent_span_id: null,
          kind: "tool.call",
          name: "create_strategy",
          started_at: "2026-05-24T12:00:00Z",
          otel_trace_id: null,
          otel_span_id: null,
          attributes_json: null,
        },
      },
      { event_id: "lc_span_start", seq: 0, span_id: SPAN, session_id: SESSION },
    ),
    ev(
      {
        kind: "tool_requested",
        data: {
          span_id: SPAN,
          tool_name: "create_strategy",
          origin: "Native",
          tool_version: null,
          tool_hash: null,
          side_effect_level: "external_write",
          risk_level: "strategy_mutation",
          requires_approval: true,
          is_run_terminator: false,
          input_hash: "h",
          input_payload_ref: null,
        },
      },
      { event_id: "lc_req", seq: 1, span_id: SPAN, session_id: SESSION },
    ),
    ev(
      { kind: "tool_started", data: { span_id: SPAN } },
      { event_id: "lc_started", seq: 2, span_id: SPAN, session_id: SESSION },
    ),
    ev(
      {
        kind: "tool_finished",
        data: {
          span_id: SPAN,
          output_hash: "oh",
          output_payload_ref: null,
          exit_code: 0,
        },
      },
      { event_id: "lc_fin", seq: 3, span_id: SPAN, session_id: SESSION },
    ),
    ev(
      {
        kind: "span_finished",
        data: { span_id: SPAN, ended_at: "2026-05-24T12:00:01Z", status: "ok", error_json: null },
      },
      { event_id: "lc_span_fin", seq: 4, span_id: SPAN, session_id: SESSION },
    ),
  ];
}

describe("session-events store — one source, two projections", () => {
  beforeEach(resetStore);

  it("a tool lifecycle ingested ONCE yields a rail tool row AND a dock span", () => {
    const store = useSessionEvents.getState();
    for (const e of toolLifecycle()) store.ingest(SESSION, e);

    // ── Projection 1: rail rows (via reduceRows) ──
    const rows = useSessionEvents.getState().rowsFor(SESSION);
    const toolRows = rows.filter((r): r is ToolRow => r.type === "tool");
    expect(toolRows).toHaveLength(1);
    const t = toolRows[0];
    expect(t.spanId).toBe(SPAN);
    expect(t.id).toBe(`tool:${SPAN}`);
    expect(t.toolName).toBe("create_strategy");
    expect(t.status).toBe("finished");
    expect(t.outputHash).toBe("oh");

    // ── Projection 2: dock spans (same log, no second ingestion) ──
    const spans = useSessionEvents.getState().spansFor(SESSION);
    expect(spans).toHaveLength(1);
    const sp = spans[0];
    expect(sp.spanId).toBe(SPAN);
    expect(sp.kind).toBe("tool.call");
    expect(sp.name).toBe("create_strategy");
    expect(sp.status).toBe("ok");
    expect(sp.finishedAt).not.toBeNull();

    // The raw log holds exactly the five events, ordered by seq.
    const events = useSessionEvents.getState().sessions[SESSION]!.events;
    expect(events.map((e) => e.seq)).toEqual([0, 1, 2, 3, 4]);
    expect(useSessionEvents.getState().lastSeqFor(SESSION)).toBe(4);
  });

  it("is idempotent: re-ingesting the same event_id leaves one row + one span", () => {
    const store = useSessionEvents.getState();
    const events = toolLifecycle();
    for (const e of events) store.ingest(SESSION, e);
    // Replay the WHOLE lifecycle a second time (reconnect / resume scenario).
    for (const e of events) store.ingest(SESSION, e);

    const rows = useSessionEvents.getState().rowsFor(SESSION);
    expect(rows.filter((r) => r.type === "tool")).toHaveLength(1);

    const spans = useSessionEvents.getState().spansFor(SESSION);
    expect(spans).toHaveLength(1);

    // The event log did not grow on replay.
    expect(useSessionEvents.getState().sessions[SESSION]!.events).toHaveLength(5);
  });

  it("a single duplicate event_id (same span lifecycle) is a no-op the second time", () => {
    const store = useSessionEvents.getState();
    const e = ev(
      {
        kind: "tool_requested",
        data: {
          span_id: "sp_dup",
          tool_name: "fetch",
          origin: "Native",
          tool_version: null,
          tool_hash: null,
          side_effect_level: "read_only",
          risk_level: "safe_read",
          requires_approval: false,
          is_run_terminator: false,
          input_hash: "h",
          input_payload_ref: null,
        },
      },
      { event_id: "dup_1", seq: 0, span_id: "sp_dup", session_id: "sess_dup" },
    );
    store.ingest("sess_dup", e);
    const after1 = useSessionEvents.getState().sessions["sess_dup"]!;
    store.ingest("sess_dup", e);
    const after2 = useSessionEvents.getState().sessions["sess_dup"]!;

    // Reference-stable: the dedupe gate short-circuits before any mutation.
    expect(after2).toBe(after1);
    expect(after2.events).toHaveLength(1);
    expect(after2.rows.filter((r) => r.type === "tool")).toHaveLength(1);
  });

  it("ingestMany folds a replay backfill in one update, deduping by event_id", () => {
    const store = useSessionEvents.getState();
    const events = toolLifecycle();
    store.ingestMany(SESSION, events);
    // A backfill that overlaps the already-ingested set must not duplicate.
    store.ingestMany(SESSION, events);

    expect(useSessionEvents.getState().rowsFor(SESSION).filter((r) => r.type === "tool")).toHaveLength(1);
    expect(useSessionEvents.getState().spansFor(SESSION)).toHaveLength(1);
    expect(useSessionEvents.getState().sessions[SESSION]!.events).toHaveLength(5);
  });

  it("orders out-of-order events by seq in the raw log", () => {
    const store = useSessionEvents.getState();
    store.ingest("sess_ooo", ev({ kind: "assistant_token_delta", data: { text: "b" } }, { event_id: "ooo_2", seq: 2, session_id: "sess_ooo" }));
    store.ingest("sess_ooo", ev({ kind: "assistant_token_delta", data: { text: "a" } }, { event_id: "ooo_0", seq: 0, session_id: "sess_ooo" }));
    store.ingest("sess_ooo", ev({ kind: "assistant_token_delta", data: { text: "ab" } }, { event_id: "ooo_1", seq: 1, session_id: "sess_ooo" }));

    const seqs = useSessionEvents.getState().sessions["sess_ooo"]!.events.map((e) => e.seq);
    expect(seqs).toEqual([0, 1, 2]);
  });

  it("an engine_event in a chat session projects an engine.event dock row (WS-8 Part 2) without disturbing the tool lifecycle", () => {
    const store = useSessionEvents.getState();
    // The full tool lifecycle (the existing rail path) …
    for (const e of toolLifecycle()) store.ingest(SESSION, e);
    // … plus a run-scoped engine event interleaved into the SAME log.
    store.ingest(
      SESSION,
      ev(
        {
          kind: "engine_event",
          data: {
            run_id: "run_1",
            span_id: null,
            kind: "risk_veto",
            payload_json: JSON.stringify({ reason: "max_drawdown" }),
            created_at: "2026-05-24T12:00:02Z",
          },
        },
        { event_id: "ee_risk", seq: 5, session_id: SESSION },
      ),
    );

    const spans = useSessionEvents.getState().spansFor(SESSION);
    // The tool span is untouched (no rail regression).
    const tool = spans.find((s) => s.spanId === SPAN);
    expect(tool?.kind).toBe("tool.call");
    expect(tool?.status).toBe("ok");
    // The engine event is now a first-class engine.event dock row carrying the
    // kind + payload the render layer resolves a family/label off.
    const engine = spans.filter((s) => s.kind === "engine.event");
    expect(engine).toHaveLength(1);
    expect(engine[0]!.attributes.engine_event_kind).toBe("risk_veto");
    expect(engine[0]!.attributes.engine_event_payload).toEqual({ reason: "max_drawdown" });
    expect(engine[0]!.parentSpanId).toBeNull(); // run-scoped ⇒ top-level row

    // The rail rows are unchanged: still exactly one tool row (engine events
    // are not rail rows — they pass through the reducer untouched).
    const toolRows = useSessionEvents.getState().rowsFor(SESSION).filter((r) => r.type === "tool");
    expect(toolRows).toHaveLength(1);
  });

  it("a re-delivered engine_event (reconnect/replay) dedupes onto the same dock row", () => {
    const store = useSessionEvents.getState();
    const engineEv = ev(
      {
        kind: "engine_event",
        data: {
          run_id: "run_1",
          span_id: "sp_decide",
          kind: "regime_transition",
          payload_json: null,
          created_at: "2026-05-24T12:00:03Z",
        },
      },
      { event_id: "ee_regime", seq: 0, span_id: "sp_decide", session_id: "sess_ee" },
    );
    store.ingest("sess_ee", engineEv);
    // Same event_id re-ingested (dedupe gate) AND a distinct event_id carrying
    // the SAME engine event (e.g. snapshot + live tail overlap) — neither must
    // produce a duplicate engine.event row.
    store.ingest("sess_ee", engineEv);
    store.ingest("sess_ee", { ...engineEv, event_id: "ee_regime_dup", seq: 1 });

    const engine = useSessionEvents
      .getState()
      .spansFor("sess_ee")
      .filter((s) => s.kind === "engine.event");
    expect(engine).toHaveLength(1);
    expect(engine[0]!.parentSpanId).toBe("sp_decide");
  });

  it("reset clears a session's log without touching siblings", () => {
    const store = useSessionEvents.getState();
    for (const e of toolLifecycle()) store.ingest(SESSION, e);
    store.ingest("sess_other", ev({ kind: "assistant_message_started" }, { event_id: "other_0", seq: 0, session_id: "sess_other" }));

    store.reset(SESSION);
    expect(useSessionEvents.getState().sessions[SESSION]).toBeUndefined();
    expect(useSessionEvents.getState().rowsFor(SESSION)).toEqual([]);
    expect(useSessionEvents.getState().spansFor(SESSION)).toEqual([]);
    // Sibling session untouched.
    expect(useSessionEvents.getState().sessions["sess_other"]).toBeDefined();
  });
});
