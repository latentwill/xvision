import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { HomeRoute } from "./home";
import * as chartApi from "@/api/chart";
import * as evalApi from "@/api/eval";
import * as strategyApi from "@/api/strategies";
import * as scenarioApi from "@/api/scenarios";

vi.mock("@/api/safety", () => ({
  safetyKeys: {
    state: () => ["safety", "state"],
  },
  getSafetyState: vi.fn().mockResolvedValue({ paused: false, reason: null }),
}));

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

  // S1-W2: CountCard removed
  it("does NOT render count-card elements", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    expect(document.querySelector('[data-testid="count-card"]')).toBeNull();
  });

  // S1-W2: ControlChartCard removed
  it("does NOT render control-chart-card element", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    expect(document.querySelector('[data-testid="control-chart-card"]')).toBeNull();
  });

  // S1-W2: NagStripStub present
  it("renders nag-strip stub", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    expect(document.querySelector('[data-testid="nag-strip"]')).not.toBeNull();
  });

  // S1-W2: All new section stubs present
  it("renders all section stubs in order", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    const activeTasksStrip = document.querySelector('[data-testid="active-tasks-strip"]');
    const liveStrategiesSection = document.querySelector('[data-testid="live-strategies-section"]');
    const criticalFindingsRow = document.querySelector('[data-testid="critical-findings-row"]');
    const strategyOutcomesList = document.querySelector('[data-testid="strategy-outcomes-list"]');
    const nagStrip = document.querySelector('[data-testid="nag-strip"]');

    expect(activeTasksStrip).not.toBeNull();
    expect(liveStrategiesSection).not.toBeNull();
    expect(criticalFindingsRow).not.toBeNull();
    expect(strategyOutcomesList).not.toBeNull();
    expect(nagStrip).not.toBeNull();

    // Verify DOM order
    const container = activeTasksStrip!.parentElement!;
    const children = Array.from(container.children);
    const idxActive = children.indexOf(activeTasksStrip as Element);
    const idxLive = children.indexOf(liveStrategiesSection as Element);
    const idxCritical = children.indexOf(criticalFindingsRow as Element);
    const idxOutcomes = children.indexOf(strategyOutcomesList as Element);
    const idxNag = children.indexOf(nagStrip as Element);

    expect(idxActive).toBeLessThan(idxLive);
    expect(idxLive).toBeLessThan(idxCritical);
    expect(idxCritical).toBeLessThan(idxOutcomes);
    expect(idxOutcomes).toBeLessThan(idxNag);
  });

  // S1-W2: Topbar subtitle updated
  it("shows cockpit subtitle in topbar", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    expect(screen.getByText(/cockpit/)).toBeInTheDocument();
  });
});
