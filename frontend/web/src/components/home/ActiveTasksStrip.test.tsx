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

// S0 / O2+O3: control the optimizer-status hook + pause/resume mutations.
const { pauseMutate, resumeMutate, statusRef } = vi.hoisted(() => ({
  pauseMutate: vi.fn(),
  resumeMutate: vi.fn(),
  statusRef: { current: undefined as unknown },
}));

vi.mock("@/features/autooptimizer/api", () => ({
  useOptimizerStatus: () => statusRef.current,
  usePauseCycle: () => ({ mutate: pauseMutate, isPending: false }),
  useResumeCycle: () => ({ mutate: resumeMutate, isPending: false }),
}));

function setStatus(
  activeSession: Record<string, unknown> | null,
  activeCycleId: string | null = null,
) {
  statusRef.current = activeSession
    ? { active_session: activeSession, last_event_seq: 0, active_cycle_id: activeCycleId }
    : { active_session: null, last_event_seq: 0 };
}

function runningSession(overrides: Record<string, unknown> = {}) {
  return {
    session_id: "sess-1",
    strategy_id: "Optimus",
    state: "running",
    mode: "explore",
    cycles_completed: 7,
    kept_count: 3,
    suspect_count: 1,
    dropped_count: 2,
    ...overrides,
  };
}

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
  statusRef.current = undefined;
  pauseMutate.mockReset();
  resumeMutate.mockReset();
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

  // ─── S0 / O2+O3: optimizer cycle in Active tasks ──────────────────────────

  it("shows the running optimizer cycle even when there are no eval runs", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(runningSession(), "cyc-1");

    renderStrip();

    const cycle = await screen.findByTestId("active-optimizer-cycle");
    expect(cycle.textContent).toContain("optimizer");
    expect(cycle.textContent).toContain("Optimus");
    expect(cycle.textContent).toContain("7 cycles");
    // Reachability: a running cycle must surface even with an empty eval list.
    expect(screen.queryByText("No active tasks")).toBeNull();
  });

  it("pauses the in-flight cycle via the cycle-level endpoint", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(runningSession(), "cyc-1");

    renderStrip();

    const pauseBtn = await screen.findByRole("button", { name: /pause optimizer cycle/i });
    await userEvent.click(pauseBtn);
    expect(pauseMutate).toHaveBeenCalledWith("cyc-1");
  });

  it("shows Resume (not Pause) when the cycle is paused", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(runningSession({ state: "paused" }), "cyc-9");

    renderStrip();

    const resumeBtn = await screen.findByRole("button", { name: /resume optimizer cycle/i });
    await userEvent.click(resumeBtn);
    expect(resumeMutate).toHaveBeenCalledWith("cyc-9");
    expect(screen.queryByRole("button", { name: /pause optimizer cycle/i })).toBeNull();
  });

  it("disables pause when no in-flight cycle id is known yet", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(runningSession(), null);

    renderStrip();

    const pauseBtn = await screen.findByRole("button", { name: /pause optimizer cycle/i });
    expect(pauseBtn).toBeDisabled();
  });

  it("does not show a cycle row when the optimizer is idle", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      makeRun({ id: "run-1", status: "running" }),
    ]);
    setStatus(null);

    renderStrip();

    await screen.findByTestId("active-tasks-strip");
    expect(screen.queryByTestId("active-optimizer-cycle")).toBeNull();
  });
});
