// QA #4 + Q1: Strategy detail page requirements panel + eval/go-live gate.
//
// Covers:
//  - the panel renders ✓ for satisfied and ⚠ (highlighted) for missing.
//  - the Run eval action is disabled with a reason when a required MODEL is
//    unsatisfied, and a normal enabled link when all models are satisfied.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { AuthoringRoute } from "./authoring";
import * as strategyApi from "@/api/strategies";
import * as agentApi from "@/api/agents";
import * as settingsApi from "@/api/settings";
import * as chartApi from "@/api/chart";

vi.mock("@/api/strategies", async () => {
  const actual =
    await vi.importActual<typeof import("@/api/strategies")>(
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
  const actual =
    await vi.importActual<typeof import("@/api/agents")>("@/api/agents");
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
    profile: () => ["settings", "profile"],
  },
  listProviders: vi.fn(),
  getProfile: vi.fn().mockResolvedValue({ display_name: null, persisted: false }),
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
  vi.mocked(strategyApi.getStrategyRequirements).mockReset();
  vi.mocked(strategyApi.validateDraft).mockReset();
  vi.mocked(chartApi.getStrategyChart).mockReset();
  vi.mocked(settingsApi.listProviders).mockResolvedValue({
    providers: [],
    default_model: null,
  });

  vi.mocked(agentApi.listAgents).mockResolvedValue([baseAgent]);
  vi.mocked(strategyApi.getStrategy).mockResolvedValue(baseStrategy);
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

describe("Strategy requirements panel", () => {
  it("renders ✓ for a satisfied requirement and ⚠ (highlighted) for a missing one", async () => {
    vi.mocked(strategyApi.getStrategyRequirements).mockResolvedValue({
      all_models_satisfied: false,
      requirements: [
        {
          name: "openrouter/deepseek-chat",
          kind: "model",
          satisfied: true,
        },
        {
          name: "anthropic/claude-not-enabled",
          kind: "model",
          satisfied: false,
          reason: "model_disabled",
          hint: "enable it in Settings → Providers",
        },
      ],
    });

    renderRoute();

    const chips = await screen.findAllByTestId("strategy-requirement-chip");
    expect(chips).toHaveLength(2);

    const satisfied = chips.find(
      (c) => c.getAttribute("data-satisfied") === "true",
    );
    const missing = chips.find(
      (c) => c.getAttribute("data-satisfied") === "false",
    );
    expect(satisfied).toBeDefined();
    expect(missing).toBeDefined();
    expect(satisfied?.textContent).toContain("✓");
    expect(satisfied?.textContent).toContain("openrouter/deepseek-chat");
    expect(missing?.textContent).toContain("⚠");
    expect(missing?.textContent).toContain("anthropic/claude-not-enabled");
    // Missing chip carries the warn tone (amber border).
    expect(missing?.className).toContain("amber");

    // A Configure CTA appears for the missing requirement.
    const configureLinks = screen.getAllByRole("link", { name: "Configure" });
    expect(configureLinks.length).toBeGreaterThan(0);
  });
});

describe("Run eval gate", () => {
  it("disables Run eval with a reason when a required model is unsatisfied", async () => {
    vi.mocked(strategyApi.getStrategyRequirements).mockResolvedValue({
      all_models_satisfied: false,
      requirements: [
        {
          name: "anthropic/claude-not-enabled",
          kind: "model",
          satisfied: false,
          reason: "model_disabled",
          hint: "enable it in Settings → Providers",
        },
      ],
    });

    renderRoute();

    // The disabled action renders as a button, not a link.
    const runButton = await screen.findByRole("button", { name: /Run eval/ });
    expect(runButton).toBeDisabled();
    // No enabled Run eval link.
    expect(
      screen.queryByRole("link", { name: /Run eval/ }),
    ).not.toBeInTheDocument();
    // Reason is surfaced inline.
    expect(
      screen.getByText(/Configure the required model\(s\) before running/),
    ).toBeInTheDocument();
  });

  it("renders Run eval as an enabled link when all models are satisfied", async () => {
    vi.mocked(strategyApi.getStrategyRequirements).mockResolvedValue({
      all_models_satisfied: true,
      requirements: [
        {
          name: "openrouter/deepseek-chat",
          kind: "model",
          satisfied: true,
        },
      ],
    });

    renderRoute();

    const runLink = await screen.findByRole("link", { name: /Run eval/ });
    expect(runLink).toHaveAttribute(
      "href",
      "/eval-runs?strategy=01TEST&start=1",
    );
    expect(
      screen.queryByText(/Configure the required model\(s\) before running/),
    ).not.toBeInTheDocument();
  });

  it("keeps Run eval enabled while requirements are still loading", async () => {
    // Never resolves — simulates an in-flight requirements query.
    vi.mocked(strategyApi.getStrategyRequirements).mockReturnValue(
      new Promise<strategyApi.StrategyRequirements>(() => {}),
    );

    renderRoute();

    // While loading, the action stays the normal enabled link (no gate flash).
    const runLink = await screen.findByRole("link", { name: /Run eval/ });
    expect(runLink).toHaveAttribute(
      "href",
      "/eval-runs?strategy=01TEST&start=1",
    );
    await waitFor(() => {
      expect(
        screen.queryByText(/Configure the required model\(s\) before running/),
      ).not.toBeInTheDocument();
    });
  });
});
