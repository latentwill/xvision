import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { ScenarioChart } from "./ScenarioChart";
import type { ScenarioChartPayload } from "@/api/types.gen/ScenarioChartPayload";

const chartMocks = vi.hoisted(() => ({
  createChart: vi.fn(),
}));

function createSeriesStub() {
  return {
    setData: vi.fn(),
  };
}

function createChartStub() {
  return {
    addCandlestickSeries: vi.fn(() => createSeriesStub()),
    addLineSeries: vi.fn(() => createSeriesStub()),
    addHistogramSeries: vi.fn(() => createSeriesStub()),
    priceScale: vi.fn(() => ({ applyOptions: vi.fn() })),
    timeScale: vi.fn(() => ({
      fitContent: vi.fn(),
      setVisibleLogicalRange: vi.fn(),
      subscribeVisibleLogicalRangeChange: vi.fn(),
    })),
    remove: vi.fn(),
  };
}

vi.mock("lightweight-charts", () => ({
  ColorType: { Solid: "solid" },
  CrosshairMode: { Normal: 0 },
  createChart: chartMocks.createChart,
}));

const indicatorSeries = [{ time: 1, value: 100 }];

const payload: ScenarioChartPayload = {
  scenario: {
    id: "s1",
    parent_scenario_id: null,
    source: "User",
    display_name: "Scenario",
    description: "",
    tags: [],
    notes: null,
    asset_class: "Crypto",
    asset: [{ symbol: "BTC/USD", venue_symbol: "BTC/USD" }],
    quote_currency: "USD",
    time_window: {
      start: "2026-01-01T00:00:00Z",
      end: "2026-01-02T00:00:00Z",
    },
    granularity: "Hour1",
    timezone: "UTC",
    calendar: "Continuous24x7",
    data_source: { vendor: "Alpaca", dataset: "crypto-bars" },
    venue: {
      venue: "Alpaca",
      fee_bps: 0,
      slippage_model: { FixedBps: 0 },
      min_notional: null,
    },
    replay_mode: "Continuous",
    capital: { initial: 100000, currency: "USD" },
    bar_cache_policy: { cache_key: "cache", warmup_bars: 0 },
    created_at: "2026-01-01T00:00:00Z",
    created_by: "test",
    archived_at: null,
  } as unknown as ScenarioChartPayload["scenario"],
  bars: [{ time: 1, open: 1, high: 2, low: 1, close: 2, volume: 10 }],
  indicators: {
    sma_20: indicatorSeries,
    sma_30: indicatorSeries,
    sma_50: indicatorSeries,
    sma_60: indicatorSeries,
    sma_90: indicatorSeries,
    sma_200: indicatorSeries,
    ema_20: indicatorSeries,
    ema_30: indicatorSeries,
    ema_50: indicatorSeries,
    ema_60: indicatorSeries,
    ema_90: indicatorSeries,
    ema_200: indicatorSeries,
    bollinger: { upper: indicatorSeries, middle: indicatorSeries, lower: indicatorSeries },
    donchian: { upper: indicatorSeries, lower: indicatorSeries },
    rsi_14: indicatorSeries,
    macd: { line: indicatorSeries, signal: indicatorSeries, histogram: indicatorSeries },
    atr_14: indicatorSeries,
  },
  cache_status: { type: "FullyCached", bar_count: 1, fetched_at: "2026-01-01T00:00:00Z" },
};

describe("ScenarioChart", () => {
  beforeEach(() => {
    localStorage.clear();
    chartMocks.createChart.mockImplementation(createChartStub);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("uses the shared layer panel with expanded moving average controls", () => {
    render(<ScenarioChart payload={payload} />);

    fireEvent.click(screen.getByText(/Layers/));

    expect(screen.getByText("sma30")).toBeInTheDocument();
    expect(screen.getByText("sma60")).toBeInTheDocument();
    expect(screen.getByText("sma90")).toBeInTheDocument();
  });
});
