// Focused tests for the sort dropdown added by
// `list-search-filter-missing-surfaces` slice 3. The compare table is a
// post-hoc N-run comparison; the highest-value ergonomic is sort
// (operator wants to rank runs by Sharpe / max DD / return), not
// search (typically 2-10 rows, all visible).

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { EvalCompareRoute } from "./eval-compare";
import type {
  ComparisonReport,
  ComparisonRunSummary,
} from "@/api/types.gen";

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>("@/api/eval");
  return { ...actual, compareRuns: vi.fn() };
});
vi.mock("@/api/chart", async () => {
  const actual = await vi.importActual<typeof import("@/api/chart")>(
    "@/api/chart",
  );
  return { ...actual, getCompareChart: vi.fn().mockResolvedValue(null) };
});
vi.mock("@/api/scenarios", async () => {
  const actual = await vi.importActual<typeof import("@/api/scenarios")>(
    "@/api/scenarios",
  );
  return { ...actual, listScenarios: vi.fn().mockResolvedValue([]) };
});
vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return { ...actual, listStrategies: vi.fn().mockResolvedValue([]) };
});
vi.mock("@/components/chart/CompareChart", () => ({
  CompareChart: () => null,
}));

const evalApi = await import("@/api/eval");

function mkRun(overrides: Partial<ComparisonRunSummary>): ComparisonRunSummary {
  return {
    id: "01RUN0000000",
    agent_id: "ag-x",
    scenario_id: "sc-x",
    mode: "Backtest" as never,
    status: "completed" as never,
    started_at: "2026-05-21T00:00:00Z",
    completed_at: "2026-05-21T01:00:00Z",
    error: null,
    metrics: {
      total_return_pct: 0,
      sharpe: 0,
      max_drawdown_pct: 0,
      win_rate: 0,
      n_trades: 0,
      n_decisions: 0,
      inference_cost_quote_total: null,
      net_return_pct: null,
    } as never,
    net_return_pct: null,
    ...overrides,
  } as ComparisonRunSummary;
}

function report(runs: ComparisonRunSummary[]): ComparisonReport {
  return {
    runs,
    equity_curves: [],
    findings: [],
  } as unknown as ComparisonReport;
}

function renderRoute(ids: string[]) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const search = `?ids=${ids.join(",")}`;
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[`/eval-runs/compare${search}`]}>
        <EvalCompareRoute />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("EvalCompareRoute sort", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });
  afterEach(() => cleanup());

  it("renders runs in call order by default", async () => {
    vi.mocked(evalApi.compareRuns).mockResolvedValue(
      report([
        mkRun({
          id: "run-A",
          metrics: {
            total_return_pct: 1,
            sharpe: 0.5,
            max_drawdown_pct: 8,
            win_rate: 0.5,
            n_trades: 1,
            n_decisions: 10,
          } as never,
        }),
        mkRun({
          id: "run-B",
          metrics: {
            total_return_pct: 9,
            sharpe: 1.5,
            max_drawdown_pct: 2,
            win_rate: 0.6,
            n_trades: 3,
            n_decisions: 20,
          } as never,
        }),
      ]),
    );

    renderRoute(["run-A", "run-B"]);

    // Wait for the table rows to render.
    await screen.findByText("run-A");
    const ids = screen
      .getAllByText(/^run-[A-Z]$/)
      .map((el) => el.textContent);
    expect(ids).toEqual(["run-A", "run-B"]);
  });

  it("re-orders rows by Sharpe (high → low) when the sort is changed", async () => {
    vi.mocked(evalApi.compareRuns).mockResolvedValue(
      report([
        mkRun({
          id: "run-A",
          metrics: {
            total_return_pct: 1,
            sharpe: 0.5,
            max_drawdown_pct: 8,
            win_rate: 0.5,
            n_trades: 1,
            n_decisions: 10,
          } as never,
        }),
        mkRun({
          id: "run-B",
          metrics: {
            total_return_pct: 9,
            sharpe: 1.5,
            max_drawdown_pct: 2,
            win_rate: 0.6,
            n_trades: 3,
            n_decisions: 20,
          } as never,
        }),
      ]),
    );

    renderRoute(["run-A", "run-B"]);
    await screen.findByText("run-A");

    const sort = screen.getByTestId("compare-sort");
    fireEvent.change(sort, { target: { value: "sharpe" } });

    const ids = screen
      .getAllByText(/^run-[A-Z]$/)
      .map((el) => el.textContent);
    // run-B has Sharpe 1.5; run-A has 0.5. Descending → B before A.
    expect(ids).toEqual(["run-B", "run-A"]);
  });
});
