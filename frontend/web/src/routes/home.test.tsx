import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { HomeRoute } from "./home";
import * as chartApi from "@/api/chart";
import * as evalApi from "@/api/eval";
import * as strategyApi from "@/api/strategies";
import * as scenarioApi from "@/api/scenarios";

vi.mock("@/api/health", () => ({
  healthKeys: {
    report: () => ["health", "report"],
  },
  getHealth: vi.fn().mockResolvedValue({
    status: "ok",
    probes: [],
  }),
}));

vi.mock("@/api/eval", () => ({
  evalKeys: {
    runs: () => ["eval", "runs"],
  },
  listRuns: vi.fn().mockResolvedValue([]),
}));

vi.mock("@/api/chart", () => ({
  chartKeys: {
    run: (id: string) => ["chart", "run", id],
  },
  getRunChart: vi.fn(),
}));

vi.mock("@/api/scenarios", () => ({
  scenarioKeys: {
    list: () => ["scenarios", "list"],
  },
  listScenarios: vi.fn().mockResolvedValue([]),
}));

vi.mock("@/api/strategies", () => ({
  strategyKeys: {
    list: () => ["strategies", "list"],
  },
  listStrategies: vi.fn().mockResolvedValue([]),
}));

vi.mock("@/api/agents", () => ({
  agentKeys: {
    list: () => ["agents", "list"],
  },
  listAgents: vi.fn().mockResolvedValue([]),
}));

vi.mock("@/api/settings", () => ({
  settingsKeys: {
    providers: () => ["settings", "providers"],
    brokers: () => ["settings", "brokers"],
  },
  listProviders: vi.fn().mockResolvedValue({ providers: [] }),
  getBrokers: vi.fn().mockResolvedValue({
    executor: "paper",
    alpaca: {
      name: "Alpaca",
      configured: true,
      credentials: [],
    },
  }),
}));

function renderRoute() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <HomeRoute />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("HomeRoute", () => {
  beforeEach(() => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    vi.mocked(chartApi.getRunChart).mockResolvedValue(null as never);
    vi.mocked(strategyApi.listStrategies).mockResolvedValue([]);
    vi.mocked(scenarioApi.listScenarios).mockResolvedValue([]);
  });

  it("renders the dashboard shell without the removed home chrome", async () => {
    renderRoute();

    expect(await screen.findByRole("heading", { name: "Dashboard" })).toBeTruthy();
    expect(screen.queryByText("Control Tower")).toBeNull();
    expect(screen.queryByText("On-chain identity")).toBeNull();
    expect(screen.queryByText("Local health")).toBeNull();
  });

  it("renders the latest-eval sub-line on the Chart snapshot card", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUN0001",
        agent_id: "01STRAT",
        scenario_id: "user-scenario-4h",
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
      } as never,
    ]);
    vi.mocked(strategyApi.listStrategies).mockResolvedValue([
      {
        agent_id: "01STRAT",
        display_name: "Trend 4H",
        template: "trend_follower",
        decision_cadence_minutes: 240,
        providers: ["openai"],
        models: ["gpt-4.1-mini"],
      } as never,
    ]);
    vi.mocked(scenarioApi.listScenarios).mockResolvedValue([
      {
        id: "user-scenario-4h",
        display_name: "User 4H",
      } as never,
    ]);

    renderRoute();

    await waitFor(() =>
      expect(screen.getByText(/Latest eval/)).toBeInTheDocument(),
    );
    const sub = screen.getByText(/Latest eval/);
    expect(sub.textContent).toContain("Trend 4H");
    expect(sub.textContent).toContain("User 4H");
  });

  it("links the chart card's open-eval action to the latest run when one exists", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "01RUNXYZ",
        agent_id: "01STRAT",
        scenario_id: "user-scenario-4h",
        mode: "backtest",
        status: "completed",
        started_at: "2026-05-13T07:00:00Z",
        completed_at: null,
        sharpe: null,
        max_drawdown_pct: null,
        total_return_pct: null,
        error: null,
        actual_input_tokens: 0,
        actual_output_tokens: 0,
      } as never,
    ]);

    renderRoute();

    await waitFor(() => {
      const link = screen.getByRole("link", { name: /open eval/ });
      expect(link).toHaveAttribute("href", "/eval-runs/01RUNXYZ");
    });
  });
});
