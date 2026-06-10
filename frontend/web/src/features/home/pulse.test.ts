import { describe, expect, it } from "vitest";

import type { RunSummary } from "@/api/types.gen";
import {
  drawdownFromEquity,
  evalThroughput,
  formatRelativeTime,
  isChartableRun,
  latestCompletionStamp,
  pickHeroRun,
  pulseChartSeries,
  recentMetricSeries,
} from "./pulse";

function run(over: Partial<RunSummary>): RunSummary {
  return {
    id: "run-1",
    agent_id: "strat-1",
    scenario_id: "scn-1",
    strategy: null,
    scenario: null,
    mode: "backtest",
    status: "completed",
    started_at: "2026-06-10T09:00:00Z",
    completed_at: "2026-06-10T10:00:00Z",
    sharpe: 1.2,
    max_drawdown_pct: 0.5,
    total_return_pct: 0.1,
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
  };
}

describe("drawdownFromEquity", () => {
  it("computes running-max minus equity as negative values", () => {
    const dd = drawdownFromEquity([
      { time: 1, value: 0 },
      { time: 2, value: 2 },
      { time: 3, value: 1 },
      { time: 4, value: 3 },
      { time: 5, value: 0.5 },
    ]);
    expect(dd.map((p) => p.value)).toEqual([0, 0, -1, 0, -2.5]);
  });

  it("skips non-finite samples without advancing the running max", () => {
    const dd = drawdownFromEquity([
      { time: 1, value: 1 },
      { time: 2, value: NaN },
      { time: 3, value: 0 },
    ]);
    expect(dd).toEqual([
      { time: 1, value: 0 },
      { time: 3, value: -1 },
    ]);
  });

  it("returns empty for empty input", () => {
    expect(drawdownFromEquity([])).toEqual([]);
  });
});

describe("pulseChartSeries", () => {
  it("keeps columns aligned by emitting null gaps for non-finite equity", () => {
    const s = pulseChartSeries([
      { time: 1, value: 1 },
      { time: 2, value: NaN },
      { time: 3, value: 0.5 },
    ]);
    expect(s.time).toEqual([1, 2, 3]);
    expect(s.equity).toEqual([1, null, 0.5]);
    expect(s.drawdown).toEqual([0, null, -0.5]);
  });

  it("drops samples with non-finite time entirely", () => {
    const s = pulseChartSeries([
      { time: NaN, value: 1 },
      { time: 2, value: 1 },
    ]);
    expect(s.time).toEqual([2]);
    expect(s.equity).toEqual([1]);
  });
});

describe("pickHeroRun", () => {
  it("prefers the newest completed chartable run WITH metrics", () => {
    const newestNoMetrics = run({
      id: "no-metrics",
      completed_at: "2026-06-10T12:00:00Z",
      total_return_pct: null,
    });
    const olderWithMetrics = run({
      id: "with-metrics",
      completed_at: "2026-06-10T11:00:00Z",
      total_return_pct: -0.05,
    });
    expect(pickHeroRun([newestNoMetrics, olderWithMetrics])?.id).toBe(
      "with-metrics",
    );
  });

  it("falls back to the newest completed chartable run without metrics", () => {
    const a = run({ id: "a", total_return_pct: null, completed_at: "2026-06-10T10:00:00Z" });
    const b = run({ id: "b", total_return_pct: null, completed_at: "2026-06-10T11:00:00Z" });
    expect(pickHeroRun([a, b])?.id).toBe("b");
  });

  it("ignores live-mode, scenario-less, and non-completed runs", () => {
    const live = run({ id: "live", mode: "live" });
    const noScenario = run({ id: "no-scn", scenario_id: " " });
    const running = run({ id: "running", status: "running" });
    expect(pickHeroRun([live, noScenario, running])).toBeNull();
  });
});

describe("isChartableRun", () => {
  it("requires non-live mode and a scenario id", () => {
    expect(isChartableRun(run({}))).toBe(true);
    expect(isChartableRun(run({ mode: "live" }))).toBe(false);
    expect(isChartableRun(run({ scenario_id: "" }))).toBe(false);
  });
});

describe("evalThroughput", () => {
  it("counts completed and in-flight runs", () => {
    const t = evalThroughput([
      run({ status: "completed" }),
      run({ status: "completed" }),
      run({ status: "running" }),
      run({ status: "queued" }),
      run({ status: "failed" }),
    ]);
    expect(t).toEqual({ completed: 2, inflight: 2 });
  });
});

describe("recentMetricSeries", () => {
  it("returns completed-run metrics oldest→newest, skipping nulls", () => {
    const series = recentMetricSeries(
      [
        run({ id: "1", completed_at: "2026-06-10T03:00:00Z", total_return_pct: 3 }),
        run({ id: "2", completed_at: "2026-06-10T01:00:00Z", total_return_pct: 1 }),
        run({ id: "3", completed_at: "2026-06-10T02:00:00Z", total_return_pct: null }),
        run({ id: "4", completed_at: "2026-06-10T04:00:00Z", status: "failed", total_return_pct: 9 }),
      ],
      (r) => r.total_return_pct,
    );
    expect(series).toEqual([1, 3]);
  });

  it("caps at n, keeping the newest", () => {
    const runs = [1, 2, 3, 4].map((i) =>
      run({ id: `${i}`, completed_at: `2026-06-10T0${i}:00:00Z`, total_return_pct: i }),
    );
    expect(recentMetricSeries(runs, (r) => r.total_return_pct, 2)).toEqual([3, 4]);
  });
});

describe("latestCompletionStamp", () => {
  it("returns the newest completed_at among completed runs", () => {
    expect(
      latestCompletionStamp([
        run({ completed_at: "2026-06-10T01:00:00Z" }),
        run({ completed_at: "2026-06-10T05:00:00Z" }),
        run({ status: "running", completed_at: "2026-06-10T09:00:00Z" }),
      ]),
    ).toBe("2026-06-10T05:00:00Z");
  });

  it("returns null when nothing completed", () => {
    expect(latestCompletionStamp([run({ status: "running" })])).toBeNull();
  });
});

describe("formatRelativeTime", () => {
  const now = new Date("2026-06-10T12:00:00Z").getTime();

  it("formats seconds/minutes/hours/days", () => {
    expect(formatRelativeTime("2026-06-10T11:59:30Z", now)).toBe("just now");
    expect(formatRelativeTime("2026-06-10T11:45:00Z", now)).toBe("15m ago");
    expect(formatRelativeTime("2026-06-10T09:00:00Z", now)).toBe("3h ago");
    expect(formatRelativeTime("2026-06-08T12:00:00Z", now)).toBe("2d ago");
  });

  it("returns empty string for null/invalid input", () => {
    expect(formatRelativeTime(null, now)).toBe("");
    expect(formatRelativeTime("not-a-date", now)).toBe("");
  });
});
