import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import * as chartApi from "@/api/chart";
import * as cliApi from "@/api/cli";
import * as evalApi from "@/api/eval";
import * as scenarioApi from "@/api/scenarios";
import * as strategiesApi from "@/api/strategies";
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
  return {
    ...actual,
    getScenario: vi.fn(),
    cloneScenario: vi.fn(),
    archiveScenario: vi.fn(),
    deleteScenario: vi.fn(),
  };
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

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return { ...actual, listStrategies: vi.fn().mockResolvedValue([]) };
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
  warmup_bars: 200,
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
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("clones a scenario for editing", async () => {
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(chartPayload);
    vi.mocked(scenarioApi.cloneScenario).mockResolvedValue({
      ...scenario,
      id: "sc_clone",
      display_name: "Crypto range bound (clone)",
      parent_scenario_id: scenario.id,
      source: "Clone",
    } as Scenario);

    renderRoute();

    // QA22 / `strategy-clone-editable-frontend`: "Clone to edit" now
    // opens an inline form (no popup per the workspace UI rule) where
    // the operator can amend display_name/description/notes/tags
    // before submitting. `scenario-clone-form-structural-fields`
    // upgraded the inline form to `<ScenarioForm>` so the operator
    // can also override asset/window/granularity/venue/warmup_bars;
    // submit is labelled "Create →" per the shared component.
    fireEvent.click(await screen.findByRole("button", { name: "Clone to edit" }));
    fireEvent.click(await screen.findByRole("button", { name: /Create/i }));

    await waitFor(() => {
      expect(scenarioApi.cloneScenario).toHaveBeenCalledWith(
        scenario.id,
        expect.objectContaining({
          display_name: "Crypto range bound (clone)",
        }),
      );
    });
  });

  it("emits the changed granularity in clone mutations when the operator overrides it", async () => {
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(chartPayload);
    vi.mocked(scenarioApi.cloneScenario).mockResolvedValue({
      ...scenario,
      id: "sc_clone_granularity",
      granularity: "5m",
      parent_scenario_id: scenario.id,
      source: "Clone",
    } as Scenario);

    renderRoute();

    fireEvent.click(await screen.findByRole("button", { name: "Clone to edit" }));

    // The form is pre-filled from the parent. The granularity widget is
    // a <select>; change it to a different supported option.
    const granularitySelect = await screen.findByLabelText(/Granularity/i);
    fireEvent.change(granularitySelect, { target: { value: "5m" } });

    fireEvent.click(await screen.findByRole("button", { name: /Create/i }));

    await waitFor(() => {
      expect(scenarioApi.cloneScenario).toHaveBeenCalledWith(
        scenario.id,
        expect.objectContaining({
          granularity: "5m",
        }),
      );
    });
  });

  it("sends null for structural fields the operator did not touch", async () => {
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(chartPayload);
    vi.mocked(scenarioApi.cloneScenario).mockResolvedValue({
      ...scenario,
      id: "sc_clone_untouched",
      parent_scenario_id: scenario.id,
      source: "Clone",
    } as Scenario);

    renderRoute();

    fireEvent.click(await screen.findByRole("button", { name: "Clone to edit" }));
    fireEvent.click(await screen.findByRole("button", { name: /Create/i }));

    await waitFor(() => {
      expect(scenarioApi.cloneScenario).toHaveBeenCalled();
    });
    const args = vi.mocked(scenarioApi.cloneScenario).mock.calls[0]!;
    const mutations = args[1] as Record<string, unknown>;
    // The operator only opened the form and hit submit without
    // touching structural fields → those must be null (inherit
    // parent). display_name diverges from parent ("(clone)" suffix
    // pre-filled) so it's a non-null override.
    expect(mutations.granularity).toBeNull();
    expect(mutations.asset).toBeNull();
    expect(mutations.time_window).toBeNull();
    expect(mutations.venue).toBeNull();
    expect(mutations.warmup_bars).toBeNull();
  });

  it("archives a scenario without leaving the detail view", async () => {
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(chartPayload);
    vi.mocked(scenarioApi.archiveScenario).mockResolvedValue(undefined);

    renderRoute();

    fireEvent.click(await screen.findByRole("button", { name: "Archive" }));

    await waitFor(() => {
      expect(scenarioApi.archiveScenario).toHaveBeenCalledWith(scenario.id);
    });
  });

  it("surfaces hard-delete failures so users can archive instead", async () => {
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(chartPayload);
    vi.mocked(scenarioApi.deleteScenario).mockRejectedValue(
      new Error("cannot delete scenario: 2 runs reference it. Archive instead."),
    );

    renderRoute();

    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));

    expect(
      await screen.findByText("cannot delete scenario: 2 runs reference it. Archive instead."),
    ).toBeInTheDocument();
  });

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
      // v2b-remote-cli-job-safety audit fields
      pid: null,
      job_user: null,
      job_source: null,
      command_class: "bars",
      cancelled_at: null,
      cancel_signal: null,
      recovered_at: null,
      recovery_reason: null,
      max_runtime_seconds: 3600,
      max_output_bytes: 10485760,
      output_cap_exceeded: false,
      runtime_cap_exceeded: false,
      output_bytes: 0,
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

  it("refetches the scenario chart when the indicator timeframe changes", async () => {
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(chartPayload);

    renderRoute();

    const selector = await screen.findByLabelText("Indicator timeframe");
    fireEvent.change(selector, { target: { value: "1h" } });

    await waitFor(() => {
      expect(chartApi.getScenarioChart).toHaveBeenCalledWith(scenario.id, "1h");
    });
  });
});

describe("RunsTab strategy name display", () => {
  const runRow = {
    id: "01HZ000000000000000000001",
    agent_id: "01HZ000000000000000000002",
    scenario_id: scenario.id,
    mode: "Backtest",
    status: "Completed",
    completed_at: "2026-05-19T10:00:00Z",
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(chartPayload);
  });

  afterEach(() => {
    cleanup();
  });

  it("renders the strategy display_name when listStrategies resolves to a matching id", async () => {
    vi.mocked(strategiesApi.listStrategies).mockResolvedValue([
      {
        agent_id: runRow.agent_id,
        display_name: "My Momentum Strategy",
        template: "single_llm",
        decision_cadence_minutes: 60,
        tags: [],
        models: [],
        providers: [],
      },
    ]);
    // @ts-expect-error — minimal RunSummary shape sufficient for test
    vi.mocked(evalApi.listRuns).mockResolvedValue([runRow]);

    renderRoute();

    // Navigate to Runs tab
    fireEvent.click(await screen.findByRole("button", { name: "Runs" }));

    // Strategy display name should appear
    expect(await screen.findByText("My Momentum Strategy")).toBeInTheDocument();
    // ULID should appear as the muted secondary line
    expect(screen.getByText(runRow.agent_id)).toBeInTheDocument();
  });

  it("falls back to showing only the ULID when listStrategies has no matching strategy", async () => {
    vi.mocked(strategiesApi.listStrategies).mockResolvedValue([
      // Different agent_id — no match for runRow.agent_id
      {
        agent_id: "01HZ000000000000000000099",
        display_name: "Other Strategy",
        template: "single_llm",
        decision_cadence_minutes: 60,
        tags: [],
        models: [],
        providers: [],
      },
    ]);
    // @ts-expect-error — minimal RunSummary shape sufficient for test
    vi.mocked(evalApi.listRuns).mockResolvedValue([runRow]);

    renderRoute();

    fireEvent.click(await screen.findByRole("button", { name: "Runs" }));

    // ULID appears (as the only Strategy cell content)
    expect(await screen.findByText(runRow.agent_id)).toBeInTheDocument();
    // Non-matching strategy name must NOT appear
    expect(screen.queryByText("Other Strategy")).not.toBeInTheDocument();
  });
});
