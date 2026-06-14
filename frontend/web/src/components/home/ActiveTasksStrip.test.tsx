import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import * as evalApi from "@/api/eval";
import type {
  DeploymentMetricsPatch,
  DeploymentStreamEvent,
} from "@/api/live-deployments";
import type { LiveDeploymentSummary, RunSummary } from "@/api/types.gen";
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

// s78.1: control the per-deployment SSE. Each `openDeploymentStream(id, cb)`
// call records the id, exposes `emit(patch)` to drive a live `metrics` frame,
// and tracks whether `close()` ran (leak detection on unmount / id change).
const streamRegistry = vi.hoisted(() => {
  type Sub = {
    id: string;
    cb: (ev: DeploymentStreamEvent) => void;
    closed: boolean;
  };
  const subs: Sub[] = [];
  function open(id: string, cb: (ev: DeploymentStreamEvent) => void) {
    const sub: Sub = { id, cb, closed: false };
    subs.push(sub);
    return () => {
      sub.closed = true;
    };
  }
  return {
    subs,
    open: vi.fn(open),
    emit(id: string, patch: DeploymentMetricsPatch) {
      for (const s of subs) {
        if (s.id === id && !s.closed) s.cb({ event: "metrics", data: patch });
      }
    },
    last(id: string) {
      return [...subs].reverse().find((s) => s.id === id);
    },
    reset() {
      subs.length = 0;
      streamRegistry.open.mockClear();
    },
  };
});

vi.mock("@/api/live-deployments", () => ({
  openDeploymentStream: (id: string, cb: (ev: DeploymentStreamEvent) => void) =>
    streamRegistry.open(id, cb),
}));

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

function renderStrip(deployments?: LiveDeploymentSummary[]) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <ActiveTasksStrip deployments={deployments} />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

function makeDeployment(
  over: Partial<LiveDeploymentSummary> = {},
): LiveDeploymentSummary {
  return {
    deployment_id: "dep-1",
    strategy_id: "strat-1",
    strategy_name: "Momentum",
    mode: "paper",
    status: "running",
    started_at: new Date(Date.now() - 60_000).toISOString(),
    last_decision_at: new Date(Date.now() - 5 * 60_000).toISOString(),
    venue: "alpaca-paper",
    venue_connected: true,
    deployed_capital_usd: 1000,
    realized_pnl_usd: 0,
    unrealized_pnl_usd: 42.5,
    drawdown_pct: 1.0,
    daily_loss_limit_remaining_usd: 500,
    daily_loss_budget_usd: null,
    stop_at: null,
    risk_veto_count_since_last_visit: null,
    paused: false,
    flatten_requested: false,
    global_safety_paused: false,
    source: "human",
    unavailable_reason: null,
    ...over,
  };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  statusRef.current = undefined;
  pauseMutate.mockReset();
  resumeMutate.mockReset();
  streamRegistry.reset();
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

  // ─── n0k / awm: live & paper deployment rows (CT5 contract) ──────────────

  it("renders a labeled live-deployments group with one row per deployment", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({ deployment_id: "d1", strategy_name: "Momentum", mode: "paper" }),
      makeDeployment({ deployment_id: "d2", strategy_name: "MeanRev", mode: "live" }),
    ]);

    const group = await screen.findByTestId("live-deployments-group");
    // The group is a distinct, labeled section — not folded into the eval queue.
    expect(group.textContent).toMatch(/live\s*&\s*paper/i);
    expect(screen.getByTestId("deployment-row-d1")).toBeInTheDocument();
    expect(screen.getByTestId("deployment-row-d2")).toBeInTheDocument();
    expect(screen.getByText("Momentum")).toBeInTheDocument();
    expect(screen.getByText("MeanRev")).toBeInTheDocument();
  });

  it("renders the mode badge from the deployment's mode field, not inferred", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({ deployment_id: "p", strategy_name: "PaperOne", mode: "paper" }),
      makeDeployment({ deployment_id: "l", strategy_name: "LiveOne", mode: "live" }),
    ]);

    await screen.findByTestId("deployment-row-p");
    const paperRow = screen.getByTestId("deployment-row-p");
    const liveRow = screen.getByTestId("deployment-row-l");
    expect(paperRow.textContent).toMatch(/paper/i);
    expect(liveRow.textContent).toMatch(/live/i);
  });

  it("shows last_decision_at as a relative time", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({
        deployment_id: "d1",
        strategy_name: "Momentum",
        last_decision_at: new Date(Date.now() - 15 * 60_000).toISOString(),
      }),
    ]);

    await screen.findByTestId("deployment-row-d1");
    expect(screen.getByText("15m ago")).toBeInTheDocument();
  });

  it("renders unrealized P&L as a signed dollar amount", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({ deployment_id: "d1", unrealized_pnl_usd: 42.5 }),
    ]);

    const row = await screen.findByTestId("deployment-row-d1");
    const pnl = row.querySelector('[data-testid="deployment-unrealized-pnl"]')!;
    expect(pnl.textContent).toBe("+$42.50");
  });

  // HONESTY: null unrealized P&L renders "—", never a fabricated 0 / $0.
  it("renders '—' (not 0 / $0) when unrealized P&L is null", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({ deployment_id: "d1", unrealized_pnl_usd: null }),
    ]);

    const row = await screen.findByTestId("deployment-row-d1");
    const pnl = row.querySelector('[data-testid="deployment-unrealized-pnl"]')!;
    expect(pnl.textContent).toBe("—");
    expect(pnl.textContent).not.toContain("0");
    expect(pnl.textContent).not.toContain("$");
  });

  it("renders no live group when the deployment set is empty (say-nothing-when-empty)", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      makeRun({ id: "run-1", status: "running", strategy: { id: "s", display_name: "Alpha" } }),
    ]);
    setStatus(null);

    renderStrip([]);

    // The eval queue still renders, but there is no live group.
    await screen.findByText("Alpha");
    expect(screen.queryByTestId("live-deployments-group")).toBeNull();
  });

  // awm (a): Stop shown for human-sourced and for absent/undefined source
  // (legacy runs); HIDDEN only on explicit source==='optimizer'.
  it("shows Stop for a human-sourced deployment", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({ deployment_id: "d1", strategy_name: "HumanRun", source: "human" }),
    ]);

    const row = await screen.findByTestId("deployment-row-d1");
    expect(
      row.querySelector('[aria-label="Stop HumanRun"]'),
    ).not.toBeNull();
  });

  it("calls flattenRun with the deployment id when Stop is clicked", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    vi.mocked(evalApi.flattenRun).mockResolvedValue(
      makeRun({ id: "d1", status: "completed" }) as never,
    );
    setStatus(null);

    renderStrip([
      makeDeployment({ deployment_id: "d1", strategy_name: "HumanRun", source: "human" }),
    ]);

    const stop = await screen.findByTestId("deployment-stop-d1");
    await userEvent.click(stop);

    expect(evalApi.flattenRun).toHaveBeenCalledWith("d1");
  });

  it("shows Stop when source is absent/undefined (legacy run)", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    const dep = makeDeployment({ deployment_id: "d1", strategy_name: "LegacyRun" });
    // Simulate a legacy row with no source field at all.
    delete (dep as Partial<LiveDeploymentSummary>).source;

    renderStrip([dep]);

    const row = await screen.findByTestId("deployment-row-d1");
    expect(
      row.querySelector('[aria-label="Stop LegacyRun"]'),
    ).not.toBeNull();
  });

  it("hides Stop for an optimizer-sourced deployment", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({ deployment_id: "d1", strategy_name: "OptRun", source: "optimizer" }),
    ]);

    const row = await screen.findByTestId("deployment-row-d1");
    expect(row.querySelector('[aria-label="Stop OptRun"]')).toBeNull();
  });

  it("hides Stop for a stopped deployment", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({
        deployment_id: "d1",
        strategy_name: "StoppedRun",
        source: "human",
        status: "stopped",
      }),
    ]);

    const row = await screen.findByTestId("deployment-row-d1");
    expect(row.querySelector('[aria-label="Stop StoppedRun"]')).toBeNull();
  });

  // awm (b): runaway warning chip on a LIVE deployment started > 24h ago.
  it("shows a runaway warning for a LIVE deployment started > 24h ago", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    const longAgo = new Date(Date.now() - 25 * 60 * 60 * 1000).toISOString();
    renderStrip([
      makeDeployment({
        deployment_id: "d1",
        strategy_name: "RunawayLive",
        mode: "live",
        started_at: longAgo,
      }),
    ]);

    const row = await screen.findByTestId("deployment-row-d1");
    expect(
      row.querySelector('[data-testid="deployment-runaway-chip"]'),
    ).not.toBeNull();
  });

  it("does NOT show a runaway warning for a fresh LIVE deployment", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({
        deployment_id: "d1",
        mode: "live",
        started_at: new Date(Date.now() - 60_000).toISOString(),
      }),
    ]);

    const row = await screen.findByTestId("deployment-row-d1");
    expect(
      row.querySelector('[data-testid="deployment-runaway-chip"]'),
    ).toBeNull();
  });

  it("does NOT show a runaway warning for a PAPER deployment older than 24h", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    const longAgo = new Date(Date.now() - 30 * 60 * 60 * 1000).toISOString();
    renderStrip([
      makeDeployment({
        deployment_id: "d1",
        mode: "paper",
        started_at: longAgo,
      }),
    ]);

    const row = await screen.findByTestId("deployment-row-d1");
    expect(
      row.querySelector('[data-testid="deployment-runaway-chip"]'),
    ).toBeNull();
  });

  it("falls back to an honest placeholder name when strategy_name is null", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({ deployment_id: "d1", strategy_name: null }),
    ]);

    const row = await screen.findByTestId("deployment-row-d1");
    expect(row.textContent).toMatch(/unknown strategy/i);
  });

  it("deployment row links to the live run inspector", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({ deployment_id: "dep-9", strategy_name: "LinkMe" }),
    ]);

    await screen.findByTestId("deployment-row-dep-9");
    const link = screen.getByRole("link", { name: /linkme/i });
    expect(link).toHaveAttribute("href", "/live/runs/dep-9");
  });

  it("keeps the eval queue rows alongside the live group", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      makeRun({ id: "run-1", status: "running", strategy: { id: "s", display_name: "EvalAlpha" } }),
    ]);
    setStatus(null);

    renderStrip([
      makeDeployment({ deployment_id: "d1", strategy_name: "LiveBeta" }),
    ]);

    await screen.findByText("EvalAlpha");
    expect(screen.getByText("LiveBeta")).toBeInTheDocument();
    expect(screen.getByTestId("live-deployments-group")).toBeInTheDocument();
  });

  // ─── s78.1: live-ticking unrealized P&L via the per-deployment SSE ─────────

  it("subscribes to the SSE for a running deployment", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([makeDeployment({ deployment_id: "d1", status: "running" })]);

    await screen.findByTestId("deployment-row-d1");
    await waitFor(() =>
      expect(streamRegistry.open).toHaveBeenCalledWith("d1", expect.any(Function)),
    );
  });

  it("overlays the streamed unrealized P&L on top of the poll value", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({
        deployment_id: "d1",
        status: "running",
        unrealized_pnl_usd: 10, // poll value
      }),
    ]);

    const row = await screen.findByTestId("deployment-row-d1");
    const pnl = row.querySelector('[data-testid="deployment-unrealized-pnl"]')!;
    // Before any tick: the poll value shows.
    expect(pnl.textContent).toBe("+$10.00");

    // A live metrics tick arrives — the row ticks to the streamed value.
    await waitFor(() => expect(streamRegistry.last("d1")).toBeTruthy());
    act(() => {
      streamRegistry.emit("d1", { equity_usd: 1000, unrealized_pnl_usd: 88.5 });
    });
    await waitFor(() => expect(pnl.textContent).toBe("+$88.50"));
  });

  // DEGRADE: a heartbeat / tick that carries NO unrealized P&L must not blank
  // the row — it falls back to the poll value (no 0 / blank flash).
  it("falls back to the poll value when the tick omits unrealized P&L", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({
        deployment_id: "d1",
        status: "running",
        unrealized_pnl_usd: 25, // poll value
      }),
    ]);

    const row = await screen.findByTestId("deployment-row-d1");
    const pnl = row.querySelector('[data-testid="deployment-unrealized-pnl"]')!;

    await waitFor(() => expect(streamRegistry.last("d1")).toBeTruthy());
    // Equity-only heartbeat: no capital fields.
    act(() => {
      streamRegistry.emit("d1", { equity_usd: 999 });
    });
    // Still shows the honest poll value, never blanked / zeroed.
    await waitFor(() => expect(pnl.textContent).toBe("+$25.00"));
  });

  // HONESTY: streamed null + poll null => "—", never a fabricated $0.
  it("renders '—' when both the stream and poll have no unrealized P&L", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({
        deployment_id: "d1",
        status: "running",
        unrealized_pnl_usd: null,
      }),
    ]);

    const row = await screen.findByTestId("deployment-row-d1");
    const pnl = row.querySelector('[data-testid="deployment-unrealized-pnl"]')!;
    await waitFor(() => expect(streamRegistry.last("d1")).toBeTruthy());
    act(() => {
      streamRegistry.emit("d1", { equity_usd: 100 }); // no pnl
    });
    await waitFor(() => expect(pnl.textContent).toBe("—"));
    expect(pnl.textContent).not.toContain("0");
    expect(pnl.textContent).not.toContain("$");
  });

  it("closes the EventSource on unmount (no leak)", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    const { unmount } = renderStrip([
      makeDeployment({ deployment_id: "d1", status: "running" }),
    ]);

    await screen.findByTestId("deployment-row-d1");
    await waitFor(() => expect(streamRegistry.last("d1")).toBeTruthy());
    const sub = streamRegistry.last("d1")!;
    expect(sub.closed).toBe(false);

    unmount();
    expect(sub.closed).toBe(true);
  });

  it("closes the old stream and opens a new one when the row id changes", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    const { rerender } = renderStrip([
      makeDeployment({ deployment_id: "d1", status: "running" }),
    ]);
    await screen.findByTestId("deployment-row-d1");
    await waitFor(() => expect(streamRegistry.last("d1")).toBeTruthy());
    const first = streamRegistry.last("d1")!;

    // Replace the row with a different deployment id.
    rerender(
      <MemoryRouter>
        <QueryClientProvider
          client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
        >
          <ActiveTasksStrip
            deployments={[makeDeployment({ deployment_id: "d2", status: "running" })]}
          />
        </QueryClientProvider>
      </MemoryRouter>,
    );

    await screen.findByTestId("deployment-row-d2");
    await waitFor(() => expect(streamRegistry.last("d2")).toBeTruthy());
    expect(first.closed).toBe(true);
    expect(streamRegistry.last("d2")!.closed).toBe(false);
  });

  // A non-running (paused) deployment must NOT open a socket — nothing to tick.
  it("does not open a stream for a paused deployment", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({ deployment_id: "d1", status: "paused" }),
    ]);

    await screen.findByTestId("deployment-row-d1");
    // Flush effects.
    await new Promise((r) => setTimeout(r, 0));
    expect(streamRegistry.last("d1")).toBeUndefined();
  });
});

// ─── awm ETA: stop_at field on DeploymentRow ─────────────────────────────────

describe("ActiveTasksStrip — ETA display (awm / stop_at)", () => {
  it("renders deployment-eta-* with '~...left' text when stop_at is a future timestamp", async () => {
    const futureStopAt = new Date(Date.now() + 2 * 60 * 60 * 1000).toISOString(); // 2h from now
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({
        deployment_id: "dep-eta",
        status: "running",
        stop_at: futureStopAt,
      }),
    ]);

    const etaEl = await screen.findByTestId("deployment-eta-dep-eta");
    expect(etaEl).toBeInTheDocument();
    expect(etaEl.textContent).toMatch(/~.+left/);
  });

  it("does NOT render deployment-eta-* when stop_at is null (no real limit)", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setStatus(null);

    renderStrip([
      makeDeployment({
        deployment_id: "dep-noeta",
        status: "running",
        stop_at: null,
      }),
    ]);

    await screen.findByTestId("deployment-row-dep-noeta");
    expect(screen.queryByTestId("deployment-eta-dep-noeta")).toBeNull();
  });
});
