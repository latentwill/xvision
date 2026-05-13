import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

import * as chartApi from "@/api/chart";
import * as cliApi from "@/api/cli";
import * as scenarioApi from "@/api/scenarios";
import type { Scenario } from "@/api/types.gen";
import type { ScenarioChartPayload } from "@/api/types.gen/ScenarioChartPayload";
import { ScenariosDetailRoute } from "./scenarios-detail";

vi.mock("lightweight-charts", () => ({
  ColorType: { Solid: "solid" },
  CrosshairMode: { Normal: "normal" },
  createChart: vi.fn(() => ({
    addCandlestickSeries: vi.fn(() => ({ setData: vi.fn() })),
    addHistogramSeries: vi.fn(() => ({ setData: vi.fn() })),
    priceScale: vi.fn(() => ({ applyOptions: vi.fn() })),
    remove: vi.fn(),
  })),
}));

vi.mock("@/api/scenarios", async () => {
  const actual = await vi.importActual<typeof import("@/api/scenarios")>(
    "@/api/scenarios",
  );
  return { ...actual, getScenario: vi.fn() };
});

vi.mock("@/api/chart", async () => {
  const actual = await vi.importActual<typeof import("@/api/chart")>(
    "@/api/chart",
  );
  return { ...actual, getScenarioChart: vi.fn() };
});

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>("@/api/eval");
  return { ...actual, listRuns: vi.fn().mockResolvedValue([]) };
});

vi.mock("@/api/cli", async () => {
  const actual = await vi.importActual<typeof import("@/api/cli")>("@/api/cli");
  return {
    ...actual,
    createCliJob: vi.fn(),
    getCliJob: vi.fn(),
    getCliJobOutput: vi.fn(),
  };
});

const scenario = {
  id: "crypto-rangebound-q2-2025",
  display_name: "Crypto range bound",
  description: "",
  tags: [],
  notes: null,
  asset_class: "Crypto",
  asset: [{ class: "Crypto", symbol: "BTC", venue_symbol: "BTC/USD" }],
  quote_currency: "Usd",
  time_window: { start: "2025-04-01T00:00:00Z", end: "2025-06-30T00:00:00Z" },
  granularity: "Hour4",
  timezone: "UTC",
  calendar: "Continuous24x7",
  data_source: { type: "AlpacaHistorical", feed: null, adjustment: "Raw" },
  venue: {
    venue: "Alpaca",
    fees: { maker_bps: 10, taker_bps: 25 },
    slippage: { model: "linear", bps: 5 },
    latency: { decision_to_fill_ms: 500 },
    fill_model: {
      market_order_fill: "FullAtClose",
      limit_order_fill: "NeverFills",
      partial_fills: false,
      volume_constraints: null,
    },
  },
  replay_mode: { mode: "Continuous" },
  capital: { initial: 10000, currency: "USD" },
  bar_cache_policy: {
    cache_key: "bars-btc-hour4",
    refresh_policy: "NeverRefresh",
    data_fetched_at: null,
  },
  parent_scenario_id: null,
  source: "User",
  created_at: "2026-05-12T00:00:00Z",
  created_by: "test",
  archived_at: null,
} as unknown as Scenario;

const chartPayload: ScenarioChartPayload = {
  scenario,
  bars: [],
  indicators: {
    sma_20: [], sma_30: [], sma_50: [], sma_60: [], sma_90: [], sma_200: [],
    ema_20: [], ema_30: [], ema_50: [], ema_60: [], ema_90: [], ema_200: [],
    bollinger: { upper: [], middle: [], lower: [] },
    donchian: { upper: [], lower: [] },
    rsi_14: [],
    macd: { line: [], signal: [], histogram: [] },
    atr_14: [],
  },
  cache_status: { type: "NotCached", expected_count: 540 },
};

function renderRoute() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter initialEntries={["/scenarios/crypto-rangebound-q2-2025"]}>
      <QueryClientProvider client={client}>
        <Routes>
          <Route path="/scenarios/:id" element={<ScenariosDetailRoute />} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("ScenariosDetailRoute bars cache actions", () => {
  it("starts a bars fetch CLI job and refreshes the scenario chart after completion", async () => {
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(chartPayload);
    vi.mocked(cliApi.createCliJob).mockResolvedValue({
      job_id: "job-1",
      status: "queued",
    });
    vi.mocked(cliApi.getCliJob).mockResolvedValue({
      job_id: "job-1",
      argv: ["bars", "fetch"],
      status: "succeeded",
      exit_code: 0,
      timed_out: false,
      cancel_requested: false,
      stdout_bytes: 0,
      stderr_bytes: 0,
      stdout_truncated: false,
      stderr_truncated: false,
      created_at: "2026-05-12T00:00:00Z",
      started_at: "2026-05-12T00:00:00Z",
      finished_at: "2026-05-12T00:00:01Z",
      error_message: null,
    });
    vi.mocked(cliApi.getCliJobOutput).mockResolvedValue({
      job_id: "job-1",
      status: "succeeded",
      exit_code: 0,
      stdout: "Fetched 540 bars",
      stderr: "",
      stdout_bytes: 16,
      stderr_bytes: 0,
      stdout_truncated: false,
      stderr_truncated: false,
    });

    renderRoute();

    fireEvent.click(await screen.findByRole("button", { name: "Fetch bars" }));

    await waitFor(() => {
      expect(cliApi.createCliJob).toHaveBeenCalledWith({
        argv: [
          "bars",
          "fetch",
          "--asset",
          "BTC",
          "--granularity",
          "4h",
          "--from",
          "2025-04-01",
          "--to",
          "2025-06-30",
        ],
      });
    });
    expect(await screen.findByText(/Fetched 540 bars/i)).toBeTruthy();
    await waitFor(() => {
      expect(chartApi.getScenarioChart).toHaveBeenCalledTimes(2);
    });
  });
});
