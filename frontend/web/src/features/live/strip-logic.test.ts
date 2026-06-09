import { describe, expect, test, beforeEach } from "vitest";
import type { AgentRunSummary } from "@/api/types-agent-runs";
import {
  computeStripMetric,
  DEFAULT_STRIP_METRIC,
  isStripMetricId,
  loadStripMetric,
  saveStripMetric,
  STRIP_METRIC_STORAGE_KEY,
} from "./strip-metrics";
import {
  deriveStripStatus,
  isLiveRun,
  pickDefaultRun,
} from "./strip-status";

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

describe("deriveStripStatus", () => {
  test("running + not paused -> ACTIVE", () => {
    expect(deriveStripStatus(mkRun({ status: "running" }))).toBe("ACTIVE");
  });
  test("running + paused -> PAUSED", () => {
    expect(deriveStripStatus(mkRun({ status: "running", paused: true }))).toBe(
      "PAUSED",
    );
  });
  test("cancelled -> STOPPED (even if paused flag stale)", () => {
    expect(
      deriveStripStatus(mkRun({ status: "cancelled", paused: true })),
    ).toBe("STOPPED");
  });
  test("each terminal status -> STOPPED", () => {
    for (const s of [
      "completed",
      "failed",
      "cancelled",
      "interrupted",
      "agent_failure",
    ] as const) {
      expect(deriveStripStatus(mkRun({ status: s }))).toBe("STOPPED");
    }
  });
});

describe("isLiveRun", () => {
  test("running/queued are live; terminal are not", () => {
    expect(isLiveRun(mkRun({ status: "running" }))).toBe(true);
    expect(isLiveRun(mkRun({ status: "queued" }))).toBe(true);
    expect(isLiveRun(mkRun({ status: "completed" }))).toBe(false);
  });
});

describe("pickDefaultRun", () => {
  test("empty list -> null", () => {
    expect(pickDefaultRun([])).toBeNull();
  });
  test("picks most recently started LIVE run", () => {
    const a = mkRun({ run_id: "a", started_at: "2026-06-09T09:00:00Z" });
    const b = mkRun({ run_id: "b", started_at: "2026-06-09T11:00:00Z" });
    const c = mkRun({ run_id: "c", started_at: "2026-06-09T10:00:00Z" });
    expect(pickDefaultRun([a, b, c])?.run_id).toBe("b");
  });
  test("prefers a live run over a more-recent terminal run", () => {
    const live = mkRun({
      run_id: "live",
      status: "running",
      started_at: "2026-06-09T09:00:00Z",
    });
    const recentDone = mkRun({
      run_id: "done",
      status: "completed",
      started_at: "2026-06-09T12:00:00Z",
    });
    expect(pickDefaultRun([recentDone, live])?.run_id).toBe("live");
  });
  test("falls back to most-recent terminal when none live", () => {
    const older = mkRun({
      run_id: "older",
      status: "completed",
      started_at: "2026-06-09T08:00:00Z",
    });
    const newer = mkRun({
      run_id: "newer",
      status: "cancelled",
      started_at: "2026-06-09T12:00:00Z",
    });
    expect(pickDefaultRun([older, newer])?.run_id).toBe("newer");
  });
});

describe("computeStripMetric", () => {
  test("trades_today uses tool_call_count", () => {
    const v = computeStripMetric("trades_today", mkRun({ tool_call_count: 4 }));
    expect(v).toMatchObject({ text: "4", derived: true });
  });
  test("decisions_today uses model_call_count", () => {
    const v = computeStripMetric(
      "decisions_today",
      mkRun({ model_call_count: 7 }),
    );
    expect(v).toMatchObject({ text: "7", derived: true });
  });
  test("run_time from started_at + now", () => {
    const run = mkRun({ started_at: "2026-06-09T10:00:00Z", duration_ms: null });
    const now = new Date("2026-06-09T10:05:30Z").getTime();
    const v = computeStripMetric("run_time", run, now);
    expect(v.derived).toBe(true);
    expect(v.text).toBe("5m");
  });
  test("run_time prefers duration_ms when present", () => {
    const v = computeStripMetric("run_time", mkRun({ duration_ms: 90_000 }));
    expect(v.text).toBe("1m");
  });
  test("financial metrics return dash placeholder (not faked)", () => {
    for (const id of [
      "daily_pnl_usd",
      "daily_pnl_pct",
      "unrealized_pnl",
      "current_equity",
      "sharpe",
      "max_drawdown",
    ] as const) {
      const v = computeStripMetric(id, mkRun());
      expect(v).toMatchObject({ text: "—", derived: false });
    }
  });
});

describe("strip metric persistence", () => {
  beforeEach(() => {
    localStorage.clear();
  });
  test("isStripMetricId guards unknown values", () => {
    expect(isStripMetricId("sharpe")).toBe(true);
    expect(isStripMetricId("bogus")).toBe(false);
    expect(isStripMetricId(null)).toBe(false);
  });
  test("loadStripMetric defaults when unset/invalid", () => {
    expect(loadStripMetric()).toBe(DEFAULT_STRIP_METRIC);
    localStorage.setItem(STRIP_METRIC_STORAGE_KEY, "bogus");
    expect(loadStripMetric()).toBe(DEFAULT_STRIP_METRIC);
  });
  test("save then load round-trips", () => {
    saveStripMetric("sharpe");
    expect(loadStripMetric()).toBe("sharpe");
    expect(localStorage.getItem(STRIP_METRIC_STORAGE_KEY)).toBe("sharpe");
  });
});
