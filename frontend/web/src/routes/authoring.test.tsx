import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { AttachedAgentRow, AuthoringRoute } from "./authoring";
import * as strategyApi from "@/api/strategies";
import * as agentApi from "@/api/agents";
import * as settingsApi from "@/api/settings";
import * as chartApi from "@/api/chart";

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return {
    ...actual,
    getStrategy: vi.fn(),
    getStrategyRequirements: vi.fn(),
    patchStrategyMetadata: vi.fn(),
    validateDraft: vi.fn(),
    setRiskConfig: vi.fn(),
    updateSlot: vi.fn(),
    setStrategyPipeline: vi.fn(),
    addStrategyAgent: vi.fn(),
    renameStrategyAgentRole: vi.fn(),
    removeStrategyAgent: vi.fn(),
  };
});

vi.mock("@/api/agents", async () => {
  const actual = await vi.importActual<typeof import("@/api/agents")>(
    "@/api/agents",
  );
  return {
    ...actual,
    listAgents: vi.fn(),
    createAgent: vi.fn(),
  };
});

vi.mock("@/api/chart", () => ({
  strategyChartKeys: {
    strategy: (id: string) => ["strategy-chart", id],
  },
  getStrategyChart: vi.fn().mockResolvedValue({ series: [] }),
}));

vi.mock("@/components/chart/v2/surfaces/StrategyHistoryChartV2", () => ({
  StrategyHistoryChartV2: () => <div data-testid="strategy-chart" />,
}));

vi.mock("@/api/settings", () => ({
  settingsKeys: {
    providers: () => ["settings", "providers"],
  },
  listProviders: vi.fn(),
}));

const baseStrategy = {
  manifest: {
    id: "01TEST",
    display_name: "Agent Stack",
    template: "custom",
    creator: "@t",
    plain_summary: "",
    regime_fit: [],
    asset_universe: [],
    decision_cadence_minutes: 240,
    attested_with: [],
    required_tools: [],
    risk_preset_or_config: "balanced",
    published_at: null,
  },
  agents: [{ agent_id: "01DEEPSEEK", role: "trader" }],
  pipeline: { kind: "single" as const },
  regime_slot: null,
  trader_slot: null,
  risk: {
    risk_pct_per_trade: 0.015,
    max_concurrent_positions: 2,
    max_leverage: 3,
    stop_loss_atr_multiple: 2,
    daily_loss_kill_pct: 0.05,
  },
};

const baseAgent = {
  agent_id: "01DEEPSEEK",
  name: "DeepSeek trader",
  description: "",
  tags: [],
  slots: [
    {
      name: "main",
      provider: "openrouter",
      model: "deepseek/deepseek-v4-flash",
      system_prompt: "Trade with discipline.",
      skill_ids: [],
    allowed_tools: [],
      max_tokens: 4096,
    },
  ],
  archived: false,
  created_at: "2026-05-13T14:52:21Z",
  updated_at: "2026-05-13T14:52:21Z",
};

function renderRoute() {
  return render(
    <MemoryRouter initialEntries={["/authoring/01TEST"]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <Routes>
          <Route path="/authoring/:id" element={<AuthoringRoute />} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  localStorage.clear();
  vi.mocked(agentApi.listAgents).mockReset();
  vi.mocked(strategyApi.getStrategy).mockReset();
  vi.mocked(strategyApi.patchStrategyMetadata).mockReset();
  vi.mocked(strategyApi.validateDraft).mockReset();
  vi.mocked(strategyApi.removeStrategyAgent).mockReset();
  vi.mocked(strategyApi.renameStrategyAgentRole).mockReset();
  vi.mocked(chartApi.getStrategyChart).mockReset();
  vi.mocked(settingsApi.listProviders).mockResolvedValue({ providers: [] ,
      default_model: null,
  });

  vi.mocked(agentApi.listAgents).mockResolvedValue([baseAgent]);
  vi.mocked(strategyApi.getStrategy).mockResolvedValue(baseStrategy);
  vi.mocked(strategyApi.getStrategyRequirements).mockResolvedValue({
    requirements: [],
    all_models_satisfied: true,
  });
  vi.mocked(chartApi.getStrategyChart).mockResolvedValue({
    strategy_id: "01TEST",
    run_series: [],
    scenarios: [],
  });
  vi.mocked(strategyApi.validateDraft).mockResolvedValue({
    id: "01TEST",
    ok: true,
    errors: [],
  });
});

afterEach(() => {
  cleanup();
});

describe("AuthoringRoute attached-agent row collapse + inline detail", () => {
  it("shows quick performance before setup fields using strategy eval chart data", async () => {
    vi.mocked(chartApi.getStrategyChart).mockResolvedValue({
      strategy_id: "01TEST",
      scenarios: [["btc-4h", "BTC 4H"]],
      run_series: [
        {
          run_id: "run-a",
          label: "Run A",
          scenario_id: "btc-4h",
          final_pnl_usd: 1250,
          max_drawdown_pct: -4.2,
          sharpe: 1.84,
          equity_normalised: [
            { time: 1, equity_usd: 100 },
            { time: 2, equity_usd: 112.5 },
          ],
        },
        {
          run_id: "run-b",
          label: "Run B",
          scenario_id: "btc-4h",
          final_pnl_usd: -120,
          max_drawdown_pct: -8.1,
          sharpe: 0.44,
          equity_normalised: [
            { time: 1, equity_usd: 100 },
            { time: 2, equity_usd: 98.8 },
          ],
        },
      ],
    });

    renderRoute();

    const quick = await screen.findByText("Quick performance");
    const manifest = screen.getByText("Manifest");
    expect(
      quick.compareDocumentPosition(manifest) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(await screen.findByText("2 evals")).toBeInTheDocument();
    expect(screen.getByText("+$1,250.00")).toBeInTheDocument();
    expect(screen.getByText("1.84")).toBeInTheDocument();
    expect(screen.getByText("−8.10%")).toBeInTheDocument();
    expect(screen.getByTestId("strategy-chart")).toBeInTheDocument();
  });

  it("places eval readiness after the primary setup sections", async () => {
    renderRoute();

    const agents = await screen.findByText("Strategy agents");
    const risk = screen.getByText("Risk");
    const readiness = screen.getByText("Eval readiness");

    expect(
      agents.compareDocumentPosition(readiness) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      risk.compareDocumentPosition(readiness) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("renders the model in the bar even when the row is collapsed", async () => {
    // Pre-set storage so the row mounts in collapsed state.
    localStorage.setItem("xvn:authoring:agent-collapsed:01TEST:trader", "1");

    renderRoute();

    // Bar shows provider/model regardless of collapse state.
    expect(
      await screen.findByText("openrouter / deepseek/deepseek-v4-flash"),
    ).toBeInTheDocument();

    // Detail body is hidden when collapsed — the agent_id only renders in the
    // expanded body, not in the bar.
    expect(screen.queryByText("01DEEPSEEK")).not.toBeInTheDocument();

    const toggle = screen.getByRole("button", { name: "Expand agent" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
  });

  it("toggles collapse state and persists the choice", async () => {
    renderRoute();

    // Default = expanded (no stored preference).
    const collapseBtn = await screen.findByRole("button", {
      name: "Collapse agent",
    });
    expect(collapseBtn).toHaveAttribute("aria-expanded", "true");
    // Wait for agent pool to settle (bar transitions from agent_id to agent name)
    await screen.findByText("DeepSeek trader");
    expect(screen.getByText("01DEEPSEEK")).toBeInTheDocument();

    fireEvent.click(collapseBtn);

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: "Expand agent" }),
      ).toHaveAttribute("aria-expanded", "false");
    });

    expect(
      localStorage.getItem("xvn:authoring:agent-collapsed:01TEST:trader"),
    ).toBe("1");
    expect(screen.queryByText("01DEEPSEEK")).not.toBeInTheDocument();

    // Toggle back to expanded.
    fireEvent.click(screen.getByRole("button", { name: "Expand agent" }));
    await waitFor(() => {
      expect(screen.getByText("01DEEPSEEK")).toBeInTheDocument();
    });
    expect(
      localStorage.getItem("xvn:authoring:agent-collapsed:01TEST:trader"),
    ).toBe("0");
  });

  it("shows model + system prompt inline in the expanded row (no overlay)", async () => {
    // qa-strategy-popup-to-accordion (2026-05-17): the "Open in window"
    // overlay was removed per the dashboard no-popups rule. Agent detail
    // now lives in the row's existing inline expansion. The expanded
    // state must render the same content the old dialog used to show.
    renderRoute();

    // Wait for the agent pool to load so the row's model/system-prompt
    // detail (sourced from `agentById.get`) populates.
    const modelMatches = await screen.findAllByText(
      "openrouter / deepseek/deepseek-v4-flash",
    );
    expect(modelMatches.length).toBeGreaterThanOrEqual(1);

    // No overlay dialog with the old name should exist.
    expect(
      screen.queryByRole("dialog", { name: "Agent trader details" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Open agent in window" }),
    ).not.toBeInTheDocument();

    // Inline detail renders the agent id and system prompt.
    expect(screen.getByText("01DEEPSEEK")).toBeInTheDocument();
    expect(screen.getByText("Trade with discipline.")).toBeInTheDocument();
  });
});

describe("AttachedAgentRow cross-strategy resync", () => {
  const sharedAgentRef = { agent_id: "01DEEPSEEK", role: "trader" };

  function renderRow(strategyId: string) {
    return render(
      <MemoryRouter>
        <AttachedAgentRow
          strategyId={strategyId}
          agentRef={sharedAgentRef}
          index={1}
          agent={baseAgent}
          onRenameRole={() => {}}
          onRemove={() => {}}
        />
      </MemoryRouter>,
    );
  }

  it("reloads collapse state from storage when strategyId changes", async () => {
    // Strategy A: collapsed. Strategy B: expanded (no storage entry).
    localStorage.setItem("xvn:authoring:agent-collapsed:01STRAT_A:trader", "1");

    const { rerender } = renderRow("01STRAT_A");

    expect(
      screen.getByRole("button", { name: "Expand agent" }),
    ).toHaveAttribute("aria-expanded", "false");
    // Detail body (agent_id text) only renders when expanded — confirm hidden.
    expect(screen.queryByText("01DEEPSEEK")).not.toBeInTheDocument();

    // Same React key (`${agent_id}:${role}`) — component instance reused.
    rerender(
      <MemoryRouter>
        <AttachedAgentRow
          strategyId="01STRAT_B"
          agentRef={sharedAgentRef}
          index={1}
          agent={baseAgent}
          onRenameRole={() => {}}
          onRemove={() => {}}
        />
      </MemoryRouter>,
    );

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: "Collapse agent" }),
      ).toHaveAttribute("aria-expanded", "true");
    });
    expect(screen.getByText("01DEEPSEEK")).toBeInTheDocument();
  });
});
