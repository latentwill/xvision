// frontend/web/src/stores/unified-trace-parity.test.ts
//
// WS-8 Part 2 — UnifiedEvent convergence PARITY GUARD.
//
// The operator decided on full convergence (the agent-runs SSE should emit
// `UnifiedEvent` frames via `RunEventProjector` and the dock should project
// from the unified envelope), BUT with a hard guardrail: the panel must NEVER
// go dark. Before retiring the raw-`RunEvent` dock path we must PROVE the
// unified projection renders the SAME rows.
//
// This test is the proof — and it currently proves the OPPOSITE: the existing
// `UnifiedEvent → SpanProjection` projection in `session-events.ts` is
// strictly LESS complete than the raw-RunEvent dock path. It silently drops
// engine events (risk veto / regime transition / order state / filter fired)
// and memory rows — the very rows WS-8 Part 1 just made first-class on the raw
// path. Switching the agent-runs stream onto it today would make those rows
// vanish, violating the never-go-dark requirement.
//
// So Part 2 stays DEFERRED. This test documents the blocker, and guards
// against a premature half-switch: it must keep passing (i.e. the gap must be
// closed in `projectSpan` BEFORE the stream is switched). When someone teaches
// `projectSpan` to project `engine_event` / `memory_recall` / `memory_write`
// onto rows, flip the `expect`s and wire the stream.

import { beforeEach, describe, expect, test } from "vitest";
import { useSessionEvents } from "./session-events";
import type { UnifiedEvent, UnifiedPayload } from "@/api/unified-events";

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

describe("WS-8 Part 2 — unified projection parity guard (convergence blocker)", () => {
  beforeEach(() => {
    useSessionEvents.setState({ sessions: {} });
  });

  test("span_started / span_finished DO project (the convergence-ready subset)", () => {
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

  // ── THE BLOCKER ──────────────────────────────────────────────────────────
  // These rows render on the RAW dock path (WS-8 Part 1). They are DROPPED by
  // the unified projection — hence the stream cannot be switched yet.

  test("BLOCKER: engine_event is DROPPED by the unified projection (raw path renders it)", () => {
    const store = useSessionEvents.getState();
    store.ingest(
      SESSION,
      ev(0, {
        kind: "engine_event",
        data: {
          run_id: "run_parity",
          span_id: null,
          kind: "risk_veto",
          payload_json: JSON.stringify({ reason: "max_drawdown" }),
          created_at: "2026-06-14T10:00:00.000Z",
        },
      } as UnifiedPayload),
    );
    const spans = useSessionEvents.getState().spansFor(SESSION);
    // No engine.event row is produced — the gap that blocks convergence.
    expect(spans.some((s) => s.kind === "engine.event")).toBe(false);
    expect(spans).toHaveLength(0);
  });

  test("BLOCKER: memory_recall / memory_write are DROPPED by the unified projection", () => {
    const store = useSessionEvents.getState();
    store.ingest(
      SESSION,
      ev(0, {
        kind: "memory_recall",
        data: {
          run_id: "run_parity",
          decision_id: 1,
          namespace: "trades",
          items: [],
        },
      } as UnifiedPayload),
    );
    const spans = useSessionEvents.getState().spansFor(SESSION);
    expect(spans).toHaveLength(0);
  });
});
