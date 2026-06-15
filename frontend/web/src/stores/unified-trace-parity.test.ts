// frontend/web/src/stores/unified-trace-parity.test.ts
//
// WS-8 Part 2 — UnifiedEvent convergence PARITY PROOF.
//
// The operator decided on full convergence: the dock should consume ONE
// `UnifiedEvent` envelope, projected through `session-events.ts::projectSpan`,
// instead of the raw-`RunEvent` dock path in `api/agent-runs.ts`. The hard
// guardrail is the NEVER-GO-DARK contract: the unified projection must render
// the SAME rows the raw path renders for the same logical events — otherwise
// switching the stream would re-drop the exact rows WS-8 Part 1 just rescued
// (engine lifecycle: risk veto / regime transition / order state / memory …)
// and the panel would go dark.
//
// This file USED to prove the OPPOSITE (that engine_event / memory_recall /
// memory_write were DROPPED by the unified projection). Part 2 closed that gap:
// `projectSpan` now projects those payloads onto the SAME `engine.event` row
// shape the raw path produces. This test is the PROOF — for a representative
// `UnifiedEvent` stream covering span lifecycle + model + tool + broker + a
// spread of engine kinds (risk_veto / regime_transition / position_exit) +
// memory_recall / memory_write, the unified projection yields the SAME set of
// trace rows (kind + scoping + label-resolvable family) the raw path yields.
//
// "Same row" parity is asserted against the raw path's CANONICAL single-event
// projector, `engineEventFrameToSpan` (api/agent-runs.ts) — the function the
// live dock uses to turn one engine event into one `engine.event` RunSpan.
// Both paths funnel through `engine.event` + `attributes.engine_event_kind`,
// which the render layer (span-colors / engine-event-kinds / SpanTree) resolves
// to a family badge. Parity here ⇒ identical rendered rows ⇒ a future stream
// flip is provably safe (no panel-goes-dark regression).

import { beforeEach, describe, expect, test } from "vitest";
import { useSessionEvents, type SpanProjection } from "./session-events";
import type { UnifiedEvent, UnifiedPayload } from "@/api/unified-events";
import { engineEventFrameToSpan } from "@/api/agent-runs";
import {
  engineEventFamilyOf,
  engineEventLabelOf,
} from "@/features/agent-runs/engine-event-kinds";

const SESSION = "sess_parity";

function ev(seq: number, payload: UnifiedPayload, over: Partial<UnifiedEvent> = {}): UnifiedEvent {
  return {
    event_id: `ev_${seq}`,
    session_id: SESSION,
    run_id: "run_parity",
    span_id: null,
    parent_event_id: null,
    seq,
    ts: `2026-06-14T10:00:0${seq}.000Z`,
    scope: { kind: "run", id: "run_parity" },
    actor: "system",
    source: "agent_run",
    blob_hash: null,
    payload,
    ...over,
  };
}

/** The `engine.event` rows the unified projection emitted, for assertions. */
function engineRows(spans: SpanProjection[]): SpanProjection[] {
  return spans.filter((s) => s.kind === "engine.event");
}

/** Read the carried engine kind off a projected `engine.event` row. */
function engineKindOf(s: SpanProjection): string | null {
  const v = (s.attributes ?? {})["engine_event_kind"];
  return typeof v === "string" && v.length > 0 ? v : null;
}

describe("WS-8 Part 2 — unified projection parity proof (convergence-ready)", () => {
  beforeEach(() => {
    useSessionEvents.setState({ sessions: {} });
  });

  test("span_started / span_finished DO project (the lifecycle subset)", () => {
    const store = useSessionEvents.getState();
    store.ingest(
      SESSION,
      ev(0, {
        kind: "span_started",
        data: {
          span_id: "sp_model",
          run_id: "run_parity",
          parent_span_id: null,
          kind: "decision.model",
          name: "claude-opus",
          started_at: "2026-06-14T10:00:00.000Z",
        },
      } as UnifiedPayload),
    );
    const spans = useSessionEvents.getState().spansFor(SESSION);
    expect(spans.find((s) => s.spanId === "sp_model")?.kind).toBe("decision.model");
  });

  // ── PARITY: engine_event ───────────────────────────────────────────────
  // The unified `engine_event` payload must project onto the SAME engine.event
  // row the raw path's `engineEventFrameToSpan` produces for the same data.

  test("engine_event projects onto an engine.event row matching the raw path", () => {
    const store = useSessionEvents.getState();
    const data = {
      run_id: "run_parity",
      span_id: "sp_decide",
      kind: "risk_veto",
      payload_json: JSON.stringify({ reason: "max_drawdown" }),
      created_at: "2026-06-14T10:00:00.000Z",
    };
    // Seed the scoping span so parent linkage is exercised on both paths.
    store.ingest(
      SESSION,
      ev(0, {
        kind: "span_started",
        data: {
          span_id: "sp_decide",
          run_id: "run_parity",
          parent_span_id: null,
          kind: "agent.decision",
          name: "decision",
          started_at: "2026-06-14T09:59:59.000Z",
        },
      } as UnifiedPayload),
    );
    store.ingest(
      SESSION,
      ev(1, { kind: "engine_event", data } as UnifiedPayload, { span_id: "sp_decide" }),
    );

    const rows = engineRows(useSessionEvents.getState().spansFor(SESSION));
    expect(rows).toHaveLength(1);
    const unified = rows[0]!;

    // The raw path's canonical single-event projection of the SAME data.
    const raw = engineEventFrameToSpan(data)!;
    expect(raw).not.toBeNull();

    // Same kind, same carried engine kind, same scoping, same label-resolvable
    // family → identical rendered row.
    expect(unified.kind).toBe(raw.kind); // "engine.event"
    expect(engineKindOf(unified)).toBe(raw.attributes.engine_event_kind);
    expect(engineKindOf(unified)).toBe("risk_veto");
    expect(unified.parentSpanId).toBe(raw.parent_span_id); // "sp_decide"
    // Payload body preserved (the dock shows it in the inspector).
    expect(unified.attributes.engine_event_payload).toEqual({ reason: "max_drawdown" });
    // The render layer resolves family/label off engine_event_kind — non-fallback.
    expect(engineEventFamilyOf(engineKindOf(unified)!)).toBe("risk");
    expect(engineEventLabelOf(engineKindOf(unified)!)).toBe("Risk veto");
  });

  test("carrier engine kinds (model/tool payload) are skipped on both paths", () => {
    const store = useSessionEvents.getState();
    for (const carrier of ["model_call_payload", "tool_call_payload"]) {
      store.ingest(
        SESSION,
        ev(0, {
          kind: "engine_event",
          data: {
            run_id: "run_parity",
            span_id: "sp_model",
            kind: carrier,
            payload_json: "{}",
            created_at: "2026-06-14T10:00:00.000Z",
          },
        } as UnifiedPayload),
      );
      // Raw path drops carriers too (engineEventFrameToSpan consumes only
      // span_id / kind / payload_json / created_at — no run_id field).
      expect(
        engineEventFrameToSpan({
          span_id: "sp_model",
          kind: carrier,
          payload_json: "{}",
          created_at: "2026-06-14T10:00:00.000Z",
        }),
      ).toBeNull();
    }
    expect(engineRows(useSessionEvents.getState().spansFor(SESSION))).toHaveLength(0);
  });

  // ── PARITY: memory_recall / memory_write ───────────────────────────────
  // The dedicated unified memory payloads must project onto engine.event rows
  // carrying the registry kind (memory_recall / memory_write) — the SAME way
  // the raw export path surfaces them (recorded as EngineEvent rows).

  test("memory_recall / memory_write project onto memory-family engine.event rows", () => {
    const store = useSessionEvents.getState();
    store.ingest(
      SESSION,
      ev(0, {
        kind: "memory_recall",
        data: { run_id: "run_parity", decision_id: 1, namespace: "trades", items: [] },
      } as UnifiedPayload),
    );
    store.ingest(
      SESSION,
      ev(1, {
        kind: "memory_write",
        data: {
          run_id: "run_parity",
          decision_id: 1,
          namespace: "trades",
          memory_item_id: "mem_1",
          text_preview: "bought BTC",
        },
      } as UnifiedPayload),
    );

    const rows = engineRows(useSessionEvents.getState().spansFor(SESSION));
    expect(rows).toHaveLength(2);
    const kinds = rows.map(engineKindOf).sort();
    expect(kinds).toEqual(["memory_recall", "memory_write"]);
    // Both resolve to the memory family (matching the raw export path's
    // engine.event rows whose engine_event_kind is memory_recall/write).
    for (const r of rows) {
      expect(r.kind).toBe("engine.event");
      expect(engineEventFamilyOf(engineKindOf(r)!)).toBe("memory");
    }
  });

  // ── THE NEVER-GO-DARK CONTRACT ─────────────────────────────────────────
  // A representative mixed stream → the SAME set of trace rows on both paths.

  test("a representative mixed stream yields the SAME engine rows as the raw path", () => {
    const store = useSessionEvents.getState();

    // Scoping spans first (model + tool + broker + decision) so engine events
    // can nest, exactly as a real run interleaves them.
    const lifecycle: UnifiedPayload[] = [
      {
        kind: "span_started",
        data: {
          span_id: "sp_decide",
          run_id: "run_parity",
          parent_span_id: null,
          kind: "agent.decision",
          name: "decision",
          started_at: "2026-06-14T10:00:00.000Z",
        },
      } as UnifiedPayload,
      {
        kind: "span_started",
        data: {
          span_id: "sp_model",
          run_id: "run_parity",
          parent_span_id: "sp_decide",
          kind: "decision.model",
          name: "claude-opus",
          started_at: "2026-06-14T10:00:01.000Z",
        },
      } as UnifiedPayload,
      { kind: "model_call_finished", data: { span_id: "sp_model" } } as unknown as UnifiedPayload,
      {
        kind: "tool_requested",
        data: { span_id: "sp_tool" },
      } as unknown as UnifiedPayload,
      { kind: "tool_finished", data: { span_id: "sp_tool" } } as unknown as UnifiedPayload,
      {
        kind: "broker_call_started",
        data: { span_id: "sp_broker" },
      } as unknown as UnifiedPayload,
      {
        kind: "broker_call_finished",
        data: { span_id: "sp_broker" },
      } as unknown as UnifiedPayload,
    ];

    // The engine kinds named in the WS-8 convergence contract.
    const engineFrames = [
      {
        run_id: "run_parity",
        span_id: "sp_decide",
        kind: "risk_veto",
        payload_json: JSON.stringify({ reason: "max_drawdown" }),
        created_at: "2026-06-14T10:00:02.000Z",
      },
      {
        run_id: "run_parity",
        span_id: null,
        kind: "regime_transition",
        payload_json: JSON.stringify({ from: "trend", to: "chop" }),
        created_at: "2026-06-14T10:00:03.000Z",
      },
      {
        run_id: "run_parity",
        span_id: "sp_broker",
        kind: "position_exit",
        payload_json: JSON.stringify({ pnl: 12.3 }),
        created_at: "2026-06-14T10:00:04.000Z",
      },
    ];

    let seq = 0;
    for (const p of lifecycle) store.ingest(SESSION, ev(seq++, p));
    for (const f of engineFrames) {
      store.ingest(
        SESSION,
        ev(seq++, { kind: "engine_event", data: f } as UnifiedPayload, {
          span_id: f.span_id,
        }),
      );
    }
    // Two memory rows.
    store.ingest(
      SESSION,
      ev(seq++, {
        kind: "memory_recall",
        data: { run_id: "run_parity", decision_id: 1, namespace: "trades", items: [] },
      } as UnifiedPayload),
    );
    store.ingest(
      SESSION,
      ev(seq++, {
        kind: "memory_write",
        data: {
          run_id: "run_parity",
          decision_id: 1,
          namespace: "trades",
          memory_item_id: "mem_1",
          text_preview: "p",
        },
      } as UnifiedPayload),
    );

    // ── Unified path: the engine rows it produced ──
    const unifiedEngine = engineRows(useSessionEvents.getState().spansFor(SESSION));
    const unifiedKinds = unifiedEngine.map(engineKindOf).filter(Boolean).sort();

    // ── Raw path: the engine rows it would produce for the SAME events ──
    // engine_event frames → engineEventFrameToSpan; the export records the two
    // memory events as engine.event rows with kind memory_recall/memory_write.
    const rawKinds = [
      ...engineFrames
        .map((f) => engineEventFrameToSpan(f))
        .filter((s): s is NonNullable<typeof s> => s != null)
        .map((s) => s.attributes.engine_event_kind as string),
      "memory_recall",
      "memory_write",
    ].sort();

    // The never-go-dark contract: identical engine-row SET, identical kinds.
    expect(unifiedKinds).toEqual(rawKinds);
    expect(unifiedKinds).toEqual([
      "memory_recall",
      "memory_write",
      "position_exit",
      "regime_transition",
      "risk_veto",
    ]);

    // Every engine row is label-resolvable to a non-fallback family (the same
    // family the raw path's render layer resolves) → none render as "dropped".
    for (const r of unifiedEngine) {
      const k = engineKindOf(r)!;
      expect(engineEventFamilyOf(k)).not.toBe("unknown");
      expect(engineEventLabelOf(k)).not.toBe(k); // a friendly label, not raw kind
    }

    // Scoping parity: run-scoped engine events are top-level; span-scoped nest
    // under the named span — matching the raw path's parent linkage.
    const byKind = (k: string) => unifiedEngine.find((r) => engineKindOf(r) === k)!;
    expect(byKind("risk_veto").parentSpanId).toBe("sp_decide");
    expect(byKind("regime_transition").parentSpanId).toBeNull();
    expect(byKind("position_exit").parentSpanId).toBe("sp_broker");

    // The lifecycle spans still project (no regression to the existing path).
    const all = useSessionEvents.getState().spansFor(SESSION);
    expect(all.some((s) => s.spanId === "sp_model" && s.kind === "decision.model")).toBe(true);
    expect(all.some((s) => s.spanId === "sp_broker" && s.kind === "broker.call")).toBe(true);
  });
});
