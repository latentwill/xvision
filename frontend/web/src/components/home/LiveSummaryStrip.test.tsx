import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import * as agentRunsApi from "@/api/agent-runs";
import { MOCK_RUN_LIVE } from "@/features/agent-runs/mock-fixtures";
import type { AgentRunSummary } from "@/api/types-agent-runs";
import { LiveSummaryStrip } from "./LiveSummaryStrip";

vi.mock("@/api/agent-runs", async () => {
  const actual = await vi.importActual<typeof import("@/api/agent-runs")>(
    "@/api/agent-runs",
  );
  return {
    ...actual,
    listAgentRuns: vi.fn(),
  };
});

function renderStrip() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <LiveSummaryStrip />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

const live = (over: Partial<AgentRunSummary> = {}): AgentRunSummary => ({
  ...MOCK_RUN_LIVE.summary,
  status: "running",
  ...over,
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("LiveSummaryStrip", () => {
  it("shows the empty state + 'Deploy a strategy' CTA when there are no live runs", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([]);

    renderStrip();

    await screen.findByText(/no live strategies running/i);
    const cta = screen.getByRole("link", { name: /deploy a strategy/i });
    expect(cta).toHaveAttribute("href", "/strategies");
  });

  it("counts ACTIVE live strategies and links to /live", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([
      live({ run_id: "a" }),
      live({ run_id: "b" }),
    ]);

    renderStrip();

    // "2 active"
    const active = await screen.findByText("2");
    expect(active).toBeInTheDocument();
    expect(screen.getByText(/active/i)).toBeInTheDocument();

    const cta = screen.getByRole("link", { name: /go to live trading/i });
    expect(cta).toHaveAttribute("href", "/live");
  });

  it("separates ACTIVE from PAUSED counts", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([
      live({ run_id: "a" }),
      live({ run_id: "p", paused: true }),
    ]);

    renderStrip();

    await screen.findByText(/active/i);
    // One active, one paused.
    expect(screen.getByText(/paused/i)).toBeInTheDocument();
    // The active count is 1 (the paused one is not counted as active).
    expect(screen.getByText("1", { selector: ".text-info" })).toBeInTheDocument();
    expect(screen.getByText("1", { selector: ".text-warn" })).toBeInTheDocument();
  });

  it("does NOT render a paused count when none are paused", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([live({ run_id: "a" })]);

    renderStrip();

    await screen.findByText(/active/i);
    expect(screen.queryByText(/paused/i)).toBeNull();
  });

  it("excludes terminal (completed) runs from the live counts", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([
      live({ run_id: "a" }),
      live({ run_id: "done", status: "completed" }),
    ]);

    renderStrip();

    // Only one live run → "1 active", and the strip is in live (not empty) mode.
    await screen.findByText(/active/i);
    expect(screen.getByText("1", { selector: ".text-info" })).toBeInTheDocument();
    expect(screen.queryByText(/no live strategies running/i)).toBeNull();
  });

  it("renders a loading indicator while pending and stays in the DOM", () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockReturnValue(new Promise(() => {}));

    renderStrip();

    expect(screen.getByTestId("live-summary-strip")).toBeInTheDocument();
    expect(screen.getByTestId("live-summary-loading")).toBeInTheDocument();
  });

  it("ALWAYS renders the strip + 'Live trading'/'Real money' label, even when empty", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([]);

    renderStrip();

    await screen.findByText(/no live strategies running/i);
    expect(screen.getByTestId("live-summary-strip")).toBeInTheDocument();
    expect(screen.getByText(/live trading/i)).toBeInTheDocument();
    expect(screen.getByText(/real money/i)).toBeInTheDocument();
  });
});
