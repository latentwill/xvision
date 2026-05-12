import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { AuthoringRoute } from "./authoring";
import * as strategyApi from "@/api/strategies";

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
  };
});

vi.mock("@/api/chart", () => ({
  strategyChartKeys: {
    strategy: (id: string) => ["strategy-chart", id],
  },
  getStrategyChart: vi.fn().mockResolvedValue({
    series: [],
  }),
}));

vi.mock("@/components/chart/StrategyChart", () => ({
  StrategyChart: () => <div data-testid="strategy-chart" />,
}));

vi.mock("@/api/settings", () => ({
  settingsKeys: {
    providers: () => ["settings", "providers"],
  },
  listProviders: vi.fn().mockResolvedValue({ providers: [] }),
}));

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

describe("AuthoringRoute risk editor", () => {
  it("edits explicit risk fields and saves them", async () => {
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
      manifest: {
        id: "01TEST",
        display_name: "Trend 4H",
        template: "trend_follower",
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
    });
    vi.mocked(strategyApi.validateDraft).mockResolvedValue({
      id: "01TEST",
      ok: true,
      errors: [],
    });
    vi.mocked(strategyApi.setRiskConfig).mockResolvedValue({
      id: "01TEST",
      applied: "explicit",
    });

    renderRoute();

    const input = (await screen.findByLabelText(
      "Risk per trade (%)",
    )) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "2.50" } });
    fireEvent.click(screen.getByRole("button", { name: "Save risk" }));

    expect(strategyApi.setRiskConfig).toHaveBeenCalledWith("01TEST", {
      explicit: {
        risk_pct_per_trade: 0.025,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });
  });
});
