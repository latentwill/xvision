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

  test("getAgentRun calls /api/agent-runs/:id and validates the response", async () => {
    expect(shouldUseMockAgentRuns()).toBe(false);
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(MOCK_RUN_COMPLETED), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    const detail = await getAgentRun("run_real_1");
    expect(fetchSpy).toHaveBeenCalledWith(
      "/api/agent-runs/run_real_1",
      expect.objectContaining({ headers: expect.any(Object) }),
    );
    expect(detail.summary.run_id).toBe(MOCK_RUN_COMPLETED.summary.run_id);
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
