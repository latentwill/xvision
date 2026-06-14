import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { HomeRoute } from "./home";
import * as chartApi from "@/api/chart";
import * as evalApi from "@/api/eval";
import * as strategyApi from "@/api/strategies";
import * as deploymentsApi from "@/api/live-deployments";
import type { RunSummary } from "@/api/types.gen";
import {
  LAST_VISIT_LS,
  __resetVisitSessionForTest,
} from "@/features/home/last-visit";

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
    runs: (p?: unknown) => ["eval", "runs", p ?? {}],
  },
  listRuns: vi.fn().mockResolvedValue([]),
  cancelRun: vi.fn(),
}));

vi.mock("@/api/chart", () => ({
  chartKeys: {
    run: (id: string, include?: string[]) =>
      ["chart", "run", id, include ? [...include].sort().join(",") : ""],
    compare: (ids: string[]) =>
      ["chart", "compare", [...ids].sort().join(",")],
  },
  getRunChart: vi.fn(),
  getCompareChart: vi.fn(),
}));

vi.mock("@/api/strategies", () => ({
  strategyKeys: {
    list: () => ["strategies", "list"],
  },
  listStrategies: vi.fn().mockResolvedValue([]),
}));

vi.mock("@/api/eval-review", () => ({
  listCriticalFindings: vi.fn().mockResolvedValue([]),
}));

// 8wn: the cost rollup strip (since-last-visit + this-week) and the digest
// budget denominator pull from /api/cost. Default to null spend / UNSET cap so
// the strips render their honest empty states; individual tests override.
vi.mock("@/api/cost", () => ({
  costKeys: {
    all: ["cost"],
    rollup: (since?: string) => ["cost", "rollup", since || ""],
    budget: () => ["cost", "budget"],
  },
  getCostRollup: vi.fn().mockResolvedValue({
    since: "2026-06-12T00:00:00Z",
    spend_usd: null,
    eval_cost_usd: null,
    optimizer_cost_usd: null,
    daily_cap_usd: null,
  }),
  getCostBudget: vi.fn().mockResolvedValue({ daily_cap_usd: null }),
}));

// OptimizerPanel pulls the ladder/stats/status via these hooks; the digest
// footer pulls the last optimizer session. Default to empty/idle; individual
// tests override.
vi.mock("@/features/autooptimizer/api", () => ({
  useSessionList: vi.fn(() => ({ data: [] })),
  useOptimizerStatus: vi.fn(() => undefined),
  useLadder: vi.fn(() => ({ data: [], isPending: false })),
  useOptimizerStats: vi.fn(() => ({ data: [], isPending: false })),
  usePauseCycle: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
  useResumeCycle: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
}));

vi.mock("@/api/agent-runs", () => ({
  agentRunKeys: {
    all: ["agent-runs"],
    list: (p?: unknown) => ["agent-runs", "list", p ?? {}],
    run: (id: string) => ["agent-runs", "run", id],
  },
  listAgentRuns: vi.fn().mockResolvedValue([]),
}));

// n0k/awm: the home route owns the 5s live-deployments poll; rows flow down
// through AttentionBand → ActiveTasksStrip. s78.1: each running row also opens
// a per-deployment SSE — stub it so the rows mount without a real EventSource.
vi.mock("@/api/live-deployments", () => ({
  deploymentKeys: {
    all: ["live-deployments"],
    list: (p?: unknown) => ["live-deployments", "list", p ?? {}],
  },
  listDeployments: vi.fn().mockResolvedValue([]),
  openDeploymentStream: vi.fn(() => () => {}),
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
  testAlpacaConnection: vi.fn().mockResolvedValue({
    ok: true,
    latency_ms: 12,
    account_status: "ACTIVE",
    equity: null,
    error: null,
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

// Minimal completed RunSummary for the last-visit delta tests.
function homeRun(over: Partial<RunSummary>): RunSummary {
  return {
    id: "run-1",
    agent_id: "strat-1",
    scenario_id: "scn-1",
    strategy: null,
    scenario: null,
    mode: "backtest",
    status: "completed",
    started_at: "2026-06-13T00:30:00Z",
    completed_at: "2026-06-13T01:00:00Z",
    sharpe: 1.2,
    max_drawdown_pct: 0.5,
    total_return_pct: 0.1,
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
    ...over,
  };
}

describe("HomeRoute", () => {
  beforeEach(() => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    vi.mocked(chartApi.getRunChart).mockResolvedValue(null as never);
    vi.mocked(strategyApi.listStrategies).mockResolvedValue([]);
    // Reset the live-deployments poll to an empty population each test. Without
    // this, a `mockResolvedValue` set by one test (the live-row / capital-risk
    // tests) leaks into later tests and the capital-risk strip would mount with
    // stale rows — surfacing its honest "Deployed capital" label in tests that
    // assert no live-money labels appear on an idle node.
    vi.mocked(deploymentsApi.listDeployments).mockResolvedValue([]);
    // Each test starts with a clean last-visit boundary; tests that exercise
    // the populated delta seed localStorage explicitly. Reset the module-scoped
    // page-load-session snapshot too, so each test's first render re-reads the
    // (seeded or cleared) boundary instead of a frozen leftover.
    try {
      localStorage.clear();
    } catch {
      /* storage may be blocked in some environments */
    }
    __resetVisitSessionForTest();
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

  // CT0: home must not imply live trading exists (no agent_runs labeled as
  // "Live strategies / Real money / active live deployments"). The nsk
  // reconcile keeps LiveSummaryStrip's honest aggregate COUNT ("N live") but
  // forbids the old per-run "Live strategies" section copy.
  it("does not imply live trading exists on the home dashboard", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    expect(screen.queryByText(/Real money/i)).toBeNull();
    expect(screen.queryByText(/active live deployments/i)).toBeNull();
    // nsk: the deleted LiveStrategiesSection's "Live strategies" header must
    // never come back — the only at-a-glance live surface is the aggregate
    // LiveSummaryStrip ("Live trading"), not a per-run "Live strategies" list.
    expect(screen.queryByText(/Live strategies/i)).toBeNull();
  });

  // nsk: exactly ONE at-a-glance live-owning surface exists on the home
  // dashboard. LiveSummaryStrip is the aggregate count owner; ActiveTasksStrip
  // is the future home for per-run live/paper ROWS (n0k/CT5) but renders no
  // live rows today. There must be a single live-summary surface, never two
  // competing live lists.
  it("has exactly one at-a-glance live-owning surface", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    await waitFor(() => {
      expect(
        document.querySelector('[data-testid="live-summary-strip"]'),
      ).not.toBeNull();
    });
    expect(
      document.querySelectorAll('[data-testid="live-summary-strip"]'),
    ).toHaveLength(1);
  });

  // Redesign: with nothing live, the execution chip must say so HONESTLY.
  it("renders the central no-live-capital execution chip when nothing is live-money", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    await waitFor(() => {
      expect(
        document.querySelector('[data-testid="execution-chip"]'),
      ).not.toBeNull();
    });
    expect(screen.getByTestId("execution-chip")).toHaveTextContent(
      /no live capital · paper\/sim only/i,
    );
  });

  // e17: the deploy-readiness strip mounts as its own slim safety-gate band,
  // directly under the SafetyPauseBanner and ABOVE the pulse/attention bands.
  it("renders the deploy-readiness strip above the pulse band", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    const strip = await screen.findByTestId("deploy-readiness-strip");
    const pulse = await screen.findByTestId("pulse-band");

    // Strip precedes the pulse band in DOM order (it is a gate, not a nag).
    expect(
      strip.compareDocumentPosition(pulse) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  // jlm: the "since you were last here" delta subtitle renders in the Topbar
  // sub slot (replacing the static strategy-count subtitle).
  it("renders the home delta subtitle in the topbar", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    expect(await screen.findByTestId("home-delta-subtitle")).toBeInTheDocument();
  });

  // jlm read-before-write (end to end): seed a PRIOR boundary plus runs that
  // completed after it; the first paint must show the non-zero delta since that
  // boundary — not the value this visit is about to persist.
  it("shows the non-zero delta since the prior visit on first paint", async () => {
    localStorage.setItem(LAST_VISIT_LS, "2026-06-13T00:00:00Z");
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      homeRun({ id: "a", completed_at: "2026-06-13T01:00:00Z" }),
      homeRun({ id: "b", completed_at: "2026-06-13T02:00:00Z" }),
    ]);

    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    const sub = await screen.findByTestId("home-delta-subtitle");
    await waitFor(() => expect(sub).toHaveTextContent(/2 runs/));
    expect(sub).toHaveTextContent(/since you were last here/i);
    expect(sub).not.toHaveTextContent(/welcome/i);
  });

  // jlm remount-safety: the prior-visit boundary is frozen for the page-load
  // session, so an in-session remount (SPA nav away and back) still measures
  // from the original boundary instead of collapsing to ~0 after this visit's
  // write. The module session is NOT reset between the two renders here.
  it("keeps the same baseline across an in-session remount", async () => {
    localStorage.setItem(LAST_VISIT_LS, "2026-06-13T00:00:00Z");
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      homeRun({ id: "a", completed_at: "2026-06-13T01:00:00Z" }),
      homeRun({ id: "b", completed_at: "2026-06-13T02:00:00Z" }),
    ]);

    const first = renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    await waitFor(() =>
      expect(screen.getByTestId("home-delta-subtitle")).toHaveTextContent(
        /2 runs/,
      ),
    );
    first.unmount();

    // First visit persisted "now" to storage; the remount must still read 2.
    const second = renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    await waitFor(() =>
      expect(screen.getByTestId("home-delta-subtitle")).toHaveTextContent(
        /2 runs/,
      ),
    );
    second.unmount();
  });

  // S1-W7: NagStrip renders (inside the attention band) when there are nag
  // items (missing provider key)
  it("renders nag-strip when a provider has a missing API key", async () => {
    const { listProviders } = await import("@/api/settings");
    vi.mocked(listProviders).mockResolvedValueOnce({
      providers: [
        {
          name: "OpenAI",
          kind: "openai-compat",
          base_url: "https://api.openai.com/v1",
          synthetic: false,
          is_default: false,
          api_key_env: "OPENAI_API_KEY",
          api_key_set: false,
          enabled_models: [],
        },
      ],
      default_model: null,
    });

    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    await waitFor(() => {
      expect(document.querySelector('[data-testid="nag-strip"]')).not.toBeNull();
    });
    // The nag lives inside the attention band, not as a floating footer.
    const nag = document.querySelector('[data-testid="nag-strip"]')!;
    expect(nag.closest('[data-testid="attention-band"]')).not.toBeNull();
  });

  // S1-W7: NagStrip returns null when config is clean (no nag items)
  it("does NOT render nag-strip when config is clean", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    await waitFor(() => {
      // Brokers configured, no missing provider keys → NagStrip returns null
      expect(document.querySelector('[data-testid="nag-strip"]')).toBeNull();
    });
  });

  // n0k/awm: the home route owns the live-deployments 5s poll and passes the
  // rows down to ActiveTasksStrip (via AttentionBand). The query must filter to
  // status=running,paused (the active-deployment window).
  it("wires the live-deployments poll with status=running,paused", async () => {
    const { listDeployments } = await import("@/api/live-deployments");
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    await waitFor(() => {
      expect(vi.mocked(listDeployments)).toHaveBeenCalled();
    });
    const called = vi
      .mocked(listDeployments)
      .mock.calls.some((c) => {
        const p = c[0] as { status?: string } | undefined;
        return p?.status === "running,paused";
      });
    expect(called).toBe(true);
  });

  // bead s78.2: the home passes the SAME last-visit boundary (the jlm
  // delta/cost rollup boundary) as `?since` on the deployments poll, so the
  // backend can count REAL risk vetoes since that boundary.
  it("passes the last-visit boundary as since on the deployments poll", async () => {
    const { listDeployments } = await import("@/api/live-deployments");
    vi.mocked(listDeployments).mockClear();
    const boundary = "2026-06-12T00:00:00.000Z";
    try {
      localStorage.setItem(LAST_VISIT_LS, boundary);
    } catch {
      /* storage may be blocked in some environments */
    }
    // Re-snapshot so the route reads the seeded boundary (beforeEach reset it).
    __resetVisitSessionForTest();

    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    await waitFor(() => expect(vi.mocked(listDeployments)).toHaveBeenCalled());
    const calledWithSince = vi
      .mocked(listDeployments)
      .mock.calls.some((c) => {
        const p = c[0] as { since?: string } | undefined;
        return p?.since === boundary;
      });
    expect(calledWithSince).toBe(true);
  });

  // bead s78.2: on a first visit (no boundary) the home omits `since` so the
  // backend leaves the per-deployment veto count null (can't count since an
  // unknown time) and the chip shows "—".
  it("omits since on the deployments poll when there is no last-visit boundary", async () => {
    const { listDeployments } = await import("@/api/live-deployments");
    // Mock call history accumulates across this file's tests (no global
    // clearMocks); clear it so this assertion only sees THIS render's calls.
    vi.mocked(listDeployments).mockClear();
    // beforeEach already cleared localStorage + reset the snapshot — first visit.
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    await waitFor(() => expect(vi.mocked(listDeployments)).toHaveBeenCalled());
    const everCalledWithSince = vi
      .mocked(listDeployments)
      .mock.calls.some((c) => {
        const p = c[0] as { since?: string } | undefined;
        return p?.since != null && p.since !== "";
      });
    expect(everCalledWithSince).toBe(false);
  });

  // 8s4: the home capital-risk strip mounts from the SAME live-deployments
  // poll (no second fetch) and aggregates the broker-sourced capital fields.
  it("mounts the capital-risk strip from the live-deployments poll", async () => {
    const { listDeployments } = await import("@/api/live-deployments");
    vi.mocked(listDeployments).mockResolvedValue([
      {
        deployment_id: "dep-cap-1",
        strategy_id: "strat-1",
        strategy_name: "CapStrat",
        mode: "paper",
        status: "running",
        started_at: "2026-06-13T00:00:00Z",
        last_decision_at: "2026-06-13T01:00:00Z",
        venue: "alpaca-paper",
        venue_connected: true,
        deployed_capital_usd: 2500,
        realized_pnl_usd: null,
        unrealized_pnl_usd: null,
        drawdown_pct: 3.2,
        daily_loss_limit_remaining_usd: 1800,
        daily_loss_budget_usd: null,
        stop_at: null,
        risk_veto_count_since_last_visit: null,
        paused: false,
        flatten_requested: false,
        global_safety_paused: false,
        source: "human",
        unavailable_reason: null,
      },
    ]);

    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    const strip = await screen.findByTestId("capital-risk-strip");
    expect(strip).toBeInTheDocument();
    // Aggregated deployed capital from the poll renders (not from a second fetch).
    await waitFor(() =>
      expect(strip.querySelector('[data-testid="capital-risk-deployed"]')!.textContent).toMatch(
        /\$2,500/,
      ),
    );
    // Deferred risk-veto chip is "—", never 0 (HONESTY).
    expect(strip.querySelector('[data-testid="capital-risk-veto"]')!.textContent).toBe("—");
  });

  // 8s4: with zero live deployments, say nothing — no empty capital-risk strip
  // implying live capital exists when none does.
  it("does NOT render the capital-risk strip when there are no live deployments", async () => {
    const { listDeployments } = await import("@/api/live-deployments");
    vi.mocked(listDeployments).mockResolvedValue([]);

    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    // Give the deployments poll a chance to resolve, then assert absence.
    await waitFor(() => expect(vi.mocked(listDeployments)).toHaveBeenCalled());
    expect(screen.queryByTestId("capital-risk-strip")).toBeNull();
  });

  // A returned deployment renders as a live row inside the attention band.
  it("renders a live deployment row from the poll inside the attention band", async () => {
    const { listDeployments } = await import("@/api/live-deployments");
    vi.mocked(listDeployments).mockResolvedValue([
      {
        deployment_id: "dep-home-1",
        strategy_id: "strat-1",
        strategy_name: "HomeMomentum",
        mode: "paper",
        status: "running",
        started_at: "2026-06-13T00:00:00Z",
        last_decision_at: "2026-06-13T01:00:00Z",
        venue: "alpaca-paper",
        venue_connected: true,
        deployed_capital_usd: 1000,
        realized_pnl_usd: 0,
        unrealized_pnl_usd: null,
        drawdown_pct: null,
        daily_loss_limit_remaining_usd: null,
        daily_loss_budget_usd: null,
        stop_at: null,
        risk_veto_count_since_last_visit: null,
        paused: false,
        flatten_requested: false,
        global_safety_paused: false,
        source: "human",
        unavailable_reason: null,
      },
    ]);

    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    const row = await screen.findByTestId("deployment-row-dep-home-1");
    expect(row).toBeInTheDocument();
    expect(screen.getByText("HomeMomentum")).toBeInTheDocument();
    // HONESTY: a null unrealized P&L renders "—", never $0, on the wired row.
    const pnl = row.querySelector('[data-testid="deployment-unrealized-pnl"]')!;
    expect(pnl.textContent).toBe("—");
    // The live row lives inside the attention band, not as a floating surface.
    expect(row.closest('[data-testid="attention-band"]')).not.toBeNull();
  });

  // Redesign composition: pulse → attention → optimizer → leaderboard.
  it("renders the four bento sections in order", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    await waitFor(() => {
      expect(document.querySelector('[data-testid="pulse-band"]')).not.toBeNull();
    });

    const ids = [
      "pulse-band",
      "attention-band",
      "optimizer-panel",
      "strategy-leaderboard",
    ];
    const nodes = ids.map((id) =>
      document.querySelector(`[data-testid="${id}"]`),
    );
    for (const node of nodes) expect(node).not.toBeNull();

    // DOM order check via compareDocumentPosition: each section precedes the next.
    for (let i = 0; i < nodes.length - 1; i++) {
      expect(
        nodes[i]!.compareDocumentPosition(nodes[i + 1]!) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    }

    // The live summary + critical findings render inside the attention band.
    const band = nodes[1]!;
    expect(
      band.querySelector('[data-testid="live-summary-strip"]'),
    ).not.toBeNull();
    expect(
      band.querySelector('[data-testid="critical-findings-row"]'),
    ).not.toBeNull();
  });

  // jlm: the Topbar subtitle is now the honest "since you were last here"
  // delta. On a first visit (no stored boundary) it shows the neutral welcome
  // line rather than a "0 runs since…" non-event.
  it("shows the neutral first-visit delta subtitle on a fresh boundary", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    expect(await screen.findByTestId("home-delta-subtitle")).toHaveTextContent(
      /welcome/i,
    );
  });

  // Reachability gate: the optimizer last-run digest must actually be MOUNTED
  // on the home route (inside the Optimizer panel) — not just exist as a
  // component.
  it("mounts the optimizer last-run digest on the home route when a session exists", async () => {
    const { useSessionList } = await import("@/features/autooptimizer/api");
    vi.mocked(useSessionList).mockReturnValue({
      data: [
        {
          session_id: "sess_01HOMEDIGEST",
          strategy_id: "strat-x",
          state: "finished",
          mode: "explore",
          cycles_completed: 12,
          kept_count: 2,
          cost_usd: 4.1,
        },
      ],
    } as unknown as ReturnType<typeof useSessionList>);

    renderRoute();

    await screen.findByRole("heading", { name: "Dashboard" });
    expect(await screen.findByText(/Last run:/)).toBeInTheDocument();
    // zn2 reshaped the digest so the count and the word "experiments" live in
    // adjacent spans; assert on the strip's full text rather than a single
    // contiguous text node.
    const digest = document.querySelector(
      '[data-testid="optimizer-digest-strip"]',
    )!;
    expect(digest.textContent).toMatch(/12\s*experiments/);
    // Mounted inside the Optimizer panel.
    expect(
      digest.closest('[data-testid="optimizer-panel"]'),
    ).not.toBeNull();
  });

  // 8wn: the cost rollup strip must be MOUNTED on the home route (it pulls the
  // since-last-visit + this-week rollups). With null spend / UNSET cap (default
  // mock) it still renders its honest empty state rather than a faked $0.
  it("mounts the cost rollup strip on the home route", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    expect(await screen.findByTestId("cost-rollup-strip")).toBeInTheDocument();
  });

  it("renders real spend in the cost rollup strip and the cap denominator when set (8wn)", async () => {
    const costApi = await import("@/api/cost");
    vi.mocked(costApi.getCostBudget).mockResolvedValue({ daily_cap_usd: 50 });
    // since-last-visit window resolves first; this-week second.
    vi.mocked(costApi.getCostRollup).mockResolvedValue({
      since: "2026-06-12T00:00:00Z",
      spend_usd: 7.5,
      eval_cost_usd: 5.0,
      optimizer_cost_usd: 2.5,
      daily_cap_usd: 50,
    });
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    const strip = await screen.findByTestId("cost-rollup-strip");
    await waitFor(() => expect(strip.textContent).toContain("$7.50"));
    // The this-week window scales the $50 daily cap to its 7-day budget ($350)
    // so cumulative spend compares like-for-like (bead s78.3).
    await waitFor(() => expect(strip.textContent).toContain("$350.00"));
  });

  // Honesty: the Optimizer panel never renders a "Waiting for connection…"
  // placeholder — the empty state names what's missing.
  it("renders the optimizer empty state, never 'Waiting for connection'", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    expect(screen.queryByText(/waiting for connection/i)).toBeNull();
    expect(
      await screen.findByTestId("optimizer-empty"),
    ).toHaveTextContent(/no optimizer cycles recorded yet/i);
  });

  // CT4 — home outcome strip renders completed/inflight eval counts and
  // per-strategy return/Sharpe from existing eval data. Must not show
  // live-money labels (PnL, deployed capital, real money).
  it("renders the home outcome strip with eval metrics, no live-money labels", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    expect(await screen.findByTestId("home-outcome-strip")).toBeInTheDocument();
    expect(screen.getByTestId("home-outcome-completed")).toBeInTheDocument();
    expect(screen.getByTestId("home-outcome-inflight")).toBeInTheDocument();
    expect(screen.getByTestId("home-outcome-best-return")).toBeInTheDocument();
    expect(screen.getByTestId("home-outcome-median-sharpe")).toBeInTheDocument();

    expect(screen.queryByText(/PnL/i)).toBeNull();
    expect(screen.queryByText(/deployed capital/i)).toBeNull();
    expect(screen.queryByText(/real money/i)).toBeNull();
  });

  // ── bead-008: time-window pills scope outcomes + findings ──────────────────

  // The pills render as an inline group and default to All. Default All must
  // reproduce today's first paint, so on mount listRuns is called WITHOUT a
  // `since` param (no extra windowed fetch — the All key collapses onto the
  // base runs key).
  it("renders the time-window pills, defaulting to All with no since on first paint", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    const pills = await screen.findByTestId("time-window-pills");
    expect(pills).toBeInTheDocument();
    expect(pills).toHaveAttribute("role", "group");

    // Default selection is All.
    expect(
      screen.getByRole("button", { name: "All" }),
    ).toHaveAttribute("aria-pressed", "true");

    // First paint: every listRuns call so far is unscoped (no since).
    await waitFor(() => {
      expect(vi.mocked(evalApi.listRuns)).toHaveBeenCalled();
    });
    for (const call of vi.mocked(evalApi.listRuns).mock.calls) {
      const params = call[0] as { since?: string } | undefined;
      expect(params?.since ?? "").toBe("");
    }
  });

  // Selecting a window rescopes the outcomes + findings surfaces: a windowed
  // listRuns call carrying a `since` param is issued. The pulse/leaderboard
  // query (the unscoped 100-row page) must remain since-free.
  it("rescopes outcomes/findings with a since param when a window is selected", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    await waitFor(() => expect(vi.mocked(evalApi.listRuns)).toHaveBeenCalled());

    vi.mocked(evalApi.listRuns).mockClear();

    fireEvent.click(screen.getByRole("button", { name: "7d" }));

    // A windowed fetch carrying `since` is issued for the scoped surfaces.
    await waitFor(() => {
      const withSince = vi
        .mocked(evalApi.listRuns)
        .mock.calls.filter((c) => {
          const p = c[0] as { since?: string } | undefined;
          return !!p?.since;
        });
      expect(withSince.length).toBeGreaterThan(0);
    });

    // The since value the scoped query carries is RFC-3339 / ISO-8601.
    const sinceVal = vi
      .mocked(evalApi.listRuns)
      .mock.calls
      .map((c) => (c[0] as { since?: string } | undefined)?.since)
      .find((s): s is string => !!s)!;
    expect(sinceVal).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/);
  });

  // The pulse/leaderboard surfaces stay on the unscoped runs query: selecting
  // a window must keep the original since-free 100-row hero page in flight
  // (the windowed fetch is an ADDITIONAL scoped query, not a replacement —
  // the unscoped page that feeds pulse/leaderboard is never re-issued WITH a
  // since). The scoped outcomes/findings query shares the page size, so the
  // invariant is "a since-free hero page still exists", not "no limit:100 has
  // a since".
  it("keeps the pulse/leaderboard query unscoped when a window is selected", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    await waitFor(() => expect(vi.mocked(evalApi.listRuns)).toHaveBeenCalled());

    fireEvent.click(screen.getByRole("button", { name: "30d" }));

    // The scoped surfaces issue a windowed fetch …
    await waitFor(() => {
      const withSince = vi
        .mocked(evalApi.listRuns)
        .mock.calls.filter((c) => !!(c[0] as { since?: string } | undefined)?.since);
      expect(withSince.length).toBeGreaterThan(0);
    });

    // … but the unscoped hero page (the pulse/leaderboard source) is still
    // present and since-free.
    const heroSinceFree = vi.mocked(evalApi.listRuns).mock.calls.filter((c) => {
      const p = c[0] as { limit?: number; since?: string } | undefined;
      return p?.limit === 100 && !p?.since;
    });
    expect(heroSinceFree.length).toBeGreaterThan(0);
  });
});
