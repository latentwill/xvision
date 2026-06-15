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
  classifyRunLiveness,
  deriveStripStatus,
  filterRunsForStrip,
  isLiveRun,
  isStaleRun,
  livenessCounts,
  pickDefaultRun,
  pickDefaultLiveRun,
  stripFilterBucket,
  stripFilterCounts,
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

/** A genuinely-live-money run: backend says so AND the parent is non-terminal. */
function mkLiveRun(over: Partial<AgentRunSummary> = {}): AgentRunSummary {
  return mkRun({
    is_live_money: true,
    eval_mode: "live",
    eval_run_status: "running",
    ...over,
  });
}

describe("deriveStripStatus", () => {
  test("running + not paused -> ACTIVE", () => {
    expect(deriveStripStatus(mkLiveRun({ status: "running" }))).toBe("ACTIVE");
  });
  test("running + paused -> PAUSED", () => {
    expect(
      deriveStripStatus(mkLiveRun({ status: "running", paused: true })),
    ).toBe("PAUSED");
  });
  test("cancelled -> STOPPED (even if paused flag stale)", () => {
    expect(
      deriveStripStatus(mkLiveRun({ status: "cancelled", paused: true })),
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
      expect(deriveStripStatus(mkLiveRun({ status: s }))).toBe("STOPPED");
    }
  });
  test("running but parent eval run terminal -> STALE, never ACTIVE", () => {
    for (const parent of ["completed", "failed", "cancelled"] as const) {
      expect(
        deriveStripStatus(
          mkRun({
            status: "running",
            is_live_money: false,
            eval_mode: "live",
            eval_run_status: parent,
          }),
        ),
      ).toBe("STALE");
    }
  });
  test("stale wins over paused (orphan with stale paused flag)", () => {
    expect(
      deriveStripStatus(
        mkRun({
          status: "running",
          paused: true,
          eval_run_status: "failed",
        }),
      ),
    ).toBe("STALE");
  });
});

describe("isLiveRun (live money)", () => {
  test("requires the backend live-money signal", () => {
    // Non-terminal but NOT live money (backtest child / no parent) -> false.
    expect(isLiveRun(mkRun({ status: "running" }))).toBe(false);
    expect(isLiveRun(mkRun({ status: "queued" }))).toBe(false);
    // Live money + non-terminal -> true.
    expect(isLiveRun(mkLiveRun({ status: "running" }))).toBe(true);
    expect(isLiveRun(mkLiveRun({ status: "queued" }))).toBe(true);
  });
  test("terminal runs are never live", () => {
    expect(isLiveRun(mkLiveRun({ status: "completed" }))).toBe(false);
    expect(isLiveRun(mkLiveRun({ status: "interrupted" }))).toBe(false);
  });
  test("defensive: live-money flag with terminal parent is NOT live", () => {
    expect(
      isLiveRun(
        mkLiveRun({ is_live_money: true, eval_run_status: "completed" }),
      ),
    ).toBe(false);
  });
});

describe("isStaleRun", () => {
  test("running agent run whose parent eval run is terminal -> stale", () => {
    expect(
      isStaleRun(mkRun({ status: "running", eval_run_status: "failed" })),
    ).toBe(true);
  });
  test("terminal agent run is never stale (it is just done)", () => {
    expect(
      isStaleRun(mkRun({ status: "interrupted", eval_run_status: "failed" })),
    ).toBe(false);
  });
  test("running with non-terminal or absent parent -> not stale", () => {
    expect(
      isStaleRun(mkRun({ status: "running", eval_run_status: "running" })),
    ).toBe(false);
    expect(isStaleRun(mkRun({ status: "running" }))).toBe(false);
  });
});

describe("classifyRunLiveness", () => {
  test("live money + non-terminal parent -> live", () => {
    expect(classifyRunLiveness(mkLiveRun())).toBe("live");
  });
  test("non-terminal without live-money signal -> paper", () => {
    expect(classifyRunLiveness(mkRun({ status: "running" }))).toBe("paper");
    expect(
      classifyRunLiveness(
        mkRun({ status: "running", eval_mode: "backtest", eval_run_status: "running" }),
      ),
    ).toBe("paper");
  });
  test("orphaned running child of a terminal eval run -> stale", () => {
    expect(
      classifyRunLiveness(
        mkRun({ status: "running", eval_mode: "live", eval_run_status: "failed" }),
      ),
    ).toBe("stale");
  });
  test("terminal -> done", () => {
    expect(classifyRunLiveness(mkLiveRun({ status: "completed" }))).toBe("done");
  });
});

describe("livenessCounts", () => {
  test("splits live into active/paused and counts paper + stale; done excluded", () => {
    const counts = livenessCounts([
      mkLiveRun({ run_id: "live-a" }),
      mkLiveRun({ run_id: "live-b", paused: true }),
      mkRun({ run_id: "paper", status: "running" }),
      mkRun({
        run_id: "stale",
        status: "running",
        eval_mode: "live",
        eval_run_status: "failed",
      }),
      mkLiveRun({ run_id: "done", status: "completed" }),
    ]);
    expect(counts).toEqual({
      liveActive: 1,
      livePaused: 1,
      paper: 1,
      stale: 1,
    });
  });

  test("empty population -> all zeros", () => {
    expect(livenessCounts([])).toEqual({
      liveActive: 0,
      livePaused: 0,
      paper: 0,
      stale: 0,
    });
  });
});

describe("pickDefaultRun", () => {
  test("empty list -> null", () => {
    expect(pickDefaultRun([])).toBeNull();
  });
  test("picks most recently started LIVE-MONEY run", () => {
    const a = mkLiveRun({ run_id: "a", started_at: "2026-06-09T09:00:00Z" });
    const b = mkLiveRun({ run_id: "b", started_at: "2026-06-09T11:00:00Z" });
    const c = mkLiveRun({ run_id: "c", started_at: "2026-06-09T10:00:00Z" });
    expect(pickDefaultRun([a, b, c])?.run_id).toBe("b");
  });
  test("prefers a live-money run over a more-recent paper run", () => {
    const live = mkLiveRun({
      run_id: "live",
      started_at: "2026-06-09T09:00:00Z",
    });
    const paper = mkRun({
      run_id: "paper",
      status: "running",
      started_at: "2026-06-09T12:00:00Z",
    });
    expect(pickDefaultRun([paper, live])?.run_id).toBe("live");
  });
  test("prefers a live-money run over a more-recent terminal run", () => {
    const live = mkLiveRun({
      run_id: "live",
      status: "running",
      started_at: "2026-06-09T09:00:00Z",
    });
    const recentDone = mkLiveRun({
      run_id: "done",
      status: "completed",
      started_at: "2026-06-09T12:00:00Z",
    });
    expect(pickDefaultRun([recentDone, live])?.run_id).toBe("live");
  });
  test("never auto-selects a stale orphan over a paper run", () => {
    const stale = mkRun({
      run_id: "stale",
      status: "running",
      eval_run_status: "failed",
      started_at: "2026-06-09T12:00:00Z",
    });
    const paper = mkRun({
      run_id: "paper",
      status: "running",
      started_at: "2026-06-09T09:00:00Z",
    });
    expect(pickDefaultRun([stale, paper])?.run_id).toBe("paper");
  });
  test("falls back to most-recent run of any status when none live", () => {
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

describe("pickDefaultLiveRun", () => {
  test("empty list -> null", () => {
    expect(pickDefaultLiveRun([])).toBeNull();
  });
  test("picks most recently started LIVE-MONEY run", () => {
    const a = mkLiveRun({ run_id: "a", started_at: "2026-06-09T09:00:00Z" });
    const b = mkLiveRun({ run_id: "b", started_at: "2026-06-09T11:00:00Z" });
    expect(pickDefaultLiveRun([a, b])?.run_id).toBe("b");
  });
  test("returns null when only paper/backtest runs exist (no live fallback)", () => {
    const paper = mkRun({
      run_id: "paper",
      status: "running",
      started_at: "2026-06-09T12:00:00Z",
    });
    expect(pickDefaultLiveRun([paper])).toBeNull();
  });
  test("returns null when only stale orphans exist", () => {
    const stale = mkRun({
      run_id: "stale",
      status: "running",
      eval_run_status: "completed",
      started_at: "2026-06-09T12:00:00Z",
    });
    expect(pickDefaultLiveRun([stale])).toBeNull();
  });
  test("returns null when only terminal runs exist", () => {
    const done = mkRun({
      run_id: "done",
      status: "completed",
      started_at: "2026-06-09T12:00:00Z",
    });
    const cancelled = mkRun({
      run_id: "cancelled",
      status: "cancelled",
      started_at: "2026-06-09T13:00:00Z",
    });
    expect(pickDefaultLiveRun([done, cancelled])).toBeNull();
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

describe("strip filter chips (stripFilterBucket / filterRunsForStrip / stripFilterCounts)", () => {
  test("live + not paused -> LIVE bucket", () => {
    expect(stripFilterBucket(mkLiveRun())).toBe("LIVE");
  });
  test("live + paused -> PAUSED bucket", () => {
    expect(stripFilterBucket(mkLiveRun({ paused: true }))).toBe("PAUSED");
  });
  test("terminal runs -> STOPPED bucket", () => {
    for (const s of [
      "completed",
      "failed",
      "cancelled",
      "interrupted",
      "agent_failure",
    ] as const) {
      expect(stripFilterBucket(mkLiveRun({ status: s }))).toBe("STOPPED");
    }
  });
  test("stale orphan (parent eval terminal) -> STOPPED, never LIVE", () => {
    const stale = mkLiveRun({ eval_run_status: "completed" });
    expect(stripFilterBucket(stale)).toBe("STOPPED");
  });
  test("running backtest/paper run -> STOPPED, never LIVE even when status=running", () => {
    const paper = mkRun({ status: "running", eval_mode: "backtest" });
    expect(stripFilterBucket(paper)).toBe("STOPPED");
    // Parentless orphan with no live-money signal.
    expect(stripFilterBucket(mkRun({ status: "running" }))).toBe("STOPPED");
    // is_live_money=false explicitly.
    expect(
      stripFilterBucket(mkRun({ status: "running", is_live_money: false })),
    ).toBe("STOPPED");
  });
  test("paused flag on a NON-live run does not produce PAUSED", () => {
    expect(stripFilterBucket(mkRun({ paused: true }))).toBe("STOPPED");
  });

  test("filterRunsForStrip: ALL passes everything through unchanged", () => {
    const runs = [mkLiveRun({ run_id: "a" }), mkRun({ run_id: "b" })];
    expect(filterRunsForStrip(runs, "ALL")).toEqual(runs);
  });
  test("filterRunsForStrip: LIVE keeps only genuinely-live active runs", () => {
    const live = mkLiveRun({ run_id: "live" });
    const paused = mkLiveRun({ run_id: "paused", paused: true });
    const backtest = mkRun({ run_id: "bt", eval_mode: "backtest" });
    const dead = mkLiveRun({ run_id: "dead", status: "completed" });
    expect(
      filterRunsForStrip([live, paused, backtest, dead], "LIVE").map(
        (r) => r.run_id,
      ),
    ).toEqual(["live"]);
  });
  test("filterRunsForStrip: PAUSED / STOPPED buckets", () => {
    const live = mkLiveRun({ run_id: "live" });
    const paused = mkLiveRun({ run_id: "paused", paused: true });
    const stale = mkLiveRun({ run_id: "stale", eval_run_status: "cancelled" });
    const dead = mkLiveRun({ run_id: "dead", status: "failed" });
    const all = [live, paused, stale, dead];
    expect(filterRunsForStrip(all, "PAUSED").map((r) => r.run_id)).toEqual([
      "paused",
    ]);
    expect(filterRunsForStrip(all, "STOPPED").map((r) => r.run_id)).toEqual([
      "stale",
      "dead",
    ]);
  });

  test("stripFilterCounts: ALL is total; buckets partition it", () => {
    const runs = [
      mkLiveRun({ run_id: "a" }),
      mkLiveRun({ run_id: "b" }),
      mkLiveRun({ run_id: "c", paused: true }),
      mkRun({ run_id: "d", eval_mode: "backtest" }),
      mkLiveRun({ run_id: "e", status: "completed" }),
    ];
    const counts = stripFilterCounts(runs);
    expect(counts).toEqual({ ALL: 5, LIVE: 2, PAUSED: 1, STOPPED: 2 });
    expect(counts.LIVE + counts.PAUSED + counts.STOPPED).toBe(counts.ALL);
  });
  test("stripFilterCounts: empty list", () => {
    expect(stripFilterCounts([])).toEqual({
      ALL: 0,
      LIVE: 0,
      PAUSED: 0,
      STOPPED: 0,
    });
  });
});
