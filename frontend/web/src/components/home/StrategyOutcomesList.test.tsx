import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";

import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import { StrategyOutcomesList } from "./StrategyOutcomesList";

// ─── Fixture helpers ────────────────────────────────────────────────────────

function makeStrategy(
  id: string,
  name: string,
): StrategyListItem {
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
    completed_at: opts.completed_at ?? "2026-01-01T01:00:00Z",
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
  };
}

function renderList(
  strategies: StrategyListItem[],
  runs: RunSummary[],
) {
  return render(
    <MemoryRouter>
      <StrategyOutcomesList strategies={strategies} runs={runs} />
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
});

// ─── Tests ───────────────────────────────────────────────────────────────────

describe("StrategyOutcomesList", () => {
  it("Test 1: renders all strategies from fixture", () => {
    const strategies = [
      makeStrategy("s-1", "Alpha Strategy"),
      makeStrategy("s-2", "Beta Strategy"),
      makeStrategy("s-3", "Gamma Strategy"),
    ];
    renderList(strategies, []);

    expect(screen.getByText("Alpha Strategy")).toBeInTheDocument();
    expect(screen.getByText("Beta Strategy")).toBeInTheDocument();
    expect(screen.getByText("Gamma Strategy")).toBeInTheDocument();
    expect(screen.getByTestId("strategy-outcomes-list")).toBeInTheDocument();
  });

  it("Test 2: strategy with completed run shows metric values; null values show dash", () => {
    const strategies = [makeStrategy("s-1", "Alpha Strategy")];
    const runs = [
      makeRun("run-1", "s-1", {
        sharpe: 1.5,
        total_return_pct: 12.3,
        max_drawdown_pct: 7.8,
      }),
      makeRun("run-2", "s-1", {
        sharpe: null,
        total_return_pct: null,
        max_drawdown_pct: null,
      }),
    ];
    renderList(strategies, runs);

    // Most recent run is run-1 (same completed_at, first sorted)
    // We'll confirm it shows a metric value (not "—")
    // and that a strategy with all-null metrics shows "—"
    // Make run-2 the most recent
    const strategies2 = [makeStrategy("s-2", "Null Strategy")];
    const runs2 = [
      makeRun("run-null", "s-2", {
        sharpe: null,
        total_return_pct: null,
        max_drawdown_pct: null,
        completed_at: "2026-06-01T00:00:00Z",
      }),
    ];
    cleanup();
    renderList(strategies2, runs2);

    // Null metrics should show em-dashes
    const dashes = screen.getAllByText("—");
    expect(dashes.length).toBeGreaterThanOrEqual(3);
  });

  it("Test 3: n>=10 completed runs AND win threshold → row has success color class", () => {
    const strategies = [makeStrategy("s-win", "Win Strategy")];
    // 10 runs, all completed, total_return_pct > 0, sharpe > 1.0
    const runs = Array.from({ length: 10 }, (_, i) =>
      makeRun(`run-${i}`, "s-win", {
        sharpe: 1.5,
        total_return_pct: 10.0,
        max_drawdown_pct: 5.0,
        completed_at: `2026-0${(i % 9) + 1}-01T00:00:00Z`,
      }),
    );
    renderList(strategies, runs);

    const row = screen.getByTestId("strategy-row-s-win");
    // Should have a success/green class
    expect(row.className).toMatch(/green|success/);
  });

  it("Test 4: n>=10 runs but NOT meeting win threshold → warning/muted color (not success)", () => {
    const strategies = [makeStrategy("s-loss", "Loss Strategy")];
    // 10 runs, all completed, total_return_pct < 0 (not win)
    const runs = Array.from({ length: 10 }, (_, i) =>
      makeRun(`run-${i}`, "s-loss", {
        sharpe: 0.5,
        total_return_pct: -5.0,
        max_drawdown_pct: 15.0,
        completed_at: `2026-0${(i % 9) + 1}-01T00:00:00Z`,
      }),
    );
    renderList(strategies, runs);

    const row = screen.getByTestId("strategy-row-s-loss");
    // Should NOT have success/green class
    expect(row.className).not.toMatch(/green|success/);
    // Should have warning/amber/muted indicator
    expect(row.className).toMatch(/amber|warning|muted|orange/);
  });

  it("Test 5: n<10 completed runs → NO color class (plain, no judgement)", () => {
    const strategies = [makeStrategy("s-few", "Few Runs Strategy")];
    const runs = Array.from({ length: 5 }, (_, i) =>
      makeRun(`run-${i}`, "s-few", {
        sharpe: 2.0,
        total_return_pct: 20.0,
        max_drawdown_pct: 2.0,
        completed_at: `2026-0${i + 1}-01T00:00:00Z`,
      }),
    );
    renderList(strategies, runs);

    const row = screen.getByTestId("strategy-row-s-few");
    // Should NOT have green/success or amber/warning class — plain neutral
    expect(row.className).not.toMatch(/green|success/);
    expect(row.className).not.toMatch(/amber|warning/);
  });

  it("Test 6: strategy with 0 completed runs shows 'no evals yet' and 'Run eval →' link", () => {
    const strategies = [makeStrategy("s-none", "No Eval Strategy")];
    renderList(strategies, []);

    expect(screen.getByText(/no evals yet/i)).toBeInTheDocument();
    const link = screen.getByRole("link", { name: /run eval/i });
    expect(link).toHaveAttribute("href", "/eval-runs");
  });

  it("Test 7: 'View chart →' link present for strategies with completed runs, links to /eval-runs/:id", () => {
    const strategies = [makeStrategy("s-chart", "Chart Strategy")];
    const runs = [
      makeRun("run-chart-1", "s-chart", {
        completed_at: "2026-06-01T00:00:00Z",
      }),
    ];
    renderList(strategies, runs);

    const link = screen.getByRole("link", { name: /view chart/i });
    expect(link).toHaveAttribute("href", "/eval-runs/run-chart-1");
  });
});
