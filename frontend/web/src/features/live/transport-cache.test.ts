import { describe, expect, test } from "vitest";
import type { AgentRunSummary } from "@/api/types-agent-runs";
import type { RunSummary } from "@/api/types.gen";
import {
  OPTIMISTIC_PATCH,
  patchRunInList,
  reconcileFromRunSummary,
} from "./transport-cache";

function mkRun(over: Partial<AgentRunSummary> = {}): AgentRunSummary {
  return {
    run_id: "run_1",
    objective: "Trade BTC",
    strategy_id: "strat_1",
    agent_id: null,
    started_at: "2026-06-09T10:00:00Z",
    finished_at: null,
    status: "running",
    span_count: 0,
    model_call_count: 7,
    tool_call_count: 4,
    error_count: 0,
    total_cost_usd: 0,
    total_input_tokens: 0,
    total_output_tokens: 0,
    duration_ms: null,
    financial_eval_id: null,
    retention_mode: "hash_only",
    ...over,
  };
}

function mkEval(over: Partial<RunSummary> = {}): RunSummary {
  return {
    id: "run_1",
    agent_id: "strat_1",
    scenario_id: "scen_1",
    strategy: null,
    scenario: null,
    mode: "live",
    status: "running",
    started_at: "2026-06-09T10:00:00Z",
    completed_at: null,
    sharpe: null,
    max_drawdown_pct: null,
    total_return_pct: null,
    error: null,
    actual_input_tokens: null,
    actual_output_tokens: null,
    inference_cost_quote_total: null,
    net_return_pct: null,
    filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: null,
    paused: false,
    paused_at: null,
    flatten_requested: false,
    ...over,
  } as RunSummary;
}

describe("OPTIMISTIC_PATCH", () => {
  test("pause flips paused true; resume flips paused false", () => {
    expect(OPTIMISTIC_PATCH.pause).toEqual({ paused: true });
    expect(OPTIMISTIC_PATCH.resume).toEqual({ paused: false });
  });
  test("flatten flips only flatten_requested (no status/paused change)", () => {
    expect(OPTIMISTIC_PATCH.flatten).toEqual({ flatten_requested: true });
    expect(OPTIMISTIC_PATCH.flatten).not.toHaveProperty("paused");
    expect(OPTIMISTIC_PATCH.flatten).not.toHaveProperty("status");
  });
  test("stop is terminal (cancelled)", () => {
    expect(OPTIMISTIC_PATCH.stop).toEqual({ status: "cancelled" });
  });
});

describe("patchRunInList", () => {
  test("patches only the matching run, returns a fresh array", () => {
    const list = [mkRun({ run_id: "a" }), mkRun({ run_id: "b" })];
    const next = patchRunInList(list, "b", OPTIMISTIC_PATCH.pause);
    expect(next).not.toBe(list);
    expect(next?.find((r) => r.run_id === "a")?.paused).toBeUndefined();
    expect(next?.find((r) => r.run_id === "b")?.paused).toBe(true);
  });
  test("returns same ref when no run matches (no needless re-render)", () => {
    const list = [mkRun({ run_id: "a" })];
    expect(patchRunInList(list, "zzz", OPTIMISTIC_PATCH.pause)).toBe(list);
  });
  test("undefined cache is a safe no-op", () => {
    expect(patchRunInList(undefined, "a", OPTIMISTIC_PATCH.pause)).toBeUndefined();
  });
  test("stop patch flips status to cancelled (drives STOPPED pill)", () => {
    const list = [mkRun({ run_id: "a", status: "running" })];
    const next = patchRunInList(list, "a", OPTIMISTIC_PATCH.stop);
    expect(next?.[0]?.status).toBe("cancelled");
  });
});

describe("reconcileFromRunSummary", () => {
  test("eval RunSummary.id maps to AgentRunSummary.run_id and wins", () => {
    const list = [mkRun({ run_id: "run_1", paused: true, flatten_requested: true })];
    const next = reconcileFromRunSummary(
      list,
      mkEval({ id: "run_1", paused: false, flatten_requested: false, status: "running" }),
    );
    expect(next?.[0]?.paused).toBe(false);
    expect(next?.[0]?.flatten_requested).toBe(false);
    expect(next?.[0]?.status).toBe("running");
  });
  test("preserves non-authoritative fields (objective, counts)", () => {
    const list = [mkRun({ run_id: "run_1", objective: "Keep me", tool_call_count: 9 })];
    const next = reconcileFromRunSummary(list, mkEval({ id: "run_1", paused: true }));
    expect(next?.[0]?.objective).toBe("Keep me");
    expect(next?.[0]?.tool_call_count).toBe(9);
    expect(next?.[0]?.paused).toBe(true);
  });
  test("cancel reconciles status to cancelled", () => {
    const list = [mkRun({ run_id: "run_1", status: "running" })];
    const next = reconcileFromRunSummary(list, mkEval({ id: "run_1", status: "cancelled" }));
    expect(next?.[0]?.status).toBe("cancelled");
  });
  test("no matching id leaves the list ref untouched", () => {
    const list = [mkRun({ run_id: "run_1" })];
    expect(reconcileFromRunSummary(list, mkEval({ id: "other" }))).toBe(list);
  });
});
