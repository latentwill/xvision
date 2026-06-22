import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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
import type * as ChartApiModule from "@/api/chart";
import type * as CliApiModule from "@/api/cli";
import type * as EvalApiModule from "@/api/eval";
import type * as ScenariosApiModule from "@/api/scenarios";
import type * as StrategiesApiModule from "@/api/strategies";

// The chart region now renders ScenarioChartV2, whose candle pane drives
// klinecharts and whose equity/volume panes drive uPlot — neither plays
// well in jsdom (canvas + matchMedia). Stub the v2 pane primitives to
// lightweight markers, mirroring the surface-test mocking pattern in
// `src/components/chart/v2/surfaces/ScenarioChartV2.test.tsx`. The bulk of
// the route tests use a `bars: []` fixture (empty-state path, no panes),
// but the bars-present test below renders the real surface.
vi.mock("@/components/chart/v2/primitives/KlineCandlePane", () => ({
  KlineCandlePane: () => <div data-testid="kline-candle-pane" />,
}));
vi.mock("@/components/chart/v2/primitives/UplotEquityPane", () => ({
  UplotEquityPane: () => <div data-testid="uplot-equity-pane" />,
}));
vi.mock("@/components/chart/v2/primitives/UplotHistogramPane", () => ({
  UplotHistogramPane: () => <div data-testid="uplot-histogram-pane" />,
}));

vi.mock("@/api/scenarios", async () => {
  const actual = await vi.importActual<typeof ScenariosApiModule>(
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
  const actual = await vi.importActual<typeof ChartApiModule>(
    "@/api/chart",
  );
  return { ...actual, getScenarioChart: vi.fn() };
});

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof EvalApiModule>("@/api/eval");
  return { ...actual, listRuns: vi.fn().mockResolvedValue([]) };
});

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof StrategiesApiModule>(
    "@/api/strategies",
  );
  return { ...actual, listStrategies: vi.fn().mockResolvedValue([]) };
});

vi.mock("@/api/cli", async () => {
  const actual = await vi.importActual<typeof CliApiModule>("@/api/cli");
  return {
    ...actual,
    createCliJob: vi.fn(),
    getCliJob: vi.fn(),
    getCliJobOutput: vi.fn(),
  };
});

// Mock the assets API so AssetPicker renders with known assets (BTC/USD + ETH/USD).
vi.mock("@/api/assets", () => ({
  useAssets: () => ({
    data: [
      { symbol: "BTC/USD", category: "Crypto", data: "alpaca", venues: {}, enabled: true },
      { symbol: "ETH/USD", category: "Crypto", data: "alpaca", venues: {}, enabled: true },
    ],
    isPending: false,
    isError: false,
  }),
  useAlpacaAssets: () => ({
    data: [
      { symbol: "BTC/USD", category: "Crypto", data: "alpaca", venues: {}, enabled: true },
      { symbol: "ETH/USD", category: "Crypto", data: "alpaca", venues: {}, enabled: true },
    ],
    isPending: false,
    isError: false,
  }),
}));

const scenario = {
  id: "crypto-rangebound-q2-2025",
  display_name: "Crypto range bound",
  description: "",
  tags: [],
  notes: null,
  asset_class: "Crypto",
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
    overrides: [],
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
  preview_asset: "BTC",
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
    // can also override window/granularity/venue/warmup_bars;
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

  it("emits null granularity on clone now that the operator-facing control is hidden", async () => {
    vi.mocked(scenarioApi.getScenario).mockResolvedValue({
      ...scenario,
      granularity: "5m",
    });
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

    // The granularity selector was removed from ScenarioForm per QA. The form
    // inherits the parent's granularity silently, so the clone mutation is
    // always "unchanged from parent" → null on the wire.
    expect(screen.queryByLabelText(/Granularity/i)).not.toBeInTheDocument();

    fireEvent.click(await screen.findByRole("button", { name: /Create/i }));

    await waitFor(() => {
      expect(scenarioApi.cloneScenario).toHaveBeenCalledWith(
        scenario.id,
        expect.objectContaining({
          granularity: null,
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
          "BTC/USD",
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
    const user = userEvent.setup();
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(chartPayload);

    renderRoute();

    await user.click(await screen.findByRole("button", { name: /indicator timeframe/i }));
    await user.click(await screen.findByRole("option", { name: "1 hour" }));

    await waitFor(() => {
      expect(chartApi.getScenarioChart).toHaveBeenCalledWith(
        scenario.id,
        "1h",
        "BTC/USD",
      );
    });
  });

  it("refetches the scenario chart when the preview asset changes", async () => {
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(chartPayload);

    renderRoute();

    // The standalone preview defaults to BTC/USD.
    await waitFor(() => {
      expect(chartApi.getScenarioChart).toHaveBeenCalledWith(
        scenario.id,
        "4h",
        "BTC/USD",
      );
    });

    // The asset picker is a Signal-styled listbox trigger.
    // Click it to open, then click the ETH/USD option.
    const assetPicker = await screen.findByRole("button", { name: "Asset picker" });
    fireEvent.click(assetPicker);
    const ethOption = await screen.findByRole("option", { name: /ETH\/USD/i });
    fireEvent.click(ethOption);

    await waitFor(() => {
      expect(chartApi.getScenarioChart).toHaveBeenCalledWith(
        scenario.id,
        "4h",
        "ETH/USD",
      );
    });
  });

  it("fetches bars for the selected preview asset", async () => {
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(chartPayload);
    vi.mocked(cliApi.createCliJob).mockResolvedValue({
      job_id: "job-1",
      status: "queued",
    });

    renderRoute();

    // The asset picker is a Signal-styled listbox trigger.
    const assetPicker = await screen.findByRole("button", { name: "Asset picker" });
    fireEvent.click(assetPicker);
    const ethOption = await screen.findByRole("option", { name: /ETH\/USD/i });
    fireEvent.click(ethOption);

    fireEvent.click(await screen.findByRole("button", { name: "Fetch bars" }));

    await waitFor(() => {
      expect(cliApi.createCliJob).toHaveBeenCalledWith({
        argv: expect.arrayContaining(["--asset", "ETH/USD"]),
      });
    });
  });

  it("renders the route-level empty state when no bars are cached", async () => {
    // Task 12: the v1 ScenarioChart owned the empty-bars message; the
    // route now renders it directly (the v2 surface is a pure renderer).
    // The chartPayload fixture has `bars: []`, so the empty state shows.
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(chartPayload);

    renderRoute();

    expect(
      await screen.findByText(
        "No bars cached yet. Use Fetch bars to populate this chart.",
      ),
    ).toBeInTheDocument();
    // The v2 candle pane must NOT render while there are no bars.
    expect(screen.queryByTestId("kline-candle-pane")).not.toBeInTheDocument();
    // The route-level asset · granularity label uses the preview defaults.
    expect(screen.getByText("BTC/USD · 4h")).toBeInTheDocument();
  });

  it("renders ScenarioChartV2 when bars are present", async () => {
    // With cached bars the route hands the payload to ScenarioChartV2
    // (klinecharts/uPlot panes stubbed above) instead of the empty state.
    const withBars: ScenarioChartPayload = {
      ...chartPayload,
      bars: [
        {
          time: 1700000000,
          open: 100,
          high: 105,
          low: 95,
          close: 101,
          volume: 10,
        },
      ],
      cache_status: { type: "FullyCached", bar_count: 1, fetched_at: "2026-05-12T00:00:00Z" },
    };
    vi.mocked(scenarioApi.getScenario).mockResolvedValue(scenario);
    vi.mocked(chartApi.getScenarioChart).mockResolvedValue(withBars);

    renderRoute();

    expect(await screen.findByTestId("kline-candle-pane")).toBeInTheDocument();
    expect(
      screen.queryByText(
        "No bars cached yet. Use Fetch bars to populate this chart.",
      ),
    ).not.toBeInTheDocument();
  });
});

// `<ResponsiveListCard>` reads `useViewportMode()` which calls
// `window.matchMedia`. jsdom doesn't provide it; install a desktop
// stub so the route mounts.
function stubMatchMediaDesktop() {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string) => ({
      matches: query.includes("min-width: 1280px"),
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

describe("RunsTab strategy name display", () => {
  const runRow = {
    id: "01HZ000000000000000000001",
    agent_id: "01HZ000000000000000000002",
    scenario_id: scenario.id,
    mode: "backtest",
    status: "completed",
    started_at: "2026-05-19T09:00:00Z",
    completed_at: "2026-05-19T10:00:00Z",
  };

  beforeEach(() => {
    vi.clearAllMocks();
    stubMatchMediaDesktop();
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

  it("filters by strategy substring and excludes runs from other scenarios", async () => {
    const otherScenarioRun = {
      ...runRow,
      id: "01HZ000000000000000000003",
      scenario_id: "scenario-other",
    };
    const matchingRun = {
      ...runRow,
      id: "01HZ000000000000000000004",
      agent_id: "01HZ000000000000000000005",
    };

    vi.mocked(strategiesApi.listStrategies).mockResolvedValue([
      {
        agent_id: runRow.agent_id,
        display_name: "Mean Reversion v3",
        template: "single_llm",
        decision_cadence_minutes: 60,
        tags: [],
        models: [],
        providers: [],
      },
      {
        agent_id: matchingRun.agent_id,
        display_name: "Trend Follow v1",
        template: "single_llm",
        decision_cadence_minutes: 60,
        tags: [],
        models: [],
        providers: [],
      },
    ]);
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      runRow,
      matchingRun,
      otherScenarioRun,
    ] as unknown as Awaited<ReturnType<typeof evalApi.listRuns>>);

    renderRoute();
    fireEvent.click(await screen.findByRole("button", { name: "Runs" }));

    // Both scenario-scoped runs render; the other-scenario run does not.
    expect(await screen.findByText("Mean Reversion v3")).toBeInTheDocument();
    expect(screen.getByText("Trend Follow v1")).toBeInTheDocument();
    expect(screen.queryByText(otherScenarioRun.id)).not.toBeInTheDocument();

    // Typing "trend" in the toolbar search narrows to the matching row.
    const search = screen.getByPlaceholderText(/search run id or strategy/i);
    fireEvent.change(search, { target: { value: "trend" } });

    await waitFor(() => {
      expect(screen.queryByText("Mean Reversion v3")).not.toBeInTheDocument();
    });
    expect(screen.getByText("Trend Follow v1")).toBeInTheDocument();
  });

  it("filters pending runs using the backend queued status value", async () => {
    const queuedRun = {
      ...runRow,
      id: "01HZ000000000000000000006",
      status: "queued",
      completed_at: null,
    };
    const failedRun = {
      ...runRow,
      id: "01HZ000000000000000000007",
      status: "failed",
      completed_at: null,
    };

    vi.mocked(strategiesApi.listStrategies).mockResolvedValue([
      {
        agent_id: runRow.agent_id,
        display_name: "Mean Reversion v3",
        template: "single_llm",
        decision_cadence_minutes: 60,
        tags: [],
        models: [],
        providers: [],
      },
    ]);
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      queuedRun,
      failedRun,
    ] as unknown as Awaited<ReturnType<typeof evalApi.listRuns>>);

    renderRoute();
    fireEvent.click(await screen.findByRole("button", { name: "Runs" }));

    expect(await screen.findByText(queuedRun.id)).toBeInTheDocument();
    expect(screen.getByText(failedRun.id)).toBeInTheDocument();

    // Status filter is now a SignalSelectMenu button (not a native <select>).
    // Click the "Status" trigger to open the listbox, then click "Pending"
    // (the operator-facing label for the "queued" status value).
    const statusTrigger = screen.getByRole("button", {
      name: (name) => /Status/i.test(name) || /All statuses/i.test(name),
    });
    fireEvent.click(statusTrigger);
    const pendingOption = await screen.findByRole("option", { name: /^Pending$/i });
    fireEvent.click(pendingOption);

    await waitFor(() => {
      expect(screen.queryByText(failedRun.id)).not.toBeInTheDocument();
    });
    expect(screen.getByText(queuedRun.id)).toBeInTheDocument();
  });
});
