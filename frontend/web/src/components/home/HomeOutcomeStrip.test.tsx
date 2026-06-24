import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";

import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import {
  HomeOutcomeStrip,
  latestCompletedRunsByStrategy,
  median,
} from "./HomeOutcomeStrip";

// ─── Fixture helpers (mirror StrategyOutcomesList.test.tsx) ──────────────────

function makeStrategy(id: string, name: string): StrategyListItem {
  return {
    agent_id: id,
    display_name: name,
    template: "default",
    decision_cadence_minutes: 60,
  };
}

function makeRun(
  id: string,
  strategyId: string,
  opts: {
    status?: string;
    sharpe?: number | null;
    max_drawdown_pct?: number | null;
    total_return_pct?: number | null;
    completed_at?: string | null;
  } = {},
): RunSummary {
  return {
    id,
    agent_id: strategyId,
    scenario_id: "scenario-1",
    strategy: { id: strategyId, display_name: "Strategy " + strategyId },
    scenario: null,
    mode: "backtest",
    status: opts.status ?? "completed",
    started_at: "2026-01-01T00:00:00Z",
    completed_at:
      opts.completed_at !== undefined ? opts.completed_at : "2026-01-01T01:00:00Z",
    sharpe: opts.sharpe !== undefined ? opts.sharpe : 1.2,
    max_drawdown_pct:
      opts.max_drawdown_pct !== undefined ? opts.max_drawdown_pct : 5.0,
    total_return_pct:
      opts.total_return_pct !== undefined ? opts.total_return_pct : 8.0,
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
  };
}

function renderStrip(strategies: StrategyListItem[], runs: RunSummary[]) {
  return render(
    <MemoryRouter>
      <HomeOutcomeStrip strategies={strategies} runs={runs} />
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
});

// ─── Pure helpers ────────────────────────────────────────────────────────────

describe("latestCompletedRunsByStrategy", () => {
  it("keeps the most recent completed run per strategy and ignores in-flight", () => {
    const runs = [
      makeRun("r1", "s1", { completed_at: "2026-01-01T00:00:00Z", total_return_pct: 5 }),
      makeRun("r2", "s1", { completed_at: "2026-02-01T00:00:00Z", total_return_pct: 9 }),
      makeRun("r3", "s1", { status: "running", completed_at: null }),
      makeRun("r4", "s2", { completed_at: "2026-03-01T00:00:00Z", total_return_pct: 1 }),
    ];

    const latest = latestCompletedRunsByStrategy(runs);

    expect(latest).toHaveLength(2);
    const s1 = latest.find((r) => r.strategy?.id === "s1");
    expect(s1?.id).toBe("r2");
  });

  it("falls back to agent_id when the summary has no strategy metadata", () => {
    // /api/eval/runs (list) never enriches run.strategy — only the detail
    // endpoint does. agent_id carries the same strategy id on the wire.
    const unenriched = makeRun("r1", "s1");
    unenriched.strategy = null;
    const latest = latestCompletedRunsByStrategy([unenriched]);
    expect(latest).toHaveLength(1);
    expect(latest[0]?.id).toBe("r1");
  });

  it("skips runs with neither strategy metadata nor agent_id", () => {
    const orphan = makeRun("r1", "");
    orphan.strategy = null;
    expect(latestCompletedRunsByStrategy([orphan])).toHaveLength(0);
  });
});

describe("median", () => {
  it("returns the middle value for odd-length input", () => {
    expect(median([3, 1, 2])).toBe(2);
  });
  it("averages the two middle values for even-length input", () => {
    expect(median([1, 2, 3, 4])).toBe(2.5);
  });
  it("returns null for empty input and filters non-finite values", () => {
    expect(median([])).toBeNull();
    expect(median([Number.NaN, 1, 3])).toBe(2);
  });
});

// ─── Component ───────────────────────────────────────────────────────────────

describe("HomeOutcomeStrip", () => {
  it("renders eval and optimizer outcome labels without live-money claims", () => {
    const strategies = [makeStrategy("s1", "Alpha")];
    const runs = [makeRun("r1", "s1", { total_return_pct: 9, sharpe: 1.3 })];

    renderStrip(strategies, runs);

    expect(screen.getByText(/completed evals/i)).toBeInTheDocument();
    expect(screen.queryByText(/PnL/i)).toBeNull();
    expect(screen.queryByText(/deployed capital/i)).toBeNull();
    expect(screen.queryByText(/real money/i)).toBeNull();
  });

  it("derives completed count, in-flight count, best return and median Sharpe", () => {
    const strategies = [
      makeStrategy("s1", "Alpha"),
      makeStrategy("s2", "Bravo"),
      makeStrategy("s3", "Charlie"),
    ];
    const runs = [
      makeRun("r1a", "s1", { completed_at: "2026-02-01T00:00:00Z", total_return_pct: 10, sharpe: 1.5 }),
      makeRun("r1b", "s1", { completed_at: "2026-01-01T00:00:00Z", total_return_pct: 5, sharpe: 1.0 }),
      makeRun("r2", "s2", { completed_at: "2026-02-01T00:00:00Z", total_return_pct: 20, sharpe: 2.0 }),
      makeRun("r3", "s3", { completed_at: "2026-02-01T00:00:00Z", total_return_pct: -5, sharpe: 0.5 }),
      makeRun("r4", "s1", { status: "queued", completed_at: null }),
      makeRun("r5", "s2", { status: "running", completed_at: null }),
    ];

    renderStrip(strategies, runs);

    // 4 completed runs (r1a, r1b, r2, r3)
    expect(screen.getByTestId("home-outcome-completed")).toHaveTextContent("4");
    // 2 in-flight (r4 queued, r5 running)
    expect(screen.getByTestId("home-outcome-inflight")).toHaveTextContent("2");
    // best latest-completed return = 20
    expect(screen.getByTestId("home-outcome-best-return")).toHaveTextContent("20");
    // median of latest Sharpes [1.5, 2.0, 0.5] = 1.5
    expect(screen.getByTestId("home-outcome-median-sharpe")).toHaveTextContent("1.5");
  });

  it("shows dashes for return/Sharpe when there are no completed evals", () => {
    renderStrip([makeStrategy("s1", "Alpha")], [
      makeRun("r1", "s1", { status: "queued", completed_at: null }),
    ]);

    expect(screen.getByTestId("home-outcome-completed")).toHaveTextContent("0");
    expect(screen.getByTestId("home-outcome-best-return")).toHaveTextContent("—");
    expect(screen.getByTestId("home-outcome-median-sharpe")).toHaveTextContent("—");
  });
});
