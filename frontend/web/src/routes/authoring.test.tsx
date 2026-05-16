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

import { AuthoringRoute } from "./authoring";
import * as strategyApi from "@/api/strategies";
import * as agentApi from "@/api/agents";
import * as settingsApi from "@/api/settings";

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return {
    ...actual,
    getStrategy: vi.fn(),
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

vi.mock("@/components/chart/StrategyChart", () => ({
  StrategyChart: () => <div data-testid="strategy-chart" />,
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
    required_models: [],
    required_tools: [],
    risk_preset_or_config: "balanced",
    published_at: null,
  },
  agents: [{ agent_id: "01DEEPSEEK", role: "trader" }],
  pipeline: { kind: "single" as const },
  regime_slot: null,
  intern_slot: null,
  trader_slot: null,
  risk: {
    risk_pct_per_trade: 0.015,
    max_concurrent_positions: 2,
    max_leverage: 3,
    stop_loss_atr_multiple: 2,
    daily_loss_kill_pct: 0.05,
  },
  mechanical_params: {},
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
  vi.mocked(strategyApi.validateDraft).mockReset();
  vi.mocked(strategyApi.removeStrategyAgent).mockReset();
  vi.mocked(strategyApi.renameStrategyAgentRole).mockReset();
  vi.mocked(settingsApi.listProviders).mockResolvedValue({ providers: [] });

  vi.mocked(agentApi.listAgents).mockResolvedValue([baseAgent]);
  vi.mocked(strategyApi.getStrategy).mockResolvedValue(baseStrategy);
  vi.mocked(strategyApi.validateDraft).mockResolvedValue({
    id: "01TEST",
    ok: true,
    errors: [],
  });
});

afterEach(() => {
  cleanup();
});

describe("AuthoringRoute attached-agent row collapse + popout", () => {
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

  it("opens a popout dialog that shows model + system prompt", async () => {
    renderRoute();

    const popout = await screen.findByRole("button", {
      name: "Open agent in window",
    });
    fireEvent.click(popout);

    const dialog = await screen.findByRole("dialog", {
      name: "Agent trader details",
    });
    expect(dialog).toBeInTheDocument();

    // Model appears in the bar AND in the dialog body — getAllByText covers both.
    expect(
      screen.getAllByText("openrouter / deepseek/deepseek-v4-flash").length,
    ).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("Trade with discipline.")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Close agent window" }));

    await waitFor(() => {
      expect(
        screen.queryByRole("dialog", { name: "Agent trader details" }),
      ).not.toBeInTheDocument();
    });
  });
});
