import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import * as evalApi from "@/api/eval";
import type { RunSummary } from "@/api/types.gen";
import { ActiveTasksStrip } from "./ActiveTasksStrip";

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>(
    "@/api/eval",
  );
  return {
    ...actual,
    listRuns: vi.fn(),
    cancelRun: vi.fn(),
  };
});

function makeRun(overrides: Partial<{
  id: string;
  status: string;
  started_at: string;
  strategy: RunSummary["strategy"];
  agent_id: string;
  scenario_id: string;
}> = {}): RunSummary {
  return {
    id: "run-1",
    agent_id: "agent-1",
    scenario_id: "scenario-1",
    strategy: { id: "strategy-1", display_name: "Alpha" },
    scenario: null,
    mode: "backtest",
    status: "running",
    started_at: new Date(Date.now() - 60_000).toISOString(),
    completed_at: null,
    sharpe: null,
    max_drawdown_pct: null,
    total_return_pct: null,
    error: null,
    actual_input_tokens: null,
    actual_output_tokens: null,
    inference_cost_quote_total: null,
    net_return_pct: null,
    filter_summaries: [],
    auto_fire_review: false,
    review_model: null,
    max_annotations_per_review: null,
    paused: false,
    paused_at: null,
    flatten_requested: false,
    ...overrides,
  };
}

function renderStrip() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <ActiveTasksStrip />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ActiveTasksStrip", () => {
  it("renders running + queued runs, hides completed/failed/cancelled", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      makeRun({ id: "1", status: "running", strategy: { id: "strategy-1", display_name: "Alpha" } }),
      makeRun({ id: "2", status: "queued", started_at: "", strategy: { id: "strategy-2", display_name: "Beta" } }),
      makeRun({ id: "3", status: "completed", strategy: { id: "strategy-3", display_name: "Gamma" } }),
    ]);

    renderStrip();

    await screen.findByText("Alpha");
    expect(screen.getByText("Beta")).toBeInTheDocument();
    expect(screen.queryByText("Gamma")).toBeNull();
  });

  it("renders 'No active tasks' when list is empty", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);

    renderStrip();

    await screen.findByText(/no active tasks/i);
  });

  it("shows '—' for missing started_at", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      makeRun({ id: "1", status: "queued", started_at: "", strategy: { id: "strategy-2", display_name: "Beta" } }),
    ]);

    renderStrip();

    await screen.findByText("Beta");
    expect(screen.getByText("—")).toBeInTheDocument();
  });

  it("shows stuck warning for runs > 2h", async () => {
    const threeHoursAgo = new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString();
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      makeRun({ id: "1", status: "running", started_at: threeHoursAgo }),
    ]);

    renderStrip();

    await screen.findByText(/may be stuck/i);
  });

  it("calls cancelRun with the correct id when Cancel is clicked", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      makeRun({ id: "run-42", status: "running" }),
    ]);
    vi.mocked(evalApi.cancelRun).mockResolvedValue(makeRun({ id: "run-42", status: "cancelled" }) as never);

    renderStrip();

    const cancelBtn = await screen.findByRole("button", { name: /cancel/i });
    await userEvent.click(cancelBtn);

    expect(evalApi.cancelRun).toHaveBeenCalledWith("run-42");
  });

  it("run row links to /eval-runs/:id", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      makeRun({ id: "run-1", status: "running", strategy: { id: "strategy-1", display_name: "Alpha" } }),
    ]);

    renderStrip();

    await screen.findByText("Alpha");
    const link = screen.getByRole("link", { name: /alpha/i });
    expect(link).toHaveAttribute("href", "/eval-runs/run-1");
  });
});
