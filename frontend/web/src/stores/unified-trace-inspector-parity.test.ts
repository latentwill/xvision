// frontend/web/src/stores/unified-trace-inspector-parity.test.ts
//
// WS-8 Part 2 Part B — UnifiedEvent wire convergence: FULL INSPECTOR FIDELITY
// golden parity proof (the never-go-dark contract, extended to the inspector).
//
// `unified-trace-parity.test.ts` (Part A) proved that engine lifecycle rows
// (risk_veto / regime / memory …) survive the unified projection. That is the
// row-EXISTENCE contract. This file proves the stronger row-CONTENT contract:
// the unified projection (`session-events::projectSpan` →
// `TraceDock::projectionToRunSpan`) must populate the SAME inspector fields the
// raw agent-run path renders — model body/tokens/cost/hashes, broker fill
// detail, tool args/result, decision index, error message. Anything the raw
// path surfaces that the unified projection drops would make the inspector go
// dark after the wire flip, so each such field is the blocker to surface.
//
// PARITY REFERENCE (the "raw" side): the raw LIVE dock does not rebuild
// inspector detail incrementally — on a terminal model/tool/broker frame it
// refetches the canonical `AgentRunExport` and rebuilds full-fidelity RunSpans
// through `normalizeAgentRunExport` (reached here via the exported
// `validateAgentRunDetail`). That export normalizer IS the fidelity the raw
// inspector shows. So we drive the SAME logical events through it (as a v3
// export payload) and compare its RunSpans field-for-field against the unified
// projection's RunSpans.

import { beforeEach, describe, expect, test } from "vitest";
import { useSessionEvents, type SpanProjection } from "./session-events";
import { projectionToRunSpan } from "@/features/agent-runs/TraceDock";
import { validateAgentRunDetail } from "@/api/agent-runs";
import type { RunSpan } from "@/api/types-agent-runs";
import type { UnifiedEvent, UnifiedPayload } from "@/api/unified-events";

const SESSION = "sess_inspector_parity";
const RUN = "run_eval_2026-06-14-parity";

// ── Logical event fixtures (the single source of truth for both paths) ──────

const SP_DECIDE = "sp_decide";
const SP_MODEL = "sp_model";
const SP_TOOL = "sp_tool";
const SP_BROKER = "sp_broker";
const SP_FAILTOOL = "sp_failtool";

const MODEL_FIXTURE = {
  span_id: SP_MODEL,
  provider: "anthropic",
  model: "claude-opus-4",
  input_token_count: 1234,
  output_token_count: 567,
  cost_usd: 0.0421,
  prompt_hash: "ph_abc",
  response_hash: "rh_def",
  prompt_text: "You are a trader. Decide.",
  response_text: "BUY 1 BTC",
  prompt_payload_ref: "blob:prompt:1",
  response_payload_ref: "blob:response:1",
};

const TOOL_INPUT = JSON.stringify({ symbol: "BTC", side: "buy" });
const TOOL_OUTPUT = JSON.stringify({ ok: true, order_id: "o1" });

const BROKER_STARTED = {
  span_id: SP_BROKER,
  run_id: RUN,
  side: "buy" as const,
  symbol: "BTC",
  qty: 1.5,
  intended_price: 65000,
  order_type: "market",
  venue: "alpaca-paper",
  // `<run_id>-<decision_idx>` carrier → decision_idx 7
  idempotency_key: `${RUN}-7`,
};
const BROKER_FINISHED = {
  span_id: SP_BROKER,
  outcome: "filled" as const,
  fill_price: 65010,
  fill_qty: 1.5,
  fee: 0.65,
  broker_order_id: "brk_99",
  error_class: null,
  error_message: null,
  severity: null,
};

const TOOL_ERROR_MSG = "tool blew up";

// ── Raw path: assemble the equivalent v3 export payload + normalize ─────────
//
// Mirrors what the recorder writes to disk and what the raw live dock refetches:
//   - `spans[]` tree (with broker_call folded into attributes_json on the
//     broker span, exactly as `SqliteRecorder` writes it),
//   - `model_calls[]` (per-call provider/model/tokens/cost/hashes/bodies),
//   - `tool_calls[]` (input/output refs),
//   - `events[]` (engine lifecycle rows).

function rawRunSpans(): RunSpan[] {
  const detail = validateAgentRunDetail({
    schema_version: "xvn.agent_run.v3",
    run_id: RUN,
    objective: "parity",
    status: "completed",
    started_at: "2026-06-14T10:00:00.000Z",
    finished_at: "2026-06-14T10:00:10.000Z",
    retention_mode: "full_debug",
    totals: { model_calls: 1, tool_calls: 2, cost_usd: 0.0421 },
    spans: [
      {
        id: SP_DECIDE,
        parent_span_id: null,
        name: "decision",
        kind: "agent.decision",
        started_at: "2026-06-14T10:00:00.000Z",
        ended_at: "2026-06-14T10:00:09.000Z",
        status: "ok",
        children: [
          {
            id: SP_MODEL,
            parent_span_id: SP_DECIDE,
            name: "claude-opus",
            kind: "decision.model",
            started_at: "2026-06-14T10:00:01.000Z",
            ended_at: "2026-06-14T10:00:02.000Z",
            status: "ok",
          },
          {
            id: SP_TOOL,
            parent_span_id: SP_DECIDE,
            name: "submit_order",
            kind: "tool.call",
            started_at: "2026-06-14T10:00:03.000Z",
            ended_at: "2026-06-14T10:00:04.000Z",
            status: "ok",
          },
          {
            id: SP_FAILTOOL,
            parent_span_id: SP_DECIDE,
            name: "broken_tool",
            kind: "tool.call",
            started_at: "2026-06-14T10:00:05.000Z",
            ended_at: "2026-06-14T10:00:06.000Z",
            status: "error",
            error_json: JSON.stringify({ message: TOOL_ERROR_MSG }),
          },
          {
            id: SP_BROKER,
            parent_span_id: SP_DECIDE,
            name: "broker submit",
            kind: "broker.call",
            started_at: "2026-06-14T10:00:07.000Z",
            ended_at: "2026-06-14T10:00:08.000Z",
            status: "ok",
            attributes_json: JSON.stringify({
              broker_call: {
                side: BROKER_STARTED.side,
                symbol: BROKER_STARTED.symbol,
                qty: BROKER_STARTED.qty,
                intended_price: BROKER_STARTED.intended_price,
                order_type: BROKER_STARTED.order_type,
                venue: BROKER_STARTED.venue,
                idempotency_key: BROKER_STARTED.idempotency_key,
                outcome: BROKER_FINISHED.outcome,
                fill_price: BROKER_FINISHED.fill_price,
                fill_qty: BROKER_FINISHED.fill_qty,
                fee: BROKER_FINISHED.fee,
                broker_order_id: BROKER_FINISHED.broker_order_id,
                error_class: BROKER_FINISHED.error_class,
                error_message: BROKER_FINISHED.error_message,
                severity: BROKER_FINISHED.severity,
              },
            }),
          },
        ],
      },
    ],
    model_calls: [
      {
        span_id: SP_MODEL,
        provider: MODEL_FIXTURE.provider,
        model: MODEL_FIXTURE.model,
        input_token_count: MODEL_FIXTURE.input_token_count,
        output_token_count: MODEL_FIXTURE.output_token_count,
        cost_usd: MODEL_FIXTURE.cost_usd,
        prompt_hash: MODEL_FIXTURE.prompt_hash,
        response_hash: MODEL_FIXTURE.response_hash,
        prompt_text: MODEL_FIXTURE.prompt_text,
        response_text: MODEL_FIXTURE.response_text,
        prompt_payload_ref: MODEL_FIXTURE.prompt_payload_ref,
        response_payload_ref: MODEL_FIXTURE.response_payload_ref,
      },
    ],
    tool_calls: [
      { span_id: SP_TOOL, tool_name: "submit_order" },
      { span_id: SP_FAILTOOL, tool_name: "broken_tool" },
    ],
    events: [
      {
        run_id: RUN,
        span_id: SP_DECIDE,
        kind: "risk_veto",
        payload_json: JSON.stringify({ reason: "max_drawdown" }),
        created_at: "2026-06-14T10:00:02.500Z",
      },
    ],
  });
  return detail.spans;
}

// ── Unified path: the same logical events as a UnifiedEvent stream ──────────

function ue(seq: number, payload: UnifiedPayload, over: Partial<UnifiedEvent> = {}): UnifiedEvent {
  return {
    event_id: `ev_${seq}`,
    session_id: SESSION,
    run_id: RUN,
    span_id: null,
    parent_event_id: null,
    seq,
    ts: `2026-06-14T10:00:0${seq}.000Z`,
    scope: { kind: "run", id: RUN },
    actor: "system",
    source: "agent_run",
    blob_hash: null,
    payload,
    ...over,
  };
}

function ingestUnifiedStream(): void {
  const store = useSessionEvents.getState();
  let seq = 0;
  const span = (
    span_id: string,
    parent: string | null,
    kind: string,
    name: string,
    started_at: string,
  ): UnifiedPayload =>
    ({
      kind: "span_started",
      data: {
        span_id,
        run_id: RUN,
        parent_span_id: parent,
        kind,
        name,
        started_at,
        otel_trace_id: null,
        otel_span_id: null,
        attributes_json: null,
      },
    }) as UnifiedPayload;
  const finish = (
    span_id: string,
    ended_at: string,
    status: "ok" | "error",
    error_json: string | null = null,
  ): UnifiedPayload =>
    ({
      kind: "span_finished",
      data: { span_id, ended_at, status, error_json },
    }) as UnifiedPayload;

  // Decision (parent) + four leaf spans.
  store.ingest(SESSION, ue(seq++, span(SP_DECIDE, null, "agent.decision", "decision", "2026-06-14T10:00:00.000Z")));
  store.ingest(SESSION, ue(seq++, span(SP_MODEL, SP_DECIDE, "decision.model", "claude-opus", "2026-06-14T10:00:01.000Z"), { span_id: SP_MODEL }));
  store.ingest(SESSION, ue(seq++, span(SP_TOOL, SP_DECIDE, "tool.call", "submit_order", "2026-06-14T10:00:03.000Z"), { span_id: SP_TOOL }));
  store.ingest(SESSION, ue(seq++, span(SP_FAILTOOL, SP_DECIDE, "tool.call", "broken_tool", "2026-06-14T10:00:05.000Z"), { span_id: SP_FAILTOOL }));
  store.ingest(SESSION, ue(seq++, span(SP_BROKER, SP_DECIDE, "broker.call", "broker submit", "2026-06-14T10:00:07.000Z"), { span_id: SP_BROKER }));

  // Tool request (carries input_text) + tool finished (carries output_text).
  store.ingest(
    SESSION,
    ue(seq++, {
      kind: "tool_requested",
      data: {
        span_id: SP_TOOL,
        tool_name: "submit_order",
        origin: "Native",
        tool_version: null,
        tool_hash: null,
        side_effect_level: "external_write",
        risk_level: "strategy_mutation",
        requires_approval: false,
        is_run_terminator: false,
        input_hash: "ih",
        input_payload_ref: null,
        input_text: TOOL_INPUT,
      },
    } as UnifiedPayload, { span_id: SP_TOOL }),
  );
  store.ingest(
    SESSION,
    ue(seq++, {
      kind: "tool_finished",
      data: { span_id: SP_TOOL, output_hash: "oh", output_payload_ref: null, exit_code: 0, output_text: TOOL_OUTPUT },
    } as UnifiedPayload, { span_id: SP_TOOL }),
  );

  // Model call finished (carries bodies/tokens/cost/hashes/refs).
  store.ingest(
    SESSION,
    ue(seq++, {
      kind: "model_call_finished",
      data: {
        span_id: SP_MODEL,
        provider: MODEL_FIXTURE.provider,
        model: MODEL_FIXTURE.model,
        input_token_count: MODEL_FIXTURE.input_token_count,
        output_token_count: MODEL_FIXTURE.output_token_count,
        cost_usd: MODEL_FIXTURE.cost_usd,
        prompt_hash: MODEL_FIXTURE.prompt_hash,
        response_hash: MODEL_FIXTURE.response_hash,
        prompt_text: MODEL_FIXTURE.prompt_text,
        response_text: MODEL_FIXTURE.response_text,
        prompt_payload_ref: MODEL_FIXTURE.prompt_payload_ref,
        response_payload_ref: MODEL_FIXTURE.response_payload_ref,
        tool_calls_requested: null,
        capability_path: null,
      },
    } as UnifiedPayload, { span_id: SP_MODEL }),
  );

  // Failing tool.
  store.ingest(
    SESSION,
    ue(seq++, {
      kind: "tool_requested",
      data: {
        span_id: SP_FAILTOOL,
        tool_name: "broken_tool",
        origin: "Native",
        tool_version: null,
        tool_hash: null,
        side_effect_level: "read_only",
        risk_level: "safe_read",
        requires_approval: false,
        is_run_terminator: false,
        input_hash: "ih2",
        input_payload_ref: null,
      },
    } as UnifiedPayload, { span_id: SP_FAILTOOL }),
  );
  store.ingest(
    SESSION,
    ue(seq++, {
      kind: "tool_failed",
      data: { span_id: SP_FAILTOOL, error_json: JSON.stringify({ message: TOOL_ERROR_MSG }) },
    } as UnifiedPayload, { span_id: SP_FAILTOOL }),
  );

  // Broker started + finished.
  store.ingest(SESSION, ue(seq++, { kind: "broker_call_started", data: BROKER_STARTED } as UnifiedPayload, { span_id: SP_BROKER }));
  store.ingest(SESSION, ue(seq++, { kind: "broker_call_finished", data: BROKER_FINISHED } as UnifiedPayload, { span_id: SP_BROKER }));

  // Engine event (risk veto), scoped to the decision.
  store.ingest(
    SESSION,
    ue(seq++, {
      kind: "engine_event",
      data: {
        run_id: RUN,
        span_id: SP_DECIDE,
        kind: "risk_veto",
        payload_json: JSON.stringify({ reason: "max_drawdown" }),
        created_at: "2026-06-14T10:00:02.500Z",
      },
    } as UnifiedPayload, { span_id: SP_DECIDE }),
  );

  // Close the lifecycle spans.
  store.ingest(SESSION, ue(seq++, finish(SP_MODEL, "2026-06-14T10:00:02.000Z", "ok"), { span_id: SP_MODEL }));
  store.ingest(SESSION, ue(seq++, finish(SP_TOOL, "2026-06-14T10:00:04.000Z", "ok"), { span_id: SP_TOOL }));
  store.ingest(SESSION, ue(seq++, finish(SP_FAILTOOL, "2026-06-14T10:00:06.000Z", "error", JSON.stringify({ message: TOOL_ERROR_MSG })), { span_id: SP_FAILTOOL }));
  store.ingest(SESSION, ue(seq++, finish(SP_BROKER, "2026-06-14T10:00:08.000Z", "ok"), { span_id: SP_BROKER }));
  store.ingest(SESSION, ue(seq++, finish(SP_DECIDE, "2026-06-14T10:00:09.000Z", "ok"), { span_id: SP_DECIDE }));
}

function unifiedRunSpans(): RunSpan[] {
  ingestUnifiedStream();
  const spans: SpanProjection[] = useSessionEvents.getState().spansFor(SESSION);
  return spans.map(projectionToRunSpan);
}

function byId(spans: RunSpan[]): Map<string, RunSpan> {
  return new Map(spans.map((s) => [s.span_id, s]));
}

describe("WS-8 Part 2 Part B — full-inspector-fidelity parity (never-go-dark)", () => {
  beforeEach(() => {
    useSessionEvents.setState({ sessions: {} });
  });

  test("MODEL span: body/tokens/cost/hashes/refs reach parity with the raw path", () => {
    const raw = byId(rawRunSpans()).get(SP_MODEL)!;
    const unified = byId(unifiedRunSpans()).get(SP_MODEL)!;
    expect(raw).toBeTruthy();
    expect(unified).toBeTruthy();

    expect(unified.provider).toBe(raw.provider);
    expect(unified.model).toBe(raw.model);
    expect(unified.tokens_in).toBe(raw.tokens_in);
    expect(unified.tokens_out).toBe(raw.tokens_out);
    expect(unified.cost).toBe(raw.cost);
    expect(unified.hash).toBe(raw.hash); // prompt_hash
    expect(unified.response_hash).toBe(raw.response_hash);
    expect(unified.prompt).toBe(raw.prompt);
    expect(unified.response).toBe(raw.response);
    expect(unified.prompt_payload_ref).toBe(raw.prompt_payload_ref);
    expect(unified.response_payload_ref).toBe(raw.response_payload_ref);

    // Concrete values (not both-undefined false-parity).
    expect(unified.provider).toBe("anthropic");
    expect(unified.tokens_in).toBe(1234);
    expect(unified.cost).toBe(0.0421);
    expect(unified.prompt).toBe(MODEL_FIXTURE.prompt_text);
    expect(unified.response).toBe(MODEL_FIXTURE.response_text);
  });

  test("BROKER span: fill detail reaches parity with the raw broker_call projection", () => {
    const raw = byId(rawRunSpans()).get(SP_BROKER)!;
    const unified = byId(unifiedRunSpans()).get(SP_BROKER)!;
    expect(raw.broker_call).toBeTruthy();
    expect(unified.broker_call).toBeTruthy();
    expect(unified.broker_call).toEqual(raw.broker_call);

    // decision_idx parsed off the idempotency key — same on both paths.
    expect(unified.decision_idx).toBe(raw.decision_idx);
    expect(unified.decision_idx).toBe(7);

    // Concrete fill values.
    expect(unified.broker_call!.outcome).toBe("filled");
    expect(unified.broker_call!.fill_price).toBe(65010);
    expect(unified.broker_call!.symbol).toBe("BTC");
  });

  test("TOOL span: args + result reach inspector fidelity", () => {
    const unified = byId(unifiedRunSpans()).get(SP_TOOL)!;
    // The raw export carries tool input/output as refs/hashes in tool_calls[],
    // not folded onto the span; the inspector surfaces the unified path's
    // args/result from input_text/output_text — the richest available body.
    expect(unified.args).toEqual(JSON.parse(TOOL_INPUT));
    expect(unified.result).toEqual(JSON.parse(TOOL_OUTPUT));
  });

  test("FAILING TOOL span: error_message reaches parity with the raw path", () => {
    const raw = byId(rawRunSpans()).get(SP_FAILTOOL)!;
    const unified = byId(unifiedRunSpans()).get(SP_FAILTOOL)!;
    expect(unified.status).toBe("error");
    expect(unified.status).toBe(raw.status);
    expect(unified.error_message).toBe(TOOL_ERROR_MSG);
    expect(unified.error_message).toBe(raw.error_message);
  });

  test("ENGINE EVENT row: present on both paths with the same kind + payload + scoping", () => {
    const rawEngine = rawRunSpans().filter((s) => s.kind === "engine.event");
    const unifiedEngine = unifiedRunSpans().filter((s) => s.kind === "engine.event");
    expect(rawEngine).toHaveLength(1);
    expect(unifiedEngine).toHaveLength(1);
    expect(unifiedEngine[0]!.attributes.engine_event_kind).toBe(
      rawEngine[0]!.attributes.engine_event_kind,
    );
    expect(unifiedEngine[0]!.attributes.engine_event_kind).toBe("risk_veto");
    expect(unifiedEngine[0]!.parent_span_id).toBe(SP_DECIDE);
    expect(unifiedEngine[0]!.attributes.engine_event_payload).toEqual({ reason: "max_drawdown" });
  });

  test("the FULL RunSpan set is equivalent across paths (kind + scoping + inspector fields)", () => {
    const raw = rawRunSpans();
    const unified = unifiedRunSpans();

    // Same lifecycle spans (engine.event rows compared separately above; their
    // synthetic ids differ by construction but their content was asserted).
    const lifecycleId = (s: RunSpan) => s.kind !== "engine.event";
    const rawLifecycle = byId(raw.filter(lifecycleId));
    const unifiedLifecycle = byId(unified.filter(lifecycleId));
    expect([...unifiedLifecycle.keys()].sort()).toEqual([...rawLifecycle.keys()].sort());

    for (const [id, r] of rawLifecycle) {
      const u = unifiedLifecycle.get(id)!;
      expect(u.kind).toBe(r.kind);
      expect(u.parent_span_id).toBe(r.parent_span_id);
      expect(u.status).toBe(r.status);
      // Inspector fields that the raw path populates must be present on unified.
      expect(u.provider).toBe(r.provider);
      expect(u.model).toBe(r.model);
      expect(u.tokens_in).toBe(r.tokens_in);
      expect(u.tokens_out).toBe(r.tokens_out);
      expect(u.cost).toBe(r.cost);
      expect(u.hash).toBe(r.hash);
      expect(u.response_hash).toBe(r.response_hash);
      expect(u.prompt).toBe(r.prompt);
      expect(u.response).toBe(r.response);
      expect(u.broker_call).toEqual(r.broker_call);
      expect(u.decision_idx).toBe(r.decision_idx);
      expect(u.error_message).toBe(r.error_message);
    }
  });
});
