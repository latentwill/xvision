import { describe, expect, it } from "vitest";

import type { RunSummary } from "@/api/types.gen";
import {
  alignFieldSeries,
  drawdownFromEquity,
  evalThroughput,
  fieldRunSeries,
  formatRelativeTime,
  holdCompareSeries,
  isChartableRun,
  latestEvaluatedStrategyRuns,
  latestCompletionStamp,
  normalizePulseView,
  pickHeroRun,
  PULSE_VIEWS,
  pulseChartSeries,
  recentMetricSeries,
  tradeMarkersFromPayload,
} from "./pulse";
import type { RunChartPayload } from "@/api/types.gen";

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
    unrealized_pnl_usd: null,
    skipped_dispatches: 0,
    delayed_decisions: 0,
    forced_cancels: 0,
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

describe("latestEvaluatedStrategyRuns", () => {
  it("returns the newest chartable eval per strategy, capped at five", () => {
    const rows = [
      run({ id: "alpha-old", agent_id: "alpha", completed_at: "2026-06-10T01:00:00Z" }),
      run({ id: "alpha-new", agent_id: "alpha", completed_at: "2026-06-10T08:00:00Z" }),
      run({ id: "beta", agent_id: "beta", completed_at: "2026-06-10T07:00:00Z" }),
      run({ id: "gamma", agent_id: "gamma", completed_at: "2026-06-10T06:00:00Z" }),
      run({ id: "delta", agent_id: "delta", completed_at: "2026-06-10T05:00:00Z" }),
      run({ id: "epsilon", agent_id: "epsilon", completed_at: "2026-06-10T04:00:00Z" }),
      run({ id: "zeta", agent_id: "zeta", completed_at: "2026-06-10T03:00:00Z" }),
      run({ id: "live", agent_id: "live", mode: "live", completed_at: "2026-06-10T09:00:00Z" }),
      run({ id: "running", agent_id: "running", status: "running", completed_at: "2026-06-10T10:00:00Z" }),
    ];

    expect(latestEvaluatedStrategyRuns(rows).map((r) => r.id)).toEqual([
      "alpha-new",
      "beta",
      "gamma",
      "delta",
      "epsilon",
    ]);
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

describe("normalizePulseView", () => {
  it("accepts every known view", () => {
    for (const v of PULSE_VIEWS) expect(normalizePulseView(v)).toBe(v);
  });
  it("falls back to return for unknown/null", () => {
    expect(normalizePulseView(null)).toBe("return");
    expect(normalizePulseView("bogus")).toBe("return");
  });
});

describe("fieldRunSeries", () => {
  const eq = (t: number, e: number) => ({ time: t, equity_usd: e });

  it("normalizes to elapsed fraction and return pct", () => {
    const s = fieldRunSeries("r1", "Alpha", [eq(100, 100_000), eq(150, 110_000), eq(200, 99_000)]);
    expect(s).not.toBeNull();
    expect(s!.fraction).toEqual([0, 0.5, 1]);
    expect(s!.returnPct[0]).toBeCloseTo(0);
    expect(s!.returnPct[1]).toBeCloseTo(10);
    expect(s!.returnPct[2]).toBeCloseTo(-1);
  });

  it("rejects degenerate series", () => {
    expect(fieldRunSeries("r1", "x", [])).toBeNull();
    expect(fieldRunSeries("r1", "x", [eq(100, 100_000)])).toBeNull();
    expect(fieldRunSeries("r1", "x", [eq(100, 0), eq(200, 5)])).toBeNull(); // zero base
    expect(fieldRunSeries("r1", "x", [eq(100, 1), eq(100, 2)])).toBeNull(); // zero span
  });
});

describe("alignFieldSeries", () => {
  it("unions fractions and gaps non-shared samples with null", () => {
    const a = { runId: "a", label: "A", fraction: [0, 1], returnPct: [0, 4] };
    const b = { runId: "b", label: "B", fraction: [0, 0.5, 1], returnPct: [0, 1, 2] };
    const { x, ys } = alignFieldSeries([a, b]);
    expect(x).toEqual([0, 0.5, 1]);
    expect(ys[0]).toEqual([0, null, 4]);
    expect(ys[1]).toEqual([0, 1, 2]);
  });
});

describe("holdCompareSeries", () => {
  it("normalizes both curves to return pct on the shared axis", () => {
    const equity = [
      { time: 1, value: 0 },
      { time: 2, value: 5 },
    ];
    const baseline = [
      { time: 1, equity_usd: 100_000 },
      { time: 2, equity_usd: 120_000 },
    ];
    const s = holdCompareSeries(equity, baseline);
    expect(s.time).toEqual([1, 2]);
    expect(s.strategy).toEqual([0, 5]);
    expect(s.hold[0]).toBeCloseTo(0);
    expect(s.hold[1]).toBeCloseTo(20);
  });

  it("gaps baseline timestamps missing from the equity axis", () => {
    const equity = [
      { time: 1, value: 0 },
      { time: 2, value: 5 },
    ];
    const baseline = [{ time: 1, equity_usd: 100_000 }];
    const s = holdCompareSeries(equity, baseline);
    expect(s.hold).toEqual([0, null]);
  });
});

describe("tradeMarkersFromPayload (QA #1 equity B/S markers)", () => {
  function payloadWithTrades(
    trades: Array<{ time: number; side: "Buy" | "Sell"; price: number }>,
  ): RunChartPayload {
    return {
      markers: {
        trades: trades.map((t) => ({
          time: t.time,
          side: t.side,
          price: t.price,
          size: 1,
          fee: 0,
          pnl_realized: null,
          decision_index: 0,
          justification: null,
        })),
        vetoes: [],
        holds: [],
      },
    } as unknown as RunChartPayload;
  }

  it("maps Buy/Sell trades to buy/sell V2 markers with time + price", () => {
    const out = tradeMarkersFromPayload(
      payloadWithTrades([
        { time: 1000, side: "Buy", price: 42 },
        { time: 2000, side: "Sell", price: 43 },
      ]),
    );
    expect(out).toEqual([
      { kind: "buy", time: 1000, price: 42 },
      { kind: "sell", time: 2000, price: 43 },
    ]);
  });

  it("returns [] for undefined payload or no trades", () => {
    expect(tradeMarkersFromPayload(undefined)).toEqual([]);
    expect(tradeMarkersFromPayload(payloadWithTrades([]))).toEqual([]);
  });
});
