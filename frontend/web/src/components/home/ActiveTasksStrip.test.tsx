import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import * as evalApi from "@/api/eval";
import * as liveDeploymentsApi from "@/api/live-deployments";
import type { RunSummary } from "@/api/types.gen";
import type { LiveDeploymentSummary } from "@/api/types.gen/LiveDeploymentSummary";
import { ActiveTasksStrip } from "./ActiveTasksStrip";

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>(
    "@/api/eval",
  );
  return {
    ...actual,
    listRuns: vi.fn(),
    cancelRun: vi.fn(),
    flattenRun: vi.fn(),
  };
});

vi.mock("@/api/live-deployments", async () => {
  const actual = await vi.importActual<typeof import("@/api/live-deployments")>(
    "@/api/live-deployments",
  );
  return {
    ...actual,
    listLiveDeployments: vi.fn(),
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

function makeLiveDeployment(
  overrides: Partial<LiveDeploymentSummary> = {},
): LiveDeploymentSummary {
  return {
    deployment_id: "dep-1",
    strategy_id: "strat-1",
    strategy_name: "PaperAlpha",
    venue_label: "paper",
    status: "running",
    paused: false,
    started_at: new Date(Date.now() - 5 * 60 * 1000).toISOString(),
    last_decision_at: new Date(Date.now() - 3 * 60 * 1000).toISOString(),
    deployed_capital_usd: 10000,
    equity_usd: 10200,
    realized_pnl_usd: 50,
    unrealized_pnl_usd: 150,
    realized_today_usd: 50,
    drawdown_pct: 2,
    daily_loss_limit_remaining_usd: 500,
    risk_veto_count: 0,
    daily_loss_budget_usd: null,
    stop_at: null,
    ...overrides,
  };
}

// Default: each test starts with an empty live-deployments list so existing
// eval-only tests don't need to be touched.
beforeEach(() => {
  vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([]);
});

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

  // CT2: an empty Active tasks panel must not occupy above-fold space.
  // The pending state is also null, so we flush past the resolved empty load
  // (macrotask after the query's microtask resolution) before asserting.
  it("returns null when there are no active evals and no optimizer cycle", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    const { container } = renderStrip();

    await waitFor(() => expect(evalApi.listRuns).toHaveBeenCalled());
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(screen.queryByText(/no active tasks/i)).toBeNull();
    expect(container.firstChild).toBeNull();
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

// ─── n0k: live deployment rows in ActiveTasksStrip ───────────────────────────

describe("ActiveTasksStrip — live deployment rows (n0k)", () => {
  it("renders a LiveDeploymentRow with VenueBadge, last-decision text, and P&L", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({
        deployment_id: "dep-abc",
        strategy_name: "PaperAlpha",
        venue_label: "paper",
        // realized_today_usd=50, unrealized_pnl_usd=150 → runningPnl = +200 → gold ▲
        realized_today_usd: 50,
        unrealized_pnl_usd: 150,
        last_decision_at: new Date(Date.now() - 3 * 60 * 1000).toISOString(),
      }),
    ]);

    renderStrip();

    const row = await screen.findByTestId("live-deployment-row-dep-abc");
    expect(row).toBeInTheDocument();

    // VenueBadge
    expect(row.querySelector("[data-testid='venue-badge-paper']")).not.toBeNull();

    // Strategy name link
    const link = screen.getByRole("link", { name: /PaperAlpha/i });
    expect(link).toHaveAttribute("href", "/eval-runs/dep-abc");

    // Last decision (3 minutes ago)
    expect(row.textContent).toMatch(/decided.*ago/i);

    // P&L: gold tone, ▲ glyph, $200
    expect(row.textContent).toContain("▲");
    expect(row.textContent).toContain("$200");
  });

  it("shows 'no decisions yet' when last_decision_at is null", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({ deployment_id: "dep-nodec", last_decision_at: null }),
    ]);

    renderStrip();

    const row = await screen.findByTestId("live-deployment-row-dep-nodec");
    expect(row.textContent).toMatch(/no decisions yet/i);
  });

  it("shows '—' P&L when both P&L components are null (neutral tone)", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({
        deployment_id: "dep-nopnl",
        unrealized_pnl_usd: null,
        realized_today_usd: null,
      }),
    ]);

    renderStrip();

    const row = await screen.findByTestId("live-deployment-row-dep-nopnl");
    // The P&L element should show —
    expect(row.textContent).toContain("—");
  });

  it("danger glyph (▼) when P&L is negative", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({
        deployment_id: "dep-loss",
        unrealized_pnl_usd: -300,
        realized_today_usd: -100,
      }),
    ]);

    renderStrip();

    const row = await screen.findByTestId("live-deployment-row-dep-loss");
    expect(row.textContent).toContain("▼");
    expect(row.textContent).toContain("$400");
  });

  it("includes live deployment in the total count", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      makeRun({ id: "run-1", status: "running" }),
    ]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({ deployment_id: "dep-1" }),
    ]);

    renderStrip();

    // 1 eval run + 1 live deployment = 2 in flight
    await screen.findByTestId("active-tasks-strip");
    expect(screen.getByText(/2 in flight/i)).toBeInTheDocument();
  });

  it("does NOT show empty state when there are no eval runs but there is a live deployment", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({ deployment_id: "dep-only" }),
    ]);

    const { container } = renderStrip();

    await screen.findByTestId("live-deployment-row-dep-only");
    // The strip renders (not null)
    expect(screen.getByTestId("active-tasks-strip")).toBeInTheDocument();
    // No empty-state text
    expect(container.textContent).not.toMatch(/nothing active/i);
    expect(container.textContent).not.toMatch(/no active tasks/i);
  });

  it("renders live rows AFTER eval rows", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      makeRun({ id: "run-first", status: "running", strategy: { id: "s1", display_name: "EvalRun" } }),
    ]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({ deployment_id: "dep-second", strategy_name: "LiveRow" }),
    ]);

    renderStrip();

    await screen.findByTestId("live-deployment-row-dep-second");
    const strip = screen.getByTestId("active-tasks-strip");
    const allText = strip.textContent ?? "";
    // EvalRun should appear before LiveRow in the rendered output
    expect(allText.indexOf("EvalRun")).toBeLessThan(allText.indexOf("LiveRow"));
  });
});

// ─── awm (S3): Stop control + runaway >24h warning on LiveDeploymentRow ───────

describe("ActiveTasksStrip — Stop control + runaway warning (awm / S3)", () => {
  it("shows Stop button for a running live deployment", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({ deployment_id: "dep-stop", status: "running" }),
    ]);

    renderStrip();

    await screen.findByTestId("live-stop-dep-stop");
    expect(screen.getByTestId("live-stop-dep-stop")).toBeInTheDocument();
  });

  it("calls flattenRun with deployment_id when Stop is clicked", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({ deployment_id: "dep-flatten", status: "running" }),
    ]);
    vi.mocked(evalApi.flattenRun).mockResolvedValue(
      makeRun({ id: "dep-flatten", status: "completed" }) as never,
    );

    renderStrip();

    const stopBtn = await screen.findByTestId("live-stop-dep-flatten");
    await userEvent.click(stopBtn);

    expect(evalApi.flattenRun).toHaveBeenCalledWith("dep-flatten");
  });

  it("hides Stop button when status is completed", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({ deployment_id: "dep-done", status: "completed" }),
    ]);

    renderStrip();

    await screen.findByTestId("live-deployment-row-dep-done");
    expect(screen.queryByTestId("live-stop-dep-done")).toBeNull();
  });

  it("hides Stop button when status is failed", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({ deployment_id: "dep-fail", status: "failed" }),
    ]);

    renderStrip();

    await screen.findByTestId("live-deployment-row-dep-fail");
    expect(screen.queryByTestId("live-stop-dep-fail")).toBeNull();
  });

  it("hides Stop button when status is cancelled", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({ deployment_id: "dep-cancelled", status: "cancelled" }),
    ]);

    renderStrip();

    await screen.findByTestId("live-deployment-row-dep-cancelled");
    expect(screen.queryByTestId("live-stop-dep-cancelled")).toBeNull();
  });

  // ─── Runaway >24h warning ─────────────────────────────────────────────────

  it("shows runaway pill when a running deployment has been running >24h", async () => {
    const twentyFiveHoursAgo = new Date(
      Date.now() - 25 * 60 * 60 * 1000,
    ).toISOString();
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({
        deployment_id: "dep-runaway",
        status: "running",
        started_at: twentyFiveHoursAgo,
      }),
    ]);

    renderStrip();

    await screen.findByTestId("live-runaway-dep-runaway");
    expect(screen.getByTestId("live-runaway-dep-runaway")).toBeInTheDocument();
  });

  it("does NOT show runaway pill when a running deployment started only 1h ago", async () => {
    const oneHourAgo = new Date(Date.now() - 1 * 60 * 60 * 1000).toISOString();
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({
        deployment_id: "dep-fresh",
        status: "running",
        started_at: oneHourAgo,
      }),
    ]);

    renderStrip();

    await screen.findByTestId("live-deployment-row-dep-fresh");
    expect(screen.queryByTestId("live-runaway-dep-fresh")).toBeNull();
  });
});

// ─── awm ETA: stop_at field on LiveDeploymentRow ─────────────────────────────

describe("ActiveTasksStrip — ETA display (awm / stop_at)", () => {
  it("renders live-eta-* with '~...left' text when stop_at is a future timestamp", async () => {
    const futureStopAt = new Date(Date.now() + 2 * 60 * 60 * 1000).toISOString(); // 2h from now
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({
        deployment_id: "dep-eta",
        status: "running",
        stop_at: futureStopAt,
      }),
    ]);

    renderStrip();

    const etaEl = await screen.findByTestId("live-eta-dep-eta");
    expect(etaEl).toBeInTheDocument();
    expect(etaEl.textContent).toMatch(/~.+left/);
  });

  it("does NOT render live-eta-* when stop_at is null (no real limit)", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);
    vi.mocked(liveDeploymentsApi.listLiveDeployments).mockResolvedValue([
      makeLiveDeployment({
        deployment_id: "dep-noeta",
        status: "running",
        stop_at: null,
      }),
    ]);

    renderStrip();

    await screen.findByTestId("live-deployment-row-dep-noeta");
    expect(screen.queryByTestId("live-eta-dep-noeta")).toBeNull();
  });
});
