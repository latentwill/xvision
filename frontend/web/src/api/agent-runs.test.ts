// frontend/web/src/api/agent-runs.test.ts
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import {
  agentRunKeys,
  getAgentRun,
  openAgentRunStream,
  shouldUseMockAgentRuns,
  validateAgentRunDetail,
} from "./agent-runs";
import { MOCK_RUN_COMPLETED, MOCK_RUN_FULL_DEBUG } from "@/features/agent-runs/mock-fixtures";

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
    expect(detail.model_calls[0]?.response_text).toBeNull();
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
});
