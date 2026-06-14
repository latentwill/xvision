// Issue #12 / QA #8: Strategy detail page marketplace provenance strip.
//
// Provenance lives in a backend sidecar (NOT on the Strategy artifact), so the
// page fetches it via getStrategyMarketplace(id). Covers:
//  - the strip renders creator + price + license id + a real explorer href
//    when provenance is present (and "Free" when price is 0).
//  - NO strip when getStrategyMarketplace resolves null (non-marketplace).
//  - the explorer link is absent/disabled (not an `href="#"`) when
//    explorer_url is absent.
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
    getStrategyMarketplace: vi.fn(),
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
  },
  listProviders: vi.fn(),
}));

const baseStrategy: strategyApi.Strategy = {
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
  } as strategyApi.Strategy["manifest"],
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
  vi.mocked(strategyApi.getStrategyMarketplace).mockReset();
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
  vi.mocked(strategyApi.getStrategyRequirements).mockResolvedValue({
    all_models_satisfied: true,
    requirements: [],
  });
  // Default: non-marketplace strategy → no provenance.
  vi.mocked(strategyApi.getStrategyMarketplace).mockResolvedValue(null);
  vi.mocked(strategyApi.validateDraft).mockResolvedValue({
    id: "01TEST",
    ok: true,
    errors: [],
  });
});

afterEach(() => {
  cleanup();
});

describe("Marketplace provenance strip", () => {
  it("renders creator, price, license id, and a real explorer href when provenance is present", async () => {
    vi.mocked(strategyApi.getStrategyMarketplace).mockResolvedValue({
      listing_id: "42",
      tier: "open",
      creator: "Momentum Maxi",
      price_usdc: 12.5,
      license_token_id: "42",
      network: "mantle-sepolia",
      explorer_url:
        "https://explorer.sepolia.mantle.xyz/token/0x1111/instance/42",
    });

    renderRoute();

    const strip = await screen.findByTestId("marketplace-provenance-strip");
    expect(strip).toBeInTheDocument();
    expect(strip.textContent).toContain("Acquired from marketplace");
    expect(strip.textContent).toContain("Momentum Maxi");
    expect(strip.textContent).toContain("12.5 USDC");
    expect(strip.textContent).toContain("#42");

    const explorerLink = screen.getByRole("link", { name: /View on Explorer/ });
    expect(explorerLink).toHaveAttribute(
      "href",
      "https://explorer.sepolia.mantle.xyz/token/0x1111/instance/42",
    );
    expect(explorerLink).toHaveAttribute("target", "_blank");
    expect(explorerLink).toHaveAttribute("rel", "noreferrer");
  });

  it("shows 'Free' when the price paid is 0", async () => {
    vi.mocked(strategyApi.getStrategyMarketplace).mockResolvedValue({
      listing_id: "7",
      tier: "open",
      creator: "0xseller",
      price_usdc: 0,
      license_token_id: "7",
      network: "mantle-sepolia",
      explorer_url: "https://explorer.sepolia.mantle.xyz/token/0x1/instance/7",
    });

    renderRoute();

    const strip = await screen.findByTestId("marketplace-provenance-strip");
    expect(strip.textContent).toContain("Free");
    expect(strip.textContent).not.toContain("USDC");
  });

  it("renders no strip for a non-marketplace strategy", async () => {
    // getStrategyMarketplace defaults to null in beforeEach.
    renderRoute();

    // Wait for the page to load (manifest card present), then assert no strip.
    await screen.findByText("Manifest");
    expect(
      screen.queryByTestId("marketplace-provenance-strip"),
    ).not.toBeInTheDocument();
  });

  it("renders a disabled (non-link) explorer label when explorer_url is absent", async () => {
    vi.mocked(strategyApi.getStrategyMarketplace).mockResolvedValue({
      listing_id: "99",
      tier: "sealed",
      creator: "0xseller",
      price_usdc: 49,
      license_token_id: "99",
      network: "mantle-sepolia",
      // explorer_url omitted — the chain was unconfigured at import, so the
      // backend `Option<String>` (skip_serializing_if) leaves the key off
      // the wire entirely.
    });

    renderRoute();

    const strip = await screen.findByTestId("marketplace-provenance-strip");
    expect(strip).toBeInTheDocument();
    // No explorer anchor at all (and definitely not a dead href="#").
    expect(
      screen.queryByRole("link", { name: /View on Explorer/ }),
    ).not.toBeInTheDocument();
    await waitFor(() => {
      const deadLink = strip.querySelector('a[href="#"]');
      expect(deadLink).toBeNull();
    });
    // A muted, non-interactive fallback label is shown instead.
    expect(strip.textContent).toContain("Explorer unavailable");
  });
});
