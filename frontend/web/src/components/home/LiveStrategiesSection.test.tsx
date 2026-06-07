import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import * as agentRunsApi from "@/api/agent-runs";
import { MOCK_RUN_LIVE } from "@/features/agent-runs/mock-fixtures";
import { LiveStrategiesSection } from "./LiveStrategiesSection";

vi.mock("@/api/agent-runs", async () => {
  const actual = await vi.importActual<typeof import("@/api/agent-runs")>(
    "@/api/agent-runs",
  );
  return {
    ...actual,
    listAgentRuns: vi.fn(),
  };
});

function renderSection() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <LiveStrategiesSection />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("LiveStrategiesSection", () => {
  it("renders 'No active live deployments' when listAgentRuns returns []", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([]);

    renderSection();

    await screen.findByText(/no active live deployments/i);
  });

  it("renders a run row with MOCK_RUN_LIVE — shows first 8 chars of run_id and 'Live' badge", async () => {
    const summary = MOCK_RUN_LIVE.summary;
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([summary]);

    renderSection();

    await screen.findByText(summary.run_id.slice(0, 8));
    expect(screen.getAllByText(/live/i).length).toBeGreaterThanOrEqual(1);
  });

  it("section header contains 'Live' and 'Real money' text", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([]);

    renderSection();

    await screen.findByText(/no active live deployments/i);
    // Header h2 contains "Live strategies"
    expect(screen.getByRole("heading", { name: /live strategies/i })).toBeInTheDocument();
    // Sub-label contains "Real money"
    expect(screen.getByText(/real money/i)).toBeInTheDocument();
  });

  it("run row links to /live/:runId", async () => {
    const summary = MOCK_RUN_LIVE.summary;
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([summary]);

    renderSection();

    const link = await screen.findByRole("link", { name: new RegExp(summary.run_id.slice(0, 8)) });
    expect(link).toHaveAttribute("href", `/live/${summary.run_id}`);
  });

  it("renders a loading skeleton while isPending", async () => {
    // Never resolves — keeps query in pending state
    vi.mocked(agentRunsApi.listAgentRuns).mockReturnValue(new Promise(() => {}));

    renderSection();

    // The section must still be in the DOM
    expect(screen.getByTestId("live-strategies-section")).toBeInTheDocument();
    // A skeleton or loading indicator should be present
    const skeleton = document.querySelector("[data-testid='live-strategies-loading']");
    expect(skeleton).not.toBeNull();
  });

  it("ALWAYS renders — does NOT return null when empty (verify fallback renders, not null)", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([]);

    renderSection();

    // The section with the testid must be present even with no data
    await screen.findByText(/no active live deployments/i);
    expect(screen.getByTestId("live-strategies-section")).toBeInTheDocument();
  });
});
