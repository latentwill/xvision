import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { EvalRunsRoute } from "./eval-runs";
import * as chartApi from "@/api/chart";
import * as evalApi from "@/api/eval";
import * as scenariosApi from "@/api/scenarios";
import * as settingsApi from "@/api/settings";
import * as strategyApi from "@/api/strategies";
import type {
  BrokerEntry,
  ProviderRow,
  Scenario,
} from "@/api/types.gen";

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>(
    "@/api/eval",
  );
  return {
    ...actual,
    listRuns: vi.fn(),
    startRun: vi.fn(),
    cancelRun: vi.fn(),
  };
});

vi.mock("@/api/chart", () => ({
  chartKeys: {
    run: (id: string) => ["chart", "run", id],
  },
  getRunChart: vi.fn().mockResolvedValue(null),
}));

vi.mock("@/api/scenarios", async () => {
  const actual = await vi.importActual<typeof import("@/api/scenarios")>(
    "@/api/scenarios",
  );
  return {
    ...actual,
    listScenarios: vi.fn(),
  };
});

vi.mock("@/api/settings", async () => {
  const actual = await vi.importActual<typeof import("@/api/settings")>(
    "@/api/settings",
  );
  return {
    ...actual,
    getBrokers: vi.fn(),
    listProviders: vi.fn(),
  };
});

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return {
    ...actual,
    listStrategies: vi.fn(),
  };
});

function renderRoute(initialEntry = "/eval-runs") {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <EvalRunsRoute />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

function provider(overrides: Partial<ProviderRow> = {}): ProviderRow {
  return {
    name: "openai",
    kind: "openai-compat",
    base_url: "https://api.openai.com/v1",
    api_key_env: "OPENAI_API_KEY",
    api_key_set: true,
    synthetic: false,
    is_default: true,
    enabled_models: ["gpt-4.1-mini"],
    ...overrides,
  };
}

function broker(overrides: Partial<BrokerEntry> = {}): BrokerEntry {
  return {
    name: "Alpaca",
    kind: "alpaca",
    credentials: [],
    configured: true,
    stored: true,
    stored_key_id_suffix: "1234",
    base_url: "https://paper-api.alpaca.markets",
    note: "paper trading",
    ...overrides,
  };
}

function scenario(overrides: Partial<Scenario> = {}): Scenario {
  return {
    id: "user-scenario-4h",
    parent_scenario_id: null,
    source: "User",
    display_name: "User 4H",
    description: "4h bars",
    tags: [],
    notes: null,
    asset_class: "Crypto",
    asset: [{ class: "Crypto", symbol: "BTC/USD", venue_symbol: "BTCUSD" }],
    quote_currency: "Usd",
    time_window: {
      start: "2025-01-01T00:00:00Z",
      end: "2025-01-11T00:00:00Z",
    },
    granularity: "4h",
    timezone: "UTC",
    calendar: "Continuous24x7",
    data_source: { type: "AlpacaHistorical", feed: null, adjustment: "Raw" },
    venue: {
      venue: "Alpaca",
      fees: { maker_bps: 0, taker_bps: 0 },
      slippage: { model: "none" },
      latency: { decision_to_fill_ms: 0 },
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
      cache_key: "user-scenario-4h",
      refresh_policy: { policy: "NeverRefresh" },
      data_fetched_at: null,
    },
    warmup_bars: 200,
    created_at: "2025-01-01T00:00:00Z",
    created_by: "test",
    archived_at: null,
    ...overrides,
  };
}

function mockReady({
  providers = [provider()],
  alpaca = broker(),
  strategyProviderModels = [{ provider: "openai", model: "gpt-4.1-mini" }],
}: {
  providers?: ProviderRow[];
  alpaca?: BrokerEntry;
  strategyProviderModels?: { provider: string; model: string }[];
} = {}) {
  vi.mocked(evalApi.listRuns).mockResolvedValue([]);
  vi.mocked(scenariosApi.listScenarios).mockResolvedValue([scenario()]);
  vi.mocked(settingsApi.listProviders).mockResolvedValue({
    providers,
    default_model: "gpt-4.1-mini",
  });
  vi.mocked(settingsApi.getBrokers).mockResolvedValue({
    alpaca,
    orderly: broker({
      name: "Orderly",
      kind: "orderly",
      configured: false,
      stored: false,
      stored_key_id_suffix: null,
      base_url: null,
      note: "post-v1",
    }),
  });
  vi.mocked(strategyApi.listStrategies).mockResolvedValue([
    {
      agent_id: "01TEST",
      display_name: "Trend 4H",
      template: "trend_follower",
      decision_cadence_minutes: 240,
      providers: ["openai"],
      models: ["gpt-4.1-mini"],
      provider_models: strategyProviderModels,
    },
  ]);
}

describe("EvalRunsRoute", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(chartApi.getRunChart).mockResolvedValue(null as never);
  });

  afterEach(() => {
    cleanup();
  });

  it("preselects strategy from the query string in the start eval dialog", async () => {
    mockReady();

    renderRoute("/eval-runs?strategy=01TEST&start=1");
    await waitFor(() =>
      expect(vi.mocked(evalApi.listRuns)).toHaveBeenCalledWith({
        agent_id: "01TEST",
      }),
    );

    const strategy = (await screen.findByLabelText("Strategy")) as HTMLSelectElement;
    await waitFor(() => expect(strategy.value).toBe("01TEST"));
    expect(screen.getByRole("option", { name: "Trend 4H" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /01TEST/ })).not.toBeInTheDocument();
  });

  it("loads launcher scenarios from the scenario registry", async () => {
    mockReady();

    renderRoute("/eval-runs?start=1");

    await screen.findByRole("option", { name: /User 4H/ });
    expect(scenariosApi.listScenarios).toHaveBeenCalled();
  });

  it("shows eval run duration from started and completed times", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN000000000000000000000",
        agent_id: "01TEST",
        scenario_id: "crypto-bull-q1-2025",
        mode: "backtest",
        status: "completed",
        started_at: "2026-05-13T07:00:00Z",
        completed_at: "2026-05-13T08:15:00Z",
        sharpe: 1.2,
        max_drawdown_pct: 4.5,
        total_return_pct: 8.1,
        error: null,
        actual_input_tokens: 1000,
        actual_output_tokens: 250,
      },
    ]);

    renderRoute();

    expect((await screen.findAllByText("Duration")).length).toBeGreaterThan(0);
    expect(screen.getAllByText("1h 15m").length).toBeGreaterThan(0);
  });

  it("shows live token usage for in-flight runs", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN000000000000000000000",
        agent_id: "01TEST",
        scenario_id: "crypto-bull-q1-2025",
        mode: "backtest",
        status: "running",
        started_at: "2026-05-13T07:00:00Z",
        completed_at: null,
        sharpe: null,
        max_drawdown_pct: null,
        total_return_pct: null,
        error: null,
        actual_input_tokens: 1200,
        actual_output_tokens: 345,
      } as never,
    ]);

    renderRoute();

    await screen.findByText("1 run");
    expect(screen.getAllByText("Tokens").length).toBeGreaterThan(0);
    expect(screen.getAllByText("1,545").length).toBeGreaterThan(0);
  });

  it("marks the running status pill with the animation hook and leaves completed pills static", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN000000000000000000001",
        agent_id: "01TEST",
        scenario_id: "crypto-bull-q1-2025",
        mode: "backtest",
        status: "running",
        started_at: "2026-05-13T07:00:00Z",
        completed_at: null,
        sharpe: null,
        max_drawdown_pct: null,
        total_return_pct: null,
        error: null,
        actual_input_tokens: 100,
        actual_output_tokens: 50,
      },
      {
        id: "01RUN000000000000000000002",
        agent_id: "01TEST",
        scenario_id: "crypto-bull-q1-2025",
        mode: "backtest",
        status: "completed",
        started_at: "2026-05-13T07:00:00Z",
        completed_at: "2026-05-13T08:15:00Z",
        sharpe: 1.2,
        max_drawdown_pct: 4.5,
        total_return_pct: 8.1,
        error: null,
        actual_input_tokens: 1000,
        actual_output_tokens: 250,
      },
    ] as never);

    renderRoute();

    await screen.findByText("2 runs");
    // EvalRunsRoute renders runs in both a card grid and a table, so each
    // row's pill appears twice. With one running row and one completed row,
    // exactly the running-row pills should carry the animation hook.
    const animatedPills = document.querySelectorAll("[data-running='true']");
    expect(animatedPills.length).toBeGreaterThanOrEqual(1);
    for (const pill of Array.from(animatedPills)) {
      expect(pill).toHaveAttribute("aria-busy", "true");
      expect(pill.className).toContain("xvn-pill-animated");
      expect(pill.textContent).toContain("running");
    }
    const staticPills = document.querySelectorAll(
      ".xvn-pill-animated[data-running='false']",
    );
    expect(staticPills.length).toBe(0);
    // The completed pill must not pick up the animation hook.
    const completedPills = Array.from(
      document.querySelectorAll("span"),
    ).filter((el) => el.textContent?.trim() === "completed");
    expect(completedPills.length).toBeGreaterThan(0);
    for (const pill of completedPills) {
      expect(pill).not.toHaveAttribute("data-running");
      expect(pill).not.toHaveAttribute("aria-busy");
      expect(pill.className).not.toContain("xvn-pill-animated");
    }
  });

  it("cancels an in-flight eval run from the list", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN000000000000000000000",
        agent_id: "01TEST",
        scenario_id: "crypto-bull-q1-2025",
        mode: "backtest",
        status: "running",
        started_at: "2026-05-13T07:00:00Z",
        completed_at: null,
        sharpe: null,
        max_drawdown_pct: null,
        total_return_pct: null,
        error: null,
        actual_input_tokens: 1200,
        actual_output_tokens: 345,
      } as never,
    ]);
    vi.mocked(evalApi.cancelRun).mockResolvedValue({
      id: "01RUN000000000000000000000",
      agent_id: "01TEST",
      scenario_id: "crypto-bull-q1-2025",
      mode: "backtest",
      status: "cancelled",
      started_at: "2026-05-13T07:00:00Z",
      completed_at: "2026-05-13T07:05:00Z",
      sharpe: null,
      max_drawdown_pct: null,
      total_return_pct: null,
      error: "cancelled by user",
      actual_input_tokens: 1200,
      actual_output_tokens: 345,
    });

    renderRoute();

    const cancel = await screen.findAllByRole("button", { name: /Cancel run/ });
    fireEvent.click(cancel[0]);

    await waitFor(() =>
      expect(vi.mocked(evalApi.cancelRun).mock.calls[0]?.[0]).toBe(
        "01RUN000000000000000000000",
      ),
    );
  });

  it("blocks eval launch when no provider with credentials is configured", async () => {
    mockReady({ providers: [provider({ api_key_set: false })] });
    vi.mocked(evalApi.startRun).mockResolvedValue({} as never);

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    await screen.findByRole("option", { name: /User 4H/ });
    const scenarioSelect = screen.getByLabelText("Scenario") as HTMLSelectElement;
    fireEvent.change(scenarioSelect, { target: { value: "user-scenario-4h" } });
    const startButton = screen.getByRole("button", { name: "Start" });
    await waitFor(() => expect(startButton).not.toBeDisabled());
    fireEvent.click(startButton);

    expect(await screen.findByText(/Add a provider\/API key/)).toBeInTheDocument();
    expect(evalApi.startRun).not.toHaveBeenCalled();
  });

  it("shows a provider setup action when no providers are configured", async () => {
    mockReady({ providers: [] });
    vi.mocked(evalApi.startRun).mockResolvedValue({} as never);

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    await screen.findByRole("option", { name: /User 4H/ });
    const scenarioSelect = screen.getByLabelText("Scenario") as HTMLSelectElement;
    fireEvent.change(scenarioSelect, { target: { value: "user-scenario-4h" } });
    const startButton = screen.getByRole("button", { name: "Start" });
    await waitFor(() => expect(startButton).not.toBeDisabled());
    fireEvent.click(startButton);

    const setup = await screen.findByRole("link", { name: "Settings -> Providers" });
    expect(setup).toHaveAttribute("href", "/settings/providers");
    expect(evalApi.startRun).not.toHaveBeenCalled();
  });

  it("starts a backtest when the attached strategy agent provider is ready", async () => {
    mockReady({
      providers: [
        provider({
          name: "openrouter",
          base_url: "https://openrouter.ai/api/v1",
          enabled_models: ["deepseek/deepseek-v4-flash"],
        }),
      ],
    });
    vi.mocked(strategyApi.listStrategies).mockResolvedValue([
      {
        agent_id: "01TEST",
        display_name: "DeepSeek Strategy",
        template: "custom",
        decision_cadence_minutes: 240,
        providers: ["openrouter"],
        models: ["deepseek/deepseek-v4-flash"],
      },
    ]);
    vi.mocked(evalApi.startRun).mockResolvedValue({
      summary: {
        id: "01RUN",
        agent_id: "01TEST",
        scenario_id: "user-scenario-4h",
        mode: "backtest",
        status: "queued",
        started_at: null,
        completed_at: null,
        sharpe: null,
        max_drawdown_pct: null,
        total_return_pct: null,
        error: null,
      },
      decisions: [],
      metrics: null,
    } as never);

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    await screen.findByRole("option", { name: /User 4H/ });
    fireEvent.change(screen.getByLabelText("Scenario"), {
      target: { value: "user-scenario-4h" },
    });
    const startButton = screen.getByRole("button", { name: "Start" });
    await waitFor(() => expect(startButton).not.toBeDisabled());
    fireEvent.click(startButton);

    await waitFor(() => {
      expect(vi.mocked(evalApi.startRun).mock.calls[0]?.[0]).toEqual({
        agent_id: "01TEST",
        scenario_id: "user-scenario-4h",
        mode: "backtest",
        params_override: null,
      });
    });
    expect(screen.queryByText(/Pick a provider\/model/)).not.toBeInTheDocument();
  });

  it("blocks eval launch when the selected strategy uses an unconfigured provider", async () => {
    mockReady({
      providers: [
        provider({
          name: "openrouter",
          base_url: "https://openrouter.ai/api/v1",
          enabled_models: ["anthropic/claude-sonnet-4"],
        }),
      ],
    });
    vi.mocked(evalApi.startRun).mockResolvedValue({} as never);

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    await screen.findByRole("option", { name: /User 4H/ });
    const scenarioSelect = screen.getByLabelText("Scenario") as HTMLSelectElement;
    fireEvent.change(scenarioSelect, { target: { value: "user-scenario-4h" } });
    const startButton = screen.getByRole("button", { name: "Start" });
    await waitFor(() => expect(startButton).not.toBeDisabled());
    fireEvent.click(startButton);

    expect(await screen.findByText(/provider 'openai' is not configured/)).toBeInTheDocument();
    expect(evalApi.startRun).not.toHaveBeenCalled();
  });

  it("blocks eval launch when the selected strategy uses a disabled model", async () => {
    mockReady({
      providers: [provider()],
      strategyProviderModels: [
        { provider: "openai", model: "claude-sonnet-4-5" },
      ],
    });
    vi.mocked(evalApi.startRun).mockResolvedValue({} as never);

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    await screen.findByRole("option", { name: /User 4H/ });
    const scenarioSelect = screen.getByLabelText("Scenario") as HTMLSelectElement;
    fireEvent.change(scenarioSelect, { target: { value: "user-scenario-4h" } });
    const startButton = screen.getByRole("button", { name: "Start" });
    await waitFor(() => expect(startButton).not.toBeDisabled());
    fireEvent.click(startButton);

    expect(
      await screen.findByText(
        /model 'claude-sonnet-4-5' is not enabled for provider 'openai'/,
      ),
    ).toBeInTheDocument();
    expect(evalApi.startRun).not.toHaveBeenCalled();
  });

  it("blocks paper eval launch when Alpaca credentials are missing", async () => {
    mockReady({ alpaca: broker({ configured: false, stored: false }) });
    vi.mocked(evalApi.startRun).mockResolvedValue({} as never);

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    await screen.findByRole("option", { name: /User 4H/ });
    const scenarioSelect = screen.getByLabelText("Scenario") as HTMLSelectElement;
    fireEvent.change(scenarioSelect, { target: { value: "user-scenario-4h" } });
    fireEvent.click(screen.getByLabelText("paper"));
    const startButton = screen.getByRole("button", { name: "Start" });
    await waitFor(() => expect(startButton).not.toBeDisabled());
    fireEvent.click(startButton);

    expect(
      await screen.findByText(/Configure Alpaca paper credentials/),
    ).toBeInTheDocument();
    expect(evalApi.startRun).not.toHaveBeenCalled();
  });
});
