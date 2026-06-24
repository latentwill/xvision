import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";

import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import { StrategyLeaderboard } from "./StrategyLeaderboard";

function strategy(over: Partial<StrategyListItem>): StrategyListItem {
  return {
    agent_id: "strat-1",
    display_name: "Strategy One",
    template: "trend_follower",
    decision_cadence_minutes: 60,
    ...over,
  };
}

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
    max_drawdown_pct: 0.08,
    total_return_pct: 0.05,
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

function renderBoard(strategies: StrategyListItem[], runs: RunSummary[]) {
  return render(
    <MemoryRouter>
      <StrategyLeaderboard strategies={strategies} runs={runs} />
    </MemoryRouter>,
  );
}

describe("StrategyLeaderboard", () => {
  it("ranks evaluated strategies by latest return and links name → strategy, metrics → run", () => {
    renderBoard(
      [
        strategy({ agent_id: "a", display_name: "Alpha" }),
        strategy({ agent_id: "b", display_name: "Beta" }),
      ],
      [
        run({ id: "ra", agent_id: "a", total_return_pct: -0.1 }),
        run({ id: "rb", agent_id: "b", total_return_pct: 0.2 }),
      ],
    );
    const rows = screen.getAllByTestId(/leaderboard-row-/);
    expect(rows[0]).toHaveTextContent("Beta");
    expect(rows[1]).toHaveTextContent("Alpha");

    const beta = within(rows[0]);
    expect(beta.getByRole("link", { name: "Beta" })).toHaveAttribute(
      "href",
      "/strategies/b",
    );
    expect(beta.getByRole("link", { name: /latest eval/i })).toHaveAttribute(
      "href",
      "/eval-runs/rb",
    );
    expect(rows[0]).toHaveTextContent("+0.20%");
  });

  it("shows the low-sample chip for thin data and origin chip for optimizer strategies", () => {
    renderBoard(
      [
        strategy({
          agent_id: "opt",
          display_name: "Optimized",
          origin: "optimizer",
        }),
      ],
      [run({ id: "r1", agent_id: "opt" })],
    );
    expect(screen.getByTestId("low-sample-chip")).toHaveTextContent(
      /low n · 1 eval/i,
    );
    expect(screen.getByTestId("origin-chip")).toHaveTextContent("Optimizer");
  });

  it("renders the segmented coverage footer with awaiting/lineage counts", () => {
    renderBoard(
      [
        strategy({ agent_id: "user-pending", display_name: "Pending" }),
        strategy({
          agent_id: "opt-pending",
          display_name: "Lineage",
          origin: "optimizer",
        }),
        strategy({ agent_id: "done", display_name: "Done" }),
      ],
      [run({ id: "r1", agent_id: "done" })],
    );
    const footer = screen.getByTestId("eval-coverage-line");
    expect(footer).toHaveTextContent("1 user strategy awaiting first eval");
    expect(footer).toHaveTextContent(
      "1 optimizer-generated (evaluated in lineage)",
    );
  });

  it("renders the empty CTA when there are no completed evals", () => {
    renderBoard([strategy({})], []);
    expect(
      screen.getByText(/no completed evals on this page yet/i),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /run an eval/i })).toHaveAttribute(
      "href",
      "/eval-runs",
    );
  });

  it("renders the create-one CTA when there are no strategies", () => {
    renderBoard([], []);
    expect(screen.getByText(/no strategies configured/i)).toBeInTheDocument();
  });

  it("links View all to the strategies leaderboard sort", () => {
    renderBoard([strategy({})], [run({})]);
    expect(screen.getByRole("link", { name: /view all/i })).toHaveAttribute(
      "href",
      "/strategies?sort=leaderboard",
    );
  });

  it("caps the board at 6 rows", () => {
    const strategies = Array.from({ length: 9 }, (_, i) =>
      strategy({ agent_id: `s${i}`, display_name: `S${i}` }),
    );
    const runs = strategies.map((s, i) =>
      run({ id: `r${i}`, agent_id: s.agent_id, total_return_pct: i }),
    );
    renderBoard(strategies, runs);
    expect(screen.getAllByTestId(/leaderboard-row-/)).toHaveLength(6);
  });
});
