import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { HomeRoute } from "./home";

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
  it("renders the dashboard shell without the removed home chrome", async () => {
    renderRoute();

    expect(await screen.findByRole("heading", { name: "Dashboard" })).toBeTruthy();
    expect(screen.queryByText("Control Tower")).toBeNull();
    expect(screen.queryByText("On-chain identity")).toBeNull();
    expect(screen.queryByText("Local health")).toBeNull();
  });
});
