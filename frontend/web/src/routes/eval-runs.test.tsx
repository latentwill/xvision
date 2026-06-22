import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import userEvent from "@testing-library/user-event";
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
import type * as EvalApiModule from "@/api/eval";
import type * as ScenariosApiModule from "@/api/scenarios";
import type * as SettingsApiModule from "@/api/settings";
import type * as StrategiesApiModule from "@/api/strategies";

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof EvalApiModule>("@/api/eval");
  return {
    ...actual,
    listRuns: vi.fn(),
    listRunsPaged: vi.fn(),
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
  const actual = await vi.importActual<typeof ScenariosApiModule>(
    "@/api/scenarios",
  );
  return {
    ...actual,
    listScenarios: vi.fn(),
  };
});

vi.mock("@/api/settings", async () => {
  const actual = await vi.importActual<typeof SettingsApiModule>(
    "@/api/settings",
  );
  return {
    ...actual,
    getBrokers: vi.fn(),
    listProviders: vi.fn(),
  };
});

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof StrategiesApiModule>(
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

async function pickScenario(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "Scenario" }));
  await user.click(await screen.findByRole("option", { name: /User 4H/ }));
}

async function pickReviewModel(user: ReturnType<typeof userEvent.setup>, model: string) {
  await user.click(await screen.findByRole("button", { name: "Review model" }));
  await user.click(await screen.findByRole("option", { name: model }));
}

async function openReviewProvider(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "Review provider" }));
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
      overrides: [],
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
    regime_label: null,
    volatility_label: null,
    trend_direction: null,
    regime_derived: false,
    venue_label: "paper",
    safety_limits: null,
    ...overrides,
  } as Scenario;
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
    byreal: broker({
      name: "Byreal",
      kind: "byreal",
      configured: false,
      stored: false,
      stored_key_id_suffix: null,
      base_url: null,
      note: null,
    }),
    degen_arena: broker({
      name: "Degen Arena",
      kind: "degen_arena",
      configured: false,
      stored: false,
      stored_key_id_suffix: null,
      base_url: null,
      note: null,
    }),
    hyperliquid: broker({
      name: "Hyperliquid",
      kind: "hyperliquid",
      configured: false,
      stored: false,
      stored_key_id_suffix: null,
      base_url: null,
      note: null,
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

// `<ResponsiveListCard>` reads `useViewportMode()` which calls
// `window.matchMedia`. jsdom doesn't provide it; install a desktop-
// breakpoint stub so the route mounts without the runtime throwing.
// Tests that need the phone branch can override locally.
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

describe("EvalRunsRoute", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    stubMatchMediaDesktop();
    vi.mocked(chartApi.getRunChart).mockResolvedValue(null as never);
    // Default to empty lookup lists so the strategy/scenario name
    // queries don't return undefined and pollute test output with
    // "Query data cannot be undefined" warnings. Individual tests
    // override via mockReady() when they need rows back.
    vi.mocked(strategyApi.listStrategies).mockResolvedValue([]);
    vi.mocked(scenariosApi.listScenarios).mockResolvedValue([]);
    // The route reads `listRunsPaged`; many legacy tests below still mock
    // `listRuns` directly. Bridge here so a `vi.mocked(evalApi.listRuns)
    // .mockResolvedValue([...])` set in any test automatically flows
    // through the paged envelope. Tests that need a specific `total`
    // (offset slice simulation) can override `listRunsPaged` directly.
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    vi.mocked(evalApi.listRunsPaged).mockImplementation(async (params) => {
      const items = await vi.mocked(evalApi.listRuns)(params);
      return { items, total: items.length };
    });
  });

  afterEach(() => {
    cleanup();
  });


  it("loads launcher scenarios from the scenario registry", async () => {
    const user = userEvent.setup();
    mockReady();

    renderRoute("/eval-runs?start=1");

    await user.click(await screen.findByRole("button", { name: "Scenario" }));
    expect(await screen.findByRole("option", { name: /User 4H/ })).toBeInTheDocument();
    expect(scenariosApi.listScenarios).toHaveBeenCalled();
  });

  it("renders a positive max-drawdown value with the danger tone class", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN000000000000000000005",
        agent_id: "01TEST",
        scenario_id: "crypto-bull-q1-2025",
        strategy: null,
        scenario: null,
        mode: "backtest",
        status: "completed",
        started_at: "2026-05-13T07:00:00Z",
        completed_at: "2026-05-13T07:30:00Z",
        sharpe: 0.8,
        max_drawdown_pct: 4.5,
        total_return_pct: 2.1,
        error: null,
        actual_input_tokens: 100,
        actual_output_tokens: 50,
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: 8,
    paused: false,
    paused_at: null,
    flatten_requested: false,
      },
    ]);

    renderRoute();

    // Find the Max DD cell rendered with a positive (4.50%) value
    // and assert it carries the magnitude-based danger tone class
    // regardless of magnitude (the old helper used `text-warn` for
    // |dd| < 10 and only `text-danger` at >= 10).
    const ddCell = await screen.findByText("+4.50%");
    expect(ddCell.className).toContain("text-danger");
    expect(ddCell.className).not.toContain("text-warn");
  });

  it("shows eval run duration from started and completed times", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN000000000000000000000",
        agent_id: "01TEST",
        scenario_id: "crypto-bull-q1-2025",
        strategy: null,
        scenario: null,
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
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: 8,
    paused: false,
    paused_at: null,
    flatten_requested: false,
      },
    ]);

    renderRoute();

    expect((await screen.findAllByText("Duration")).length).toBeGreaterThan(0);
    expect((await screen.findAllByText("1h 15m")).length).toBeGreaterThan(0);
  });

  it("shows live token usage for in-flight runs", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN000000000000000000000",
        agent_id: "01TEST",
        scenario_id: "crypto-bull-q1-2025",
        strategy: null,
        scenario: null,
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
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: 8,
      } as never,
    ]);

    renderRoute();

    await screen.findByText("1 run");
    expect(screen.getAllByText("Tokens").length).toBeGreaterThan(0);
    expect(screen.getAllByText("1,545").length).toBeGreaterThan(0);
  });

  it("marks in-flight status pills with the animation hook and leaves completed pills static", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN000000000000000000001",
        agent_id: "01TEST",
        scenario_id: "crypto-bull-q1-2025",
        strategy: null,
        scenario: null,
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
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: 8,
      },
      {
        id: "01RUN000000000000000000003",
        agent_id: "01TEST",
        scenario_id: "crypto-bull-q1-2025",
        strategy: null,
        scenario: null,
        mode: "backtest",
        status: "queued",
        started_at: "2026-05-13T07:02:00Z",
        completed_at: null,
        sharpe: null,
        max_drawdown_pct: null,
        total_return_pct: null,
        error: null,
        actual_input_tokens: null,
        actual_output_tokens: null,
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: 8,
      },
      {
        id: "01RUN000000000000000000002",
        agent_id: "01TEST",
        scenario_id: "crypto-bull-q1-2025",
        strategy: null,
        scenario: null,
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
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: 8,
      },
    ] as never);

    renderRoute();

    await screen.findByText("3 runs");
    // EvalRunsRoute renders runs in both a card grid and a table, so each
    // row's pill appears twice. In-flight rows should carry the animation hook.
    const animatedPills = document.querySelectorAll("[data-running='true']");
    expect(animatedPills.length).toBeGreaterThanOrEqual(2);
    const animatedText = Array.from(animatedPills)
      .map((pill) => pill.textContent ?? "")
      .join(" ");
    expect(animatedText).toContain("running");
    expect(animatedText).toContain("queued");
    for (const pill of Array.from(animatedPills)) {
      expect(pill).toHaveAttribute("aria-busy", "true");
      expect(pill.className).toContain("xvn-pill-animated");
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
        strategy: null,
        scenario: null,
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
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: 8,
      } as never,
    ]);
    vi.mocked(evalApi.cancelRun).mockResolvedValue({
      id: "01RUN000000000000000000000",
      agent_id: "01TEST",
      scenario_id: "crypto-bull-q1-2025",
      strategy: null,
      scenario: null,
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
      inference_cost_quote_total: null,
      net_return_pct: null,
      filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: 8,
    paused: false,
    paused_at: null,
    flatten_requested: false,
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
    const user = userEvent.setup();
    mockReady({ providers: [provider({ api_key_set: false })] });
    vi.mocked(evalApi.startRun).mockResolvedValue({} as never);

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    await pickScenario(user);
    const startButton = screen.getByRole("button", { name: "Start" });
    await waitFor(() => expect(startButton).not.toBeDisabled());
    fireEvent.click(startButton);

    expect(await screen.findByText(/Add a provider\/API key/)).toBeInTheDocument();
    expect(evalApi.startRun).not.toHaveBeenCalled();
  });

  it("shows a provider setup action when no providers are configured", async () => {
    const user = userEvent.setup();
    mockReady({ providers: [] });
    vi.mocked(evalApi.startRun).mockResolvedValue({} as never);

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    await pickScenario(user);
    const startButton = screen.getByRole("button", { name: "Start" });
    await waitFor(() => expect(startButton).not.toBeDisabled());
    fireEvent.click(startButton);

    const setup = await screen.findByRole("link", { name: "Settings -> Providers" });
    expect(setup).toHaveAttribute("href", "/settings/providers");
    expect(evalApi.startRun).not.toHaveBeenCalled();
  });


  it("searches the start-eval strategy picker by strategy id", async () => {
    mockReady();
    vi.mocked(strategyApi.listStrategies).mockResolvedValue([
      {
        agent_id: "agent-one",
        display_name: "First strategy",
        template: "custom",
        decision_cadence_minutes: 240,
        providers: ["openai"],
        models: ["gpt-4.1-mini"],
      },
      {
        agent_id: "agent-two",
        display_name: "Second strategy",
        template: "custom",
        decision_cadence_minutes: 240,
        providers: ["openai"],
        models: ["gpt-4.1-mini"],
      },
    ]);
    const user = userEvent.setup();

    renderRoute("/eval-runs?start=1");

    const picker = await screen.findByRole("button", { name: "Strategy" });
    await user.click(picker);
    await user.type(screen.getByRole("textbox", { name: "Search Strategy" }), "agent-two");
    await user.click(await screen.findByRole("option", { name: /Second strategy/i }));

    expect(screen.getByRole("button", { name: "Strategy" })).toHaveTextContent(
      "Second strategy",
    );
  });
  it("starts a backtest when the attached strategy agent provider is ready", async () => {
    const user = userEvent.setup();
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

    await pickScenario(user);
    const startButton = screen.getByRole("button", { name: "Start" });
    await waitFor(() => expect(startButton).not.toBeDisabled());
    fireEvent.click(startButton);

    await waitFor(() => {
      expect(vi.mocked(evalApi.startRun).mock.calls[0]?.[0]).toEqual({
        agent_id: "01TEST",
        scenario_id: "user-scenario-4h",
        mode: "backtest",
        params_override: null,
        auto_fire_review: false,
        review_model: null,
        max_annotations_per_review: 8,
      });
    });
    expect(screen.queryByText(/Pick a provider\/model/)).not.toBeInTheDocument();
  });

  it("persists the selected review model when auto-fire review is enabled", async () => {
    const user = userEvent.setup();
    mockReady({
      providers: [
        provider({
          name: "openrouter",
          base_url: "https://openrouter.ai/api/v1",
          enabled_models: ["deepseek/deepseek-v4-flash", "qwen/qwen3"],
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

    await pickScenario(user);
    await user.click(screen.getByLabelText("auto-run review annotations on completion"));
    await pickReviewModel(user, "qwen/qwen3");
    fireEvent.click(screen.getByRole("button", { name: "Start" }));

    await waitFor(() => {
      expect(vi.mocked(evalApi.startRun).mock.calls[0]?.[0]).toEqual({
        agent_id: "01TEST",
        scenario_id: "user-scenario-4h",
        mode: "backtest",
        params_override: null,
        auto_fire_review: true,
        review_model: { provider: "openrouter", model: "qwen/qwen3" },
        max_annotations_per_review: 8,
      });
    });
  });

  it("allows eval launch when the strategy uses a no-auth provider (Ollama/local)", async () => {
    const user = userEvent.setup();
    mockReady({
      providers: [
        provider({
          name: "ollama",
          base_url: "http://localhost:11434/v1",
          api_key_env: "",
          api_key_set: false,
          enabled_models: ["llama3.2"],
        }),
      ],
    });
    vi.mocked(strategyApi.listStrategies).mockResolvedValue([
      {
        agent_id: "01TEST",
        display_name: "Ollama Strategy",
        template: "custom",
        decision_cadence_minutes: 240,
        providers: ["ollama"],
        models: ["llama3.2"],
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

    await pickScenario(user);
    fireEvent.click(screen.getByRole("button", { name: "Start" }));

    await waitFor(() => expect(evalApi.startRun).toHaveBeenCalled());
    expect(screen.queryByText(/Add a provider\/API key/)).not.toBeInTheDocument();
  });

  it("excludes unconfigured providers from the reviewer dropdown", async () => {
    const user = userEvent.setup();
    mockReady({
      providers: [
        provider({
          name: "openai",
          enabled_models: ["gpt-4.1-mini"],
          api_key_set: true,
        }),
        provider({
          name: "unkeyed",
          enabled_models: ["some-model"],
          api_key_env: "UNKEYED_API_KEY",
          api_key_set: false,
        }),
      ],
    });

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    await screen.findByRole("button", { name: "Scenario" });
    await user.click(screen.getByLabelText("auto-run review annotations on completion"));
    await openReviewProvider(user);
    expect(await screen.findByRole("option", { name: /openai/ })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /unkeyed/ })).not.toBeInTheDocument();
  });

  it("blocks eval launch when the selected strategy uses an unconfigured provider", async () => {
    const user = userEvent.setup();
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

    await pickScenario(user);
    const startButton = screen.getByRole("button", { name: "Start" });
    await waitFor(() => expect(startButton).not.toBeDisabled());
    fireEvent.click(startButton);

    expect(await screen.findByText(/provider 'openai' is not configured/)).toBeInTheDocument();
    expect(evalApi.startRun).not.toHaveBeenCalled();
  });

  it("blocks eval launch when the selected strategy uses a disabled model", async () => {
    const user = userEvent.setup();
    mockReady({
      providers: [provider()],
      strategyProviderModels: [
        { provider: "openai", model: "claude-sonnet-4-5" },
      ],
    });
    vi.mocked(evalApi.startRun).mockResolvedValue({} as never);

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    await pickScenario(user);
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

  it("renders strategy display name and scenario display name in the run list (desktop + mobile)", async () => {
    mockReady();
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN000000000000000000000",
        agent_id: "01TEST",
        scenario_id: "user-scenario-4h",
        strategy: null,
        scenario: null,
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
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: 8,
    paused: false,
    paused_at: null,
    flatten_requested: false,
      },
    ]);

    renderRoute();

    // EvalRunsRoute renders the row through <ResponsiveListCard> which
    // picks desktop (this test stubs matchMedia to desktop). The
    // friendly labels should be visible in the rendered row. Display
    // names come from listStrategies / listScenarios mocks above.
    await waitFor(() =>
      expect(screen.getAllByText(/Trend 4H/).length).toBeGreaterThan(0),
    );
    expect(screen.getAllByText(/User 4H/).length).toBeGreaterThan(0);
    // The raw agent_id should not appear as plain text for the run row.
    // (It will appear as the value of the Strategy filter <option> in
    // the standardized toolbar, but never as a text node inside the
    // row body.) Scope to the table body to assert this cleanly.
    const tables = screen.getAllByRole("table");
    const listTable = tables[0]!;
    expect(within(listTable).queryByText("01TEST")).not.toBeInTheDocument();
  });

  it("renders strategy name as a link to /strategies/:agent_id in the strategy column", async () => {
    mockReady();
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN000000000000000000000",
        agent_id: "01TEST",
        scenario_id: "user-scenario-4h",
        strategy: null,
        scenario: null,
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
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
        auto_fire_review: false,
        review_model: null,
        max_annotations_per_review: 8,
        paused: false,
        paused_at: null,
        flatten_requested: false,
      },
    ]);

    renderRoute();

    const strategyLink = await screen.findByRole("link", { name: "Trend 4H" });
    expect(strategyLink).toHaveAttribute("href", "/strategies/01TEST");
  });

  it("shows the Strategy column header in the desktop table", async () => {
    renderRoute();

    await waitFor(() =>
      expect(screen.getAllByRole("columnheader", { name: "Strategy" }).length).toBeGreaterThan(0),
    );
  });

  it("run column shows only disambiguator and run id, not the strategy display name", async () => {
    mockReady();
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN000000000000000000000",
        agent_id: "01TEST",
        scenario_id: "user-scenario-4h",
        strategy: null,
        scenario: null,
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
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
        auto_fire_review: false,
        review_model: null,
        max_annotations_per_review: 8,
        paused: false,
        paused_at: null,
        flatten_requested: false,
      },
    ]);

    renderRoute();

    await screen.findByText("1 run");
    const tables = screen.getAllByRole("table");
    const listTable = tables[0]!;

    // The run ID must appear in the run column (aria-label checks this)
    const runCell = within(listTable).getByLabelText(
      "Run id 01RUN000000000000000000000",
    );
    expect(runCell).toBeInTheDocument();

    // The strategy display name link lives in its own column, not nested inside the run cell
    const strategyLink = within(listTable).getByRole("link", { name: "Trend 4H" });
    expect(runCell).not.toContainElement(strategyLink);
  });

  it("falls back to the full id when the strategy/scenario lookup misses", async () => {
    mockReady();
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN000000000000000000000",
        agent_id: "01ORPHANSTRAT",
        scenario_id: "deleted-scenario",
        strategy: null,
        scenario: null,
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
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: 8,
    paused: false,
    paused_at: null,
    flatten_requested: false,
      },
    ]);

    renderRoute();

    await waitFor(() =>
      expect(screen.getAllByText("Strategy 01ORPHANSTRAT").length).toBeGreaterThan(0),
    );
    expect(screen.getAllByText("Deleted Scenario").length).toBeGreaterThan(0);
  });

  it("shows forward-test launch controls when forward test is selected", async () => {
    mockReady({ alpaca: broker({ configured: false, stored: false }) });
    vi.mocked(evalApi.startRun).mockResolvedValue({} as never);

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    await screen.findByRole("button", { name: "Scenario" });
    expect(screen.queryByLabelText("paper")).not.toBeInTheDocument();
    expect(screen.getByLabelText("backtest")).toBeChecked();
    fireEvent.click(screen.getByLabelText("forward test"));
    expect(screen.getByLabelText("forward test")).toBeChecked();
    expect(screen.getByLabelText("Forward-test asset")).toBeVisible();
    expect(screen.getByLabelText("Forward-test capital")).toBeVisible();
    expect(screen.getByLabelText("Forward-test bar limit")).toBeVisible();
    expect(screen.getByLabelText("Forward-test warmup bars")).toBeVisible();
  });

  it("offers Degen Arena as a selectable forward-test venue", async () => {
    mockReady();
    vi.mocked(evalApi.startRun).mockResolvedValue({} as never);

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    await screen.findByRole("button", { name: "Scenario" });
    fireEvent.click(screen.getByLabelText("forward test"));

    // The venue button was entirely absent before this change.
    const degenBtn = screen.getByRole("button", { name: "Degen Arena" });
    expect(degenBtn).toBeInTheDocument();

    fireEvent.click(degenBtn);
    expect(degenBtn).toHaveAttribute("aria-pressed", "true");
  });
});
