// frontend/web/src/api/agent-runs.test.ts
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import {
  REAL_SSE_EVENTS,
  agentRunKeys,
  engineEventFrameToSpan,
  fetchAgentRunBlob,
  getAgentRun,
  getAgentRunMemoryEvents,
  listAgentRuns,
  openAgentRunStream,
  shouldUseMockAgentRuns,
  validateAgentRunDetail,
} from "./agent-runs";
import { MOCK_RUN_COMPLETED, MOCK_RUN_FULL_DEBUG } from "@/features/agent-runs/mock-fixtures";
import { useTraceDock } from "@/stores/trace-dock";
import type { AgentRunStreamEvent } from "./types-agent-runs";

const EXPORT_PAYLOAD = {
  schema_version: "xvn.agent_run.v1",
  run_id: "run_export_1",
  objective: "Inspect a real export",
  strategy_id: "strat_1",
  eval_run_id: null,
  status: "interrupted",
  retention_mode: "hash_only",
  started_at: "2026-05-17T16:00:00Z",
  finished_at: "2026-05-17T16:00:03Z",
  totals: {
    model_calls: 1,
    tool_calls: 1,
    approvals: 0,
    input_tokens: 10,
    output_tokens: 5,
    cost_usd: 0.001,
  },
  spans: [
    {
      id: "span_root",
      run_id: "run_export_1",
      parent_span_id: null,
      kind: "agent.run",
      name: "agent.run",
      status: "ok",
      started_at: "2026-05-17T16:00:00Z",
      ended_at: "2026-05-17T16:00:03Z",
      duration_ms: 3000,
      attributes_json: "{\"phase\":\"root\"}",
      error_json: null,
      children: [
        {
          id: "span_model",
          run_id: "run_export_1",
          parent_span_id: "span_root",
          kind: "model.call",
          name: "anthropic/claude",
          status: "ok",
          started_at: "2026-05-17T16:00:01Z",
          ended_at: "2026-05-17T16:00:02Z",
          duration_ms: 1000,
          attributes_json: null,
          error_json: null,
          children: [],
        },
      ],
    },
  ],
  model_calls: [
    {
      span_id: "span_model",
      provider: "anthropic",
      model: "claude",
      input_token_count: 10,
      output_token_count: 5,
      cost_usd: 0.001,
      prompt_hash: "sha256:abc",
      response_hash: "sha256:def",
    },
  ],
  tool_calls: [],
};

describe("agent-runs API (mock mode)", () => {
  test("shouldUseMockAgentRuns is true under vitest (MODE=test)", () => {
    expect(shouldUseMockAgentRuns()).toBe(true);
  });

  test("getAgentRun returns the canned completed run", async () => {
    const detail = await getAgentRun("run_abc1234");
    expect(detail.summary.run_id).toBe("run_abc1234");
    expect(detail.spans.length).toBeGreaterThan(0);
    expect(detail.summary.retention_mode).toBe("hash_only");
  });

  test("getAgentRun returns the full_debug fixture with retention_mode=full_debug", async () => {
    const detail = await getAgentRun("run_debug42");
    expect(detail.summary.retention_mode).toBe("full_debug");
  });

  test("getAgentRun for unknown id rejects with not_found", async () => {
    await expect(getAgentRun("missing")).rejects.toMatchObject({
      code: "not_found",
    });
  });

  test("agentRunKeys.run produces a stable cache key", () => {
    expect(agentRunKeys.run("x")).toEqual(["agent-runs", "run", "x"]);
  });
});

describe("validateAgentRunDetail", () => {
  test("accepts the mock fixture", () => {
    expect(() => validateAgentRunDetail(MOCK_RUN_COMPLETED)).not.toThrow();
  });

  test("rejects non-object payloads", () => {
    expect(() => validateAgentRunDetail(null)).toThrow(/invalid/i);
    expect(() => validateAgentRunDetail(42)).toThrow(/invalid/i);
  });

  test("rejects payloads missing retention_mode", () => {
    const broken = {
      ...MOCK_RUN_COMPLETED,
      summary: { ...MOCK_RUN_COMPLETED.summary, retention_mode: undefined },
    };
    expect(() => validateAgentRunDetail(broken)).toThrow(/retention_mode/);
  });

  test("rejects payloads with invalid status", () => {
    const broken = {
      ...MOCK_RUN_COMPLETED,
      summary: { ...MOCK_RUN_COMPLETED.summary, status: "weird" },
    };
    expect(() => validateAgentRunDetail(broken)).toThrow(/status/);
  });

  test("rejects payloads with non-array spans", () => {
    const broken = { ...MOCK_RUN_COMPLETED, spans: "nope" };
    expect(() => validateAgentRunDetail(broken)).toThrow(/spans/);
  });

  test("accepts the full_debug fixture", () => {
    expect(() => validateAgentRunDetail(MOCK_RUN_FULL_DEBUG)).not.toThrow();
  });

  test("normalizes the backend xvn.agent_run.v1 export shape", () => {
    const detail = validateAgentRunDetail(EXPORT_PAYLOAD);
    expect(detail.summary.run_id).toBe("run_export_1");
    expect(detail.summary.status).toBe("interrupted");
    expect(detail.summary.span_count).toBe(2);
    expect(detail.summary.total_input_tokens).toBe(10);
    expect(detail.spans.map((s) => s.span_id)).toEqual(["span_root", "span_model"]);
    expect(detail.spans[0]?.attributes).toEqual({ phase: "root" });
    expect(detail.model_calls[0]?.input_tokens).toBe(10);
    expect(detail.model_calls[0]?.response_hash).toBe("sha256:def");
    expect(detail.model_calls[0]?.prompt_text).toBeNull();
    expect(detail.model_calls[0]?.response_text).toBeNull();
  });

  test("normalizes backend xvn.agent_run.v2 accounting exports", () => {
    const detail = validateAgentRunDetail({
      ...EXPORT_PAYLOAD,
      schema_version: "xvn.agent_run.v2",
      status: "completed",
      finished_at: "2026-05-17T16:00:09Z",
      eval_run_id: "eval_run_1",
      totals: {
        ...EXPORT_PAYLOAD.totals,
        model_calls: 0,
        input_tokens: 46473,
        output_tokens: 991,
      },
      accounting: {
        source: "eval_actuals",
        eval_run_id: "eval_run_1",
        eval_mode: "live",
        eval_status: "completed",
        eval_actual_input_tokens: 46473,
        eval_actual_output_tokens: 991,
        eval_model_calls: 0,
        eval_model_call_input_tokens: null,
        eval_model_call_output_tokens: null,
        eval_model_call_cost_usd: null,
      },
    });

    expect(detail.summary.status).toBe("completed");
    expect(detail.summary.finished_at).toBe("2026-05-17T16:00:09Z");
    expect(detail.summary.financial_eval_id).toBe("eval_run_1");
    expect(detail.summary.total_input_tokens).toBe(46473);
    expect(detail.summary.total_output_tokens).toBe(991);
    expect((detail.summary as any).accounting?.source).toBe("eval_actuals");
    expect((detail.summary as any).accounting?.eval_mode).toBe("live");
  });

  test("projects model_calls.provider/model/hashes onto the matching span", () => {
    const detail = validateAgentRunDetail(EXPORT_PAYLOAD);
    const modelSpan = detail.spans.find((s) => s.span_id === "span_model");
    expect(modelSpan?.provider).toBe("anthropic");
    expect(modelSpan?.model).toBe("claude");
    expect(modelSpan?.tokens_in).toBe(10);
    expect(modelSpan?.tokens_out).toBe(5);
    expect(modelSpan?.cost).toBeCloseTo(0.001);
    expect(modelSpan?.hash).toBe("sha256:abc");
    expect(modelSpan?.response_hash).toBe("sha256:def");
  });

  test("projects plaintext model prompt and response onto the matching span", () => {
    const detail = validateAgentRunDetail({
      ...EXPORT_PAYLOAD,
      model_calls: [
        {
          ...EXPORT_PAYLOAD.model_calls[0],
          prompt_text: "decide whether to trade BTC",
          response_text: "{\"action\":\"hold\"}",
        },
      ],
    });
    const modelSpan = detail.spans.find((s) => s.span_id === "span_model");
    expect(modelSpan?.prompt).toBe("decide whether to trade BTC");
    expect(modelSpan?.response).toBe("{\"action\":\"hold\"}");
    expect(detail.model_calls[0]?.prompt_text).toBe("decide whether to trade BTC");
    expect(detail.model_calls[0]?.response_text).toBe("{\"action\":\"hold\"}");
  });

  test("surfaces payload refs when retention preserves them", () => {
    const withRefs = {
      ...EXPORT_PAYLOAD,
      retention_mode: "full_debug",
      model_calls: [
        {
          ...EXPORT_PAYLOAD.model_calls[0],
          prompt_payload_ref: "blob://prompts/abc",
          response_payload_ref: "blob://responses/def",
        },
      ],
    };
    const detail = validateAgentRunDetail(withRefs);
    const modelSpan = detail.spans.find((s) => s.span_id === "span_model");
    expect(modelSpan?.prompt_payload_ref).toBe("blob://prompts/abc");
    expect(modelSpan?.response_payload_ref).toBe("blob://responses/def");
  });

  // qa-trace-error-surfacing (2026-05-17): error_json on a span must
  // surface as a first-class `error_message` field so the inspector
  // can render it without reaching into raw attributes.
  test("parses error_json {message:...} payload into RunSpan.error_message", () => {
    const payload = {
      ...EXPORT_PAYLOAD,
      run_id: "run_with_error",
      status: "failed",
      spans: [
        {
          id: "span_failed_call",
          run_id: "run_with_error",
          parent_span_id: null,
          kind: "model.call",
          name: "anthropic/claude",
          status: "error",
          started_at: "2026-05-17T16:00:00Z",
          ended_at: "2026-05-17T16:00:01Z",
          duration_ms: 1000,
          attributes_json: null,
          error_json: JSON.stringify({
            message:
              "[unclassified] error decoding response body: EOF while parsing a value at line 1145 column 0",
          }),
          children: [],
        },
      ],
    };
    const detail = validateAgentRunDetail(payload);
    const span = detail.spans[0];
    expect(span?.status).toBe("error");
    expect(span?.error_message).toContain("EOF while parsing");
    expect(span?.error_message).toContain("[unclassified]");
    // Error count rolled up correctly.
    expect(detail.summary.error_count).toBeGreaterThanOrEqual(1);
  });

  test("falls back to bare error_json strings when not JSON-wrapped", () => {
    const payload = {
      ...EXPORT_PAYLOAD,
      run_id: "run_bare_error",
      status: "failed",
      spans: [
        {
          id: "span_x",
          run_id: "run_bare_error",
          parent_span_id: null,
          kind: "tool.call",
          name: "tool",
          status: "error",
          started_at: "2026-05-17T16:00:00Z",
          ended_at: null,
          duration_ms: null,
          attributes_json: null,
          error_json: "raw-string-no-json-wrap",
          children: [],
        },
      ],
    };
    const detail = validateAgentRunDetail(payload);
    expect(detail.spans[0]?.status).toBe("error");
    expect(detail.spans[0]?.error_message).toBe("raw-string-no-json-wrap");
  });

  test("drops error_message when error_json is null", () => {
    const detail = validateAgentRunDetail(EXPORT_PAYLOAD);
    for (const span of detail.spans) {
      expect(span.error_message).toBeUndefined();
    }
  });

  // PR #385 followup: broker.call spans must surface
  // `decision_idx` projected from `attributes_json.broker_call.idempotency_key`
  // so the FilterBar dropdown / per-cycle filter chip / DecisionJump
  // button can resolve the cycle a span is attached to without each
  // consumer re-parsing the key.
  test("projects decision_idx onto broker.call spans from idempotency_key", () => {
    const payload = {
      ...EXPORT_PAYLOAD,
      run_id: "run_with_broker",
      spans: [
        {
          id: "span_broker",
          run_id: "run_with_broker",
          parent_span_id: null,
          kind: "broker.call",
          name: "alpaca-paper AAPL Buy",
          status: "ok",
          started_at: "2026-05-17T16:00:00Z",
          ended_at: "2026-05-17T16:00:01Z",
          duration_ms: 1000,
          attributes_json: JSON.stringify({
            run_id: "run_with_broker",
            broker_call: {
              side: "buy",
              symbol: "AAPL",
              qty: 1,
              intended_price: 195.0,
              order_type: "market",
              venue: "alpaca-paper",
              idempotency_key: "run_with_broker-14",
              outcome: "filled",
              fill_price: 195.01,
              fill_qty: 1,
            },
          }),
          error_json: null,
          children: [],
        },
      ],
      model_calls: [],
      tool_calls: [],
    };
    const detail = validateAgentRunDetail(payload);
    const broker = detail.spans.find((s) => s.span_id === "span_broker");
    expect(broker?.decision_idx).toBe(14);
    expect(broker?.broker_call?.idempotency_key).toBe("run_with_broker-14");
  });

  test("does not populate decision_idx on non-broker spans", () => {
    // Today only broker.call spans carry the carrier; model.call /
    // tool.call etc. should never get a stray decision_idx because
    // their attributes don't currently encode one and we don't want to
    // fabricate values.
    const detail = validateAgentRunDetail(EXPORT_PAYLOAD);
    for (const span of detail.spans) {
      expect(span.decision_idx).toBeUndefined();
    }
  });

  test("omits decision_idx on broker.call spans with a malformed idempotency_key", () => {
    const payload = {
      ...EXPORT_PAYLOAD,
      run_id: "run_malformed_key",
      spans: [
        {
          id: "span_broker_bad",
          run_id: "run_malformed_key",
          parent_span_id: null,
          kind: "broker.call",
          name: "alpaca-paper AAPL Buy",
          status: "ok",
          started_at: "2026-05-17T16:00:00Z",
          ended_at: "2026-05-17T16:00:01Z",
          duration_ms: 1000,
          attributes_json: JSON.stringify({
            broker_call: {
              side: "buy",
              symbol: "AAPL",
              qty: 1,
              order_type: "market",
              venue: "alpaca-paper",
              idempotency_key: "no-trailing-integer-here",
            },
          }),
          error_json: null,
          children: [],
        },
      ],
      model_calls: [],
      tool_calls: [],
    };
    const detail = validateAgentRunDetail(payload);
    const broker = detail.spans.find((s) => s.span_id === "span_broker_bad");
    expect(broker).toBeDefined();
    expect(broker?.decision_idx).toBeUndefined();
  });
});

describe("agent-runs real-mode branch", () => {
  // Flip the flag explicitly OFF so we exercise the real fetch path.
  // `vi.stubEnv` updates `import.meta.env` for the current test and is
  // automatically reverted by `vi.unstubAllEnvs` in afterEach.
  beforeEach(() => {
    vi.stubEnv("VITE_USE_MOCK_AGENT_RUNS", "0");
    vi.stubEnv("MODE", "production");
    // `import.meta.env.DEV` is a boolean in Vite; stubbing as a string keeps
    // it falsy for the explicit-string check inside shouldUseMockAgentRuns().
    // DEV/PROD are typed `boolean` in Vite's ImportMetaEnv, so stub with
    // booleans (cast away from the `string` overload).
    (vi.stubEnv as unknown as (k: string, v: boolean) => void)("DEV", false);
    (vi.stubEnv as unknown as (k: string, v: boolean) => void)("PROD", true);
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
  });

  test("getAgentRun calls /api/agent-runs/:id and normalizes the export response", async () => {
    expect(shouldUseMockAgentRuns()).toBe(false);
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(EXPORT_PAYLOAD), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    const detail = await getAgentRun("run_real_1");
    expect(fetchSpy).toHaveBeenCalledWith(
      "/api/agent-runs/run_real_1",
      expect.objectContaining({ headers: expect.any(Object) }),
    );
    expect(detail.summary.run_id).toBe("run_export_1");
    expect(detail.summary.status).toBe("interrupted");
  });

  test("getAgentRun throws invalid_response when the backend returns garbage", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ wrong: "shape" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    await expect(getAgentRun("run_bad")).rejects.toMatchObject({
      code: "invalid_response",
    });
  });

  test("openAgentRunStream opens an EventSource against /api/agent-runs/:id/stream", () => {
    type EsCtor = (url: string) => EventSource;
    const ctorCalls: string[] = [];
    const original = globalThis.EventSource;
    class CapturingES {
      url: string;
      constructor(url: string) {
        this.url = url;
        ctorCalls.push(url);
      }
      addEventListener() {}
      removeEventListener() {}
      close() {}
    }
    (globalThis as { EventSource: unknown }).EventSource = CapturingES as unknown as EsCtor;
    try {
      const close = openAgentRunStream("run_real_2", () => {});
      expect(ctorCalls).toEqual(["/api/agent-runs/run_real_2/stream"]);
      close();
    } finally {
      (globalThis as { EventSource: unknown }).EventSource = original;
    }
  });

  test("openAgentRunStream maps SSE event names → typed events and dispatches to store", () => {
    type Listener = (ev: MessageEvent) => void;
    const listeners: Record<string, Listener[]> = {};
    class MockES {
      constructor(_url: string) {}
      addEventListener(name: string, fn: EventListener) {
        (listeners[name] ??= []).push(fn as unknown as Listener);
      }
      removeEventListener() {}
      close() {}
    }
    const original = globalThis.EventSource;
    (globalThis as { EventSource: unknown }).EventSource =
      MockES as unknown as typeof EventSource;

    // Make sure prior test state doesn't bleed in.
    useTraceDock.getState().setActiveRun("eval", null, "post-hoc");

    const received: AgentRunStreamEvent[] = [];
    const close = openAgentRunStream("run_stream_1", (ev) => received.push(ev));

    function fire(name: string, payload: unknown) {
      const data = typeof payload === "string" ? payload : JSON.stringify(payload);
      const ev = new MessageEvent(name, { data });
      for (const fn of listeners[name] ?? []) fn(ev);
    }

    try {
      // 1) Snapshot — must parse via validateAgentRunDetail and dispatch.
      fire("snapshot", MOCK_RUN_COMPLETED);

      // 2) Span lifecycle.
      fire("span_started", {
        kind: "kind-tag-ignored",
        span_id: "s_live",
        run_id: "run_stream_1",
        parent_span_id: null,
        kind_dup: "ignored",
        name: "model.call streaming",
        started_at: "2026-05-17T10:00:00.000Z",
      });
      // Tweak: the real payload uses `kind`, not `kind_dup`. Send a proper one too.
      fire("span_started", {
        span_id: "s_live2",
        run_id: "run_stream_1",
        parent_span_id: null,
        kind: "tool.call",
        name: "execute_slot",
        started_at: "2026-05-17T10:00:01.000Z",
      });

      // 3) Delta + lag.
      fire("assistant_text_delta", {
        span_id: "s_live2",
        run_id: "run_stream_1",
        delta_len: 11,
      });
      fire("lagged", { dropped: 7 });
      fire("memory_recall", {
        run_id: "run_stream_1",
        flywheel_cycle_id: "run_stream_1:3",
        decision_id: 3,
        namespace: "agent:A",
        items: [],
      });
      fire("memory_write", {
        run_id: "run_stream_1",
        flywheel_cycle_id: "run_stream_1:3",
        decision_id: 3,
        namespace: "agent:A",
        memory_item_id: "obs_3",
        text_preview: "remembered",
      });

      // WS-8: a live engine_event frame must reach the consumer (transport
      // must NOT drop it) so TraceDock can project it onto an engine.event row.
      fire("engine_event", {
        run_id: "run_stream_1",
        span_id: "s_live2",
        kind: "risk_veto",
        payload_json: JSON.stringify({ reason: "max_drawdown" }),
        created_at: "2026-05-17T10:00:01.200Z",
      });

      // 4) Terminal span event.
      fire("span_finished", {
        span_id: "s_live2",
        ended_at: "2026-05-17T10:00:01.500Z",
        status: "ok",
      });

      // Callback sees one typed event per SSE frame, in order.
      expect(received.map((e) => e.event)).toEqual([
        "snapshot",
        "span_started",
        "span_started",
        "assistant_text_delta",
        "lagged",
        "memory_recall",
        "memory_write",
        "engine_event",
        "span_finished",
      ]);
      // The engine_event frame is delivered with its raw payload intact so the
      // dock can project it (engineEventFrameToSpan). Nothing dropped.
      const engineFrame = received.find((e) => e.event === "engine_event");
      expect(engineFrame?.data).toMatchObject({ kind: "risk_veto" });

      // Store side effects applied.
      const s = useTraceDock.getState().streamingState;
      expect(s.deltaCharsBySpan.s_live2 ?? 0).toBe(11);
      expect(s.droppedEvents).toBe(7);
      // s_live2 was opened then closed.
      expect(s.activeSpanIds.has("s_live2")).toBe(false);
    } finally {
      close();
      (globalThis as { EventSource: unknown }).EventSource = original;
      useTraceDock.getState().setActiveRun("eval", null, "post-hoc");
    }
  });

  test("openAgentRunStream drops malformed snapshot frames without crashing", () => {
    type Listener = (ev: MessageEvent) => void;
    const listeners: Record<string, Listener[]> = {};
    class MockES {
      constructor(_url: string) {}
      addEventListener(name: string, fn: EventListener) {
        (listeners[name] ??= []).push(fn as unknown as Listener);
      }
      removeEventListener() {}
      close() {}
    }
    const original = globalThis.EventSource;
    (globalThis as { EventSource: unknown }).EventSource =
      MockES as unknown as typeof EventSource;

    useTraceDock.getState().setActiveRun("eval", null, "post-hoc");
    const received: AgentRunStreamEvent[] = [];
    const close = openAgentRunStream("run_stream_bad", (ev) => received.push(ev));

    function fire(name: string, raw: string) {
      const ev = new MessageEvent(name, { data: raw });
      for (const fn of listeners[name] ?? []) fn(ev);
    }
    try {
      fire("snapshot", "{not json");
      fire("span_started", "{also not json");
      // A subsequent valid lagged event should still flow.
      fire("lagged", JSON.stringify({ dropped: 1 }));
      expect(received.map((e) => e.event)).toEqual(["lagged"]);
      expect(useTraceDock.getState().streamingState.droppedEvents).toBe(1);
    } finally {
      close();
      (globalThis as { EventSource: unknown }).EventSource = original;
      useTraceDock.getState().setActiveRun("eval", null, "post-hoc");
    }
  });
});

describe("getAgentRunMemoryEvents", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  test("fetches persisted memory events for a run", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          run_id: "run_mem",
          events: [
            {
              kind: "memory_write",
              created_at: "2026-05-25T00:00:00Z",
              payload: {
                run_id: "run_mem",
                flywheel_cycle_id: "run_mem:1",
                decision_id: 1,
                namespace: "agent:A",
                memory_item_id: "obs_1",
                text_preview: "remembered",
              },
            },
          ],
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );

    const body = await getAgentRunMemoryEvents("run/mem");
    expect(fetchSpy).toHaveBeenCalledWith(
      "/api/agent-runs/run%2Fmem/memory-events",
      expect.any(Object),
    );
    expect(body.events[0]?.kind).toBe("memory_write");
    expect(body.events[0]?.payload).toMatchObject({
      flywheel_cycle_id: "run_mem:1",
      memory_item_id: "obs_1",
    });
  });
});

describe("fetchAgentRunBlob", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  test("returns the body text on 200", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response("hello prompt body", {
        status: 200,
        headers: { "content-type": "application/octet-stream" },
      }),
    );
    const body = await fetchAgentRunBlob(
      "run_x",
      "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    expect(body).toBe("hello prompt body");
  });

  test("URL-encodes runId and ref so callers can pass slashes safely", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(new Response("ok", { status: 200 }));
    await fetchAgentRunBlob("run/with/slash", "ref:weird");
    expect(fetchSpy).toHaveBeenCalledWith(
      "/api/agent-runs/run%2Fwith%2Fslash/blobs/ref%3Aweird",
      expect.objectContaining({ headers: expect.any(Object) }),
    );
  });

  test("403 surfaces ApiError with code=forbidden + server message", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          code: "forbidden",
          message: "retention is hash_only — blob bodies are not stored on disk",
        }),
        { status: 403, headers: { "content-type": "application/json" } },
      ),
    );
    await expect(
      fetchAgentRunBlob(
        "run_y",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      ),
    ).rejects.toMatchObject({
      status: 403,
      code: "forbidden",
    });
  });

  test("404 surfaces ApiError with code=not_found", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({ code: "not_found", message: "blob not associated with run" }),
        { status: 404, headers: { "content-type": "application/json" } },
      ),
    );
    await expect(
      fetchAgentRunBlob(
        "run_z",
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
      ),
    ).rejects.toMatchObject({
      status: 404,
      code: "not_found",
    });
  });
});

describe("normalizeAgentRunExport — broker.call projection", () => {
  // qa-trace-broker-spans review (P1): broker spans on the wire must
  // surface their `broker_call` payload as a first-class field, not
  // buried in `attributes`. The dashboard recorder bakes the started
  // payload into `attributes_json.broker_call` and json_set's the
  // finished fields onto the same object; `normalizeAgentRunExport`
  // projects that JSON onto `RunSpan.broker_call` so SpanInspector
  // renders without re-reading attributes.
  test("populates RunSpan.broker_call on broker.call spans", () => {
    const payload = {
      ...EXPORT_PAYLOAD,
      spans: [
        {
          id: "span_broker_real",
          run_id: "run_export_1",
          parent_span_id: null,
          kind: "broker.call",
          name: "paper BTC/USD short",
          status: "ok",
          started_at: "2026-05-17T16:00:01Z",
          ended_at: "2026-05-17T16:00:02Z",
          duration_ms: 1000,
          attributes_json: JSON.stringify({
            broker_call: {
              side: "short",
              symbol: "BTC/USD",
              qty: 0.1,
              intended_price: 60000,
              order_type: "market",
              venue: "paper",
              idempotency_key: "run_export_1-0001",
              outcome: "filled",
              fill_price: 60010,
              fill_qty: 0.1,
              fee: 0.01,
              broker_order_id: "ord_real",
              error_class: null,
              error_message: null,
            },
          }),
          error_json: null,
          children: [],
        },
      ],
    };
    const detail = validateAgentRunDetail(payload);
    const span = detail.spans.find((s) => s.span_id === "span_broker_real");
    expect(span?.kind).toBe("broker.call");
    expect(span?.broker_call?.side).toBe("short");
    expect(span?.broker_call?.outcome).toBe("filled");
    expect(span?.broker_call?.broker_order_id).toBe("ord_real");
    expect(span?.broker_call?.fill_price).toBe(60010);
  });

  test("non-broker.call spans get no broker_call field", () => {
    const detail = validateAgentRunDetail(EXPORT_PAYLOAD);
    for (const span of detail.spans) {
      expect(span.broker_call).toBeUndefined();
    }
  });
});

// WS-8 taxonomy convergence: the v3 export carries an `events` array (every
// EngineEvent for the run). Before WS-8 the trace rendered only `spans` and
// dropped every engine event on the floor. `normalizeAgentRunExport` now
// projects each lifecycle event onto an `engine.event` RunSpan so it flows
// through the existing tree / inspector / filter machinery — nothing dropped.
describe("normalizeAgentRunExport — engine-event projection (WS-8)", () => {
  test("projects events[] lifecycle rows onto engine.event spans", () => {
    const payload = {
      ...EXPORT_PAYLOAD,
      schema_version: "xvn.agent_run.v3",
      events: [
        {
          span_id: "span_model",
          kind: "risk_veto",
          payload_json: { reason: "max_drawdown", decision_index: 3 },
          created_at: "2026-05-17T16:00:01.500Z",
        },
        {
          span_id: null,
          kind: "regime_transition",
          payload_json: { from: "bull", to: "chop" },
          created_at: "2026-05-17T16:00:02.000Z",
        },
      ],
    };
    const detail = validateAgentRunDetail(payload);
    const engineSpans = detail.spans.filter((s) => s.kind === "engine.event");
    expect(engineSpans).toHaveLength(2);

    const risk = engineSpans.find(
      (s) => s.attributes.engine_event_kind === "risk_veto",
    );
    expect(risk).toBeDefined();
    // The lifecycle event inherits its scoping span as parent so the tree can
    // nest it under the decision/model it fired against.
    expect(risk?.parent_span_id).toBe("span_model");
    // Payload survives so the inspector can render it.
    expect((risk?.attributes.engine_event_payload as { reason?: string })?.reason).toBe(
      "max_drawdown",
    );

    const regime = engineSpans.find(
      (s) => s.attributes.engine_event_kind === "regime_transition",
    );
    // Run-scoped (span_id null) events become top-level rows, never dropped.
    expect(regime?.parent_span_id).toBeNull();
  });

  test("does NOT project payload-carrier events (model_call_payload / tool_call_payload)", () => {
    const payload = {
      ...EXPORT_PAYLOAD,
      schema_version: "xvn.agent_run.v3",
      events: [
        {
          span_id: "span_model",
          kind: "model_call_payload",
          payload_json: { prompt: "…" },
          created_at: "2026-05-17T16:00:01.500Z",
        },
        {
          span_id: "span_model",
          kind: "tool_call_payload",
          payload_json: { input: "…" },
          created_at: "2026-05-17T16:00:01.600Z",
        },
      ],
    };
    const detail = validateAgentRunDetail(payload);
    // Those carrier events are folded onto the model/tool spans elsewhere;
    // they must NOT show up as duplicate engine.event rows.
    expect(detail.spans.filter((s) => s.kind === "engine.event")).toHaveLength(0);
  });

  test("a run with no events[] array yields no engine.event spans", () => {
    const detail = validateAgentRunDetail(EXPORT_PAYLOAD);
    expect(detail.spans.some((s) => s.kind === "engine.event")).toBe(false);
  });

  test("an unknown engine-event kind is projected, not dropped", () => {
    const payload = {
      ...EXPORT_PAYLOAD,
      schema_version: "xvn.agent_run.v3",
      events: [
        {
          span_id: null,
          kind: "brand_new_future_signal",
          payload_json: { x: 1 },
          created_at: "2026-05-17T16:00:02.000Z",
        },
      ],
    };
    const detail = validateAgentRunDetail(payload);
    const projected = detail.spans.filter((s) => s.kind === "engine.event");
    expect(projected).toHaveLength(1);
    expect(projected[0]?.attributes.engine_event_kind).toBe("brand_new_future_signal");
  });
});

// WS-8: live `engine_event` SSE frames must convert to the SAME engine.event
// row shape the post-hoc export produces, so live + replayed runs render
// engine events identically.
describe("engineEventFrameToSpan (WS-8 live path)", () => {
  test("parses payload_json string and links to its scoping span", () => {
    const span = engineEventFrameToSpan({
      span_id: "span_42",
      kind: "risk_veto",
      payload_json: JSON.stringify({ reason: "max_drawdown" }),
      created_at: "2026-06-14T10:00:01.500Z",
    });
    expect(span).not.toBeNull();
    expect(span?.kind).toBe("engine.event");
    expect(span?.parent_span_id).toBe("span_42");
    expect(span?.attributes.engine_event_kind).toBe("risk_veto");
    expect(
      (span?.attributes.engine_event_payload as { reason?: string })?.reason,
    ).toBe("max_drawdown");
  });

  test("run-scoped frame (null span_id) becomes a top-level row", () => {
    const span = engineEventFrameToSpan({
      span_id: null,
      kind: "regime_transition",
      payload_json: null,
      created_at: "2026-06-14T10:00:02.000Z",
    });
    expect(span?.parent_span_id).toBeNull();
    expect(span?.attributes.engine_event_kind).toBe("regime_transition");
  });

  test("keeps malformed payload_json as a raw string (never dropped)", () => {
    const span = engineEventFrameToSpan({
      kind: "filter_fired",
      payload_json: "{not-json",
      created_at: "2026-06-14T10:00:03.000Z",
    });
    expect(span?.attributes.engine_event_payload).toBe("{not-json");
  });

  test("returns null for carrier kinds (model_call_payload / tool_call_payload)", () => {
    expect(
      engineEventFrameToSpan({
        kind: "model_call_payload",
        payload_json: "{}",
        created_at: "2026-06-14T10:00:04.000Z",
      }),
    ).toBeNull();
  });

  test("unknown kind still projects (typed fallback)", () => {
    const span = engineEventFrameToSpan({
      kind: "brand_new_live_signal",
      payload_json: null,
      created_at: "2026-06-14T10:00:05.000Z",
    });
    expect(span?.kind).toBe("engine.event");
    expect(span?.attributes.engine_event_kind).toBe("brand_new_live_signal");
  });
});

// bead-008: listAgentRuns threads the `since` time-window param into the
// `GET /api/agent-runs` query string (and only when present).
describe("listAgentRuns — since (real-mode)", () => {
  beforeEach(() => {
    vi.stubEnv("VITE_USE_MOCK_AGENT_RUNS", "0");
    vi.stubEnv("MODE", "production");
    (vi.stubEnv as unknown as (k: string, v: boolean) => void)("DEV", false);
    (vi.stubEnv as unknown as (k: string, v: boolean) => void)("PROD", true);
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
  });

  function mockRunsFetch() {
    return vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ runs: [], total: 0 }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
  }

  test("omits since when not provided", async () => {
    const fetchSpy = mockRunsFetch();
    await listAgentRuns({ status: "running" });
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const url = fetchSpy.mock.calls[0][0] as string;
    expect(url).not.toContain("since=");
  });

  test("appends a URL-encoded since param", async () => {
    const fetchSpy = mockRunsFetch();
    await listAgentRuns({ since: "2026-06-06T00:00:00Z" });
    const url = fetchSpy.mock.calls[0][0] as string;
    expect(url).toContain("since=2026-06-06T00%3A00%3A00Z");
  });

  test("agentRunKeys.list varies on since", () => {
    const a = agentRunKeys.list({ status: "running" });
    const b = agentRunKeys.list({ status: "running", since: "2026-06-06T00:00:00Z" });
    expect(b).not.toEqual(a);
  });
});

// ---------------------------------------------------------------------------
// DoD: shouldUseMockAgentRuns() gate-flip contract
// ---------------------------------------------------------------------------
// TDD: these tests are written against the NEW contract.  The existing
// "is true under vitest" test above exercises the MODE=test path and
// remains valid.  The tests below cover the development-mode flip and
// the explicit-override paths.
// ---------------------------------------------------------------------------
describe("shouldUseMockAgentRuns() — gate-flip contract", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  test("returns false when MODE=development and no explicit override", () => {
    // The dev-auto-true branch was removed: development should hit real HTTP.
    vi.stubEnv("MODE", "development");
    // Ensure no explicit override is set (stub to empty string = unset).
    vi.stubEnv("VITE_USE_MOCK_AGENT_RUNS", "");
    expect(shouldUseMockAgentRuns()).toBe(false);
  });

  test("returns true when MODE=test (vitest baseline)", () => {
    // No stubbing needed — vitest runs with MODE=test by default.
    // But make it explicit so the assertion is unambiguous.
    vi.stubEnv("MODE", "test");
    vi.stubEnv("VITE_USE_MOCK_AGENT_RUNS", "");
    expect(shouldUseMockAgentRuns()).toBe(true);
  });

  test("returns true when VITE_USE_MOCK_AGENT_RUNS=1 regardless of MODE", () => {
    vi.stubEnv("MODE", "development");
    vi.stubEnv("VITE_USE_MOCK_AGENT_RUNS", "1");
    expect(shouldUseMockAgentRuns()).toBe(true);
  });

  test("returns true when VITE_USE_MOCK_AGENT_RUNS=true regardless of MODE", () => {
    vi.stubEnv("MODE", "production");
    vi.stubEnv("VITE_USE_MOCK_AGENT_RUNS", "true");
    expect(shouldUseMockAgentRuns()).toBe(true);
  });

  test("returns false when VITE_USE_MOCK_AGENT_RUNS=0 even in MODE=test", () => {
    vi.stubEnv("MODE", "test");
    vi.stubEnv("VITE_USE_MOCK_AGENT_RUNS", "0");
    expect(shouldUseMockAgentRuns()).toBe(false);
  });

  test("returns false when VITE_USE_MOCK_AGENT_RUNS=false even in MODE=test", () => {
    vi.stubEnv("MODE", "test");
    vi.stubEnv("VITE_USE_MOCK_AGENT_RUNS", "false");
    expect(shouldUseMockAgentRuns()).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// DoD: REAL_SSE_EVENTS wire vocabulary completeness
// ---------------------------------------------------------------------------
describe("REAL_SSE_EVENTS wire vocabulary", () => {
  test("includes broker_call_started", () => {
    expect(REAL_SSE_EVENTS).toContain("broker_call_started");
  });

  test("includes broker_call_finished", () => {
    expect(REAL_SSE_EVENTS).toContain("broker_call_finished");
  });

  test("includes engine_event", () => {
    expect(REAL_SSE_EVENTS).toContain("engine_event");
  });
});
