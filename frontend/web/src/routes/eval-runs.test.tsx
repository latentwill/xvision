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
  };
});

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
    created_at: "2025-01-01T00:00:00Z",
    created_by: "test",
    archived_at: null,
    ...overrides,
  };
}

function mockReady({
  providers = [provider()],
  alpaca = broker(),
}: {
  providers?: ProviderRow[];
  alpaca?: BrokerEntry;
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
    },
  ]);
}

describe("EvalRunsRoute", () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("preselects strategy from the query string in the start eval dialog", async () => {
    mockReady();

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    const strategy = (await screen.findByLabelText("Strategy")) as HTMLSelectElement;
    await waitFor(() => expect(strategy.value).toBe("01TEST"));
  });

  it("loads launcher scenarios from the scenario registry", async () => {
    mockReady();

    renderRoute("/eval-runs?start=1");

    await screen.findByRole("option", { name: /User 4H/ });
    expect(scenariosApi.listScenarios).toHaveBeenCalled();
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

    expect(await screen.findByText(/Settings -> Providers/)).toBeInTheDocument();
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

    expect(await screen.findByText(/Settings -> Brokers/)).toBeInTheDocument();
    expect(evalApi.startRun).not.toHaveBeenCalled();
  });
});
