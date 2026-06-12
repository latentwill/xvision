import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

import type { AgentRunSummary } from "@/api/types-agent-runs";

import { StrategyStrip } from "./StrategyStrip";

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

function mkLiveRun(over: Partial<AgentRunSummary> = {}): AgentRunSummary {
  return mkRun({
    is_live_money: true,
    eval_mode: "live",
    eval_run_status: "running",
    ...over,
  });
}

function renderStrip(runs: AgentRunSummary[]) {
  return render(
    <MemoryRouter>
      <StrategyStrip
        runs={runs}
        selectedId={runs[0]?.run_id ?? null}
        onSelect={vi.fn()}
        selectedConnStatus="streaming"
        walletDisabled={false}
        strategies={[
          { agent_id: "strat_live", display_name: "Promoted live strategy" },
          { agent_id: "strat_paper", display_name: "Backtest-only strategy" },
        ]}
      />
    </MemoryRouter>,
  );
}

describe("StrategyStrip", () => {
  it("renders a live run list instead of the old capsule strip", () => {
    renderStrip([
      mkLiveRun({
        run_id: "live_1",
        agent_id: "strat_live",
        model_call_count: 11,
        span_count: 5,
      }),
      mkRun({
        run_id: "paper_1",
        agent_id: "strat_paper",
        eval_mode: "backtest",
      }),
    ]);

    expect(screen.getByTestId("live-run-list")).toBeInTheDocument();
    expect(screen.getByTestId("live-run-row-live_1")).toHaveTextContent(
      "Promoted live strategy",
    );
    expect(screen.getByTestId("live-run-row-live_1")).not.toHaveTextContent(
      /eval run/i,
    );
    expect(screen.queryByTestId("live-run-row-paper_1")).not.toBeInTheDocument();
    const liveRow = screen.getByTestId("live-run-row-live_1");
    expect(liveRow).toHaveTextContent("PnL —");
    expect(liveRow).toHaveTextContent("Decisions 11");
    expect(liveRow).toHaveTextContent("Trades 4");
    expect(liveRow).toHaveTextContent("Sharpe —");
    expect(within(liveRow).getByRole("button", { name: "Pause strategy" }))
      .toBeVisible();
    expect(within(liveRow).getByRole("button", { name: "Stop strategy" }))
      .toBeVisible();
    expect(screen.getByRole("link", { name: /trace 5/i })).toHaveAttribute(
      "href",
      "/live/runs/live_1",
    );

    fireEvent.click(screen.getByTestId("strip-filter-all"));

    expect(screen.getByTestId("live-run-row-paper_1")).toHaveTextContent(
      "Backtest-only strategy",
    );
  });
});
