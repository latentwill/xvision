import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import type { StrategyChartPayload } from "@/api/types.gen/StrategyChartPayload";
import { StrategyChart } from "./StrategyChart";

const chartMocks = vi.hoisted(() => ({
  createChart: vi.fn(),
  createdCharts: [] as Array<ReturnType<typeof createChartStub>>,
}));

function createSeriesStub() {
  return {
    setData: vi.fn(),
  };
}

function createChartStub() {
  const timeScaleApi = {
    fitContent: vi.fn(),
    setVisibleLogicalRange: vi.fn(),
  };
  return {
    addLineSeries: vi.fn(() => createSeriesStub()),
    priceScale: vi.fn(() => ({ applyOptions: vi.fn() })),
    timeScale: vi.fn(() => timeScaleApi),
    timeScaleApi,
    remove: vi.fn(),
  };
}

function latestChart() {
  return chartMocks.createdCharts[chartMocks.createdCharts.length - 1];
}

vi.mock("lightweight-charts", () => ({
  ColorType: { Solid: "solid" },
  CrosshairMode: { Normal: 0 },
  createChart: chartMocks.createChart,
}));

function strategyPayload(pointCount: number): StrategyChartPayload {
  return {
    strategy_id: "strategy-1",
    scenarios: [["scenario-1", "Scenario 1"]],
    run_series: [
      {
        run_id: "run-1",
        label: "Run 1",
        scenario_id: "scenario-1",
        final_pnl_usd: 100,
        max_drawdown_pct: 1,
        sharpe: null,
        equity_normalised: Array.from({ length: pointCount }, (_, index) => ({
          time: 1_700_000_000 + index * 3_600,
          equity_usd: 100_000 + index,
        })),
      },
    ],
  };
}

describe("StrategyChart", () => {
  beforeEach(() => {
    chartMocks.createdCharts.length = 0;
    chartMocks.createChart.mockImplementation(() => {
      const chart = createChartStub();
      chartMocks.createdCharts.push(chart);
      return chart;
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("applies range preset buttons to the strategy viewport", () => {
    render(<StrategyChart payload={strategyPayload(48)} />);

    expect(latestChart().timeScaleApi.fitContent).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "1d" }));

    expect(latestChart().timeScaleApi.setVisibleLogicalRange).toHaveBeenCalledWith({
      from: 24,
      to: 50,
    });
    expect(latestChart().timeScaleApi.fitContent).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "All" }));

    expect(latestChart().timeScaleApi.fitContent).toHaveBeenCalledTimes(1);
  });
});
