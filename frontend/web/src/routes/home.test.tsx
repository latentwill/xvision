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

// OptimizerDigestStrip pulls the last optimizer session via this hook. Default
// to no sessions (strip renders nothing); the reachability test overrides it.
// ActiveTasksStrip pulls the running optimizer cycle via useOptimizerStatus
// (default: idle → no cycle row) and the cycle pause/resume mutations.
vi.mock("@/features/autooptimizer/api", () => ({
  useSessionList: vi.fn(() => ({ data: [] })),
  useOptimizerStatus: vi.fn(() => undefined),
  usePauseCycle: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
  useResumeCycle: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
}));

vi.mock("@/api/agent-runs", () => ({
  agentRunKeys: {
    all: ["agent-runs"],
    list: () => ["agent-runs", "list"],
    run: (id: string) => ["agent-runs", "run", id],
  },
  listAgentRuns: vi.fn().mockResolvedValue([]),
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

  // CT0: home must not imply live trading exists (no agent_runs labeled as
  // "Live strategies / Real money / active live deployments")
  it("does not imply live trading exists on the home dashboard", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    expect(screen.queryByText(/Live strategies/i)).toBeNull();
    expect(screen.queryByText(/Real money/i)).toBeNull();
    expect(screen.queryByText(/active live deployments/i)).toBeNull();
  });

  // S1-W7: NagStrip renders when there are nag items (missing provider key)
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

  // S1-W7: Section DOM order — NagStrip renders last when nag items exist
  it("renders sections in correct order when nag items are present", async () => {
    const { listProviders } = await import("@/api/settings");
    const { useOptimizerStatus } = await import("@/features/autooptimizer/api");
    // CT2: an empty Active tasks panel renders nothing; give it a running
    // optimizer cycle so the section is present to anchor DOM-order assertions.
    vi.mocked(useOptimizerStatus).mockReturnValue({
      active_session: {
        session_id: "sess_order",
        strategy_id: "strat-order",
        state: "running",
        mode: "explore",
        cycles_completed: 1,
        kept_count: 0,
        suspect_count: 0,
        dropped_count: 0,
      },
      active_cycle_id: "cycle_order",
      last_event_seq: 1,
    } as unknown as ReturnType<typeof useOptimizerStatus>);
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

    // ActiveTasksStrip returns null while pending — wait for it to resolve
    await waitFor(() => {
      expect(document.querySelector('[data-testid="active-tasks-strip"]')).not.toBeNull();
    });

    // NagStrip renders when a provider has a missing key
    await waitFor(() => {
      expect(document.querySelector('[data-testid="nag-strip"]')).not.toBeNull();
    });

    const outcomeStrip = document.querySelector('[data-testid="home-outcome-strip"]');
    const activeTasksStrip = document.querySelector('[data-testid="active-tasks-strip"]');
    const liveSummaryStrip = document.querySelector('[data-testid="live-summary-strip"]');
    const criticalFindingsRow = document.querySelector('[data-testid="critical-findings-row"]');
    const strategyOutcomesSummary = document.querySelector('[data-testid="strategy-outcomes-summary"]');
    const nagStrip = document.querySelector('[data-testid="nag-strip"]');

    expect(outcomeStrip).not.toBeNull();
    expect(activeTasksStrip).not.toBeNull();
    expect(liveSummaryStrip).not.toBeNull();
    expect(criticalFindingsRow).not.toBeNull();
    expect(strategyOutcomesSummary).not.toBeNull();
    expect(nagStrip).not.toBeNull();

    // Verify DOM order: outcome strip first, NagStrip last
    const container = activeTasksStrip!.parentElement!;
    const children = Array.from(container.children);
    const idxOutcomeStrip = children.indexOf(outcomeStrip as Element);
    const idxActive = children.indexOf(activeTasksStrip as Element);
    const idxLive = children.indexOf(liveSummaryStrip as Element);
    const idxCritical = children.indexOf(criticalFindingsRow as Element);
    const idxOutcomes = children.indexOf(strategyOutcomesSummary as Element);
    const idxNag = children.indexOf(nagStrip as Element);

    expect(idxOutcomeStrip).toBeLessThan(idxActive);
    expect(idxActive).toBeLessThan(idxLive);
    expect(idxLive).toBeLessThan(idxCritical);
    expect(idxCritical).toBeLessThan(idxOutcomes);
    expect(idxOutcomes).toBeLessThan(idxNag);

    // Reset the optimizer-status override so it does not leak to later tests.
    vi.mocked(useOptimizerStatus).mockReturnValue(undefined);
  });

  // S1-W2: Topbar subtitle updated
  it("shows cockpit subtitle in topbar", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    expect(screen.getByText(/cockpit/)).toBeInTheDocument();
  });

  // Reachability gate: the OptimizerDigestStrip must actually be MOUNTED on the
  // home route — not just exist as a component. (It was built + tested but never
  // wired into home.tsx; a component-only test can't catch that.)
  it("mounts OptimizerDigestStrip on the home route when a session exists", async () => {
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
    expect(screen.getByText(/12 experiments/)).toBeInTheDocument();
  });
});
