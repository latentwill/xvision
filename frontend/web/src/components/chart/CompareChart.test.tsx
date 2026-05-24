import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import type { CompareChartPayload } from "@/api/types.gen/CompareChartPayload";
import { CompareChart } from "./CompareChart";

const chartMocks = vi.hoisted(() => ({
  createChart: vi.fn(),
  createdCharts: [] as Array<ReturnType<typeof createChartStub>>,
  priceScaleOptions: [] as Array<{ id: string; options: unknown }>,
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
  const priceScales = new Map<string, { applyOptions: ReturnType<typeof vi.fn> }>();
  return {
    addCandlestickSeries: vi.fn(() => createSeriesStub()),
    addLineSeries: vi.fn(() => createSeriesStub()),
    priceScale: vi.fn((id: string) => {
      const existing = priceScales.get(id);
      if (existing) return existing;
      const scale = {
        applyOptions: vi.fn((options: unknown) => {
          chartMocks.priceScaleOptions.push({ id, options });
        }),
      };
      priceScales.set(id, scale);
      return scale;
    }),
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

function comparePayload(pointCount: number): CompareChartPayload {
  return {
    shared_scenario: "scenario-1",
    price_backdrop: null,
    runs: [
      {
        run_id: "run-1",
        label: "Run 1",
        scenario_id: "scenario-1",
        equity: Array.from({ length: pointCount }, (_, index) => ({
          time: 1_700_000_000 + index * 3_600,
          equity_usd: 100_000 + index,
        })),
      },
    ],
  };
}

describe("CompareChart", () => {
  beforeEach(() => {
    chartMocks.createdCharts.length = 0;
    chartMocks.priceScaleOptions.length = 0;
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

  it("applies range preset buttons to the comparison viewport", () => {
    render(<CompareChart payload={comparePayload(48)} />);

    expect(latestChart().timeScaleApi.fitContent).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "1d" }));

    expect(latestChart().timeScaleApi.setVisibleLogicalRange).toHaveBeenCalledWith({
      from: 24,
      to: 50,
    });
    expect(chartMocks.priceScaleOptions).toContainEqual({
      id: "right",
      options: { autoScale: true },
    });
    expect(latestChart().timeScaleApi.fitContent).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "All" }));

    expect(latestChart().timeScaleApi.fitContent).toHaveBeenCalledTimes(1);
  });
});
