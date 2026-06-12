import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { HomeRoute } from "./home";
import * as chartApi from "@/api/chart";
import * as evalApi from "@/api/eval";
import * as strategyApi from "@/api/strategies";

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

    expect(screen.queryByText(/Real money/i)).toBeNull();
    expect(screen.queryByText(/active live deployments/i)).toBeNull();
  });

  // Redesign: with nothing live, the execution chip must say so HONESTLY.
  it("renders the honest paper execution chip when nothing is live-money", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    await waitFor(() => {
      expect(
        document.querySelector('[data-testid="execution-chip"]'),
      ).not.toBeNull();
    });
    expect(screen.getByTestId("execution-chip")).toHaveTextContent(
      /paper · no live capital deployed/i,
    );
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

  // S1-W2: Topbar subtitle shows strategy count
  it("shows strategy count subtitle in topbar", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });
    expect(screen.getByText("0 strategies")).toBeInTheDocument();
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
    expect(screen.getByText(/12 experiments/)).toBeInTheDocument();
    // Mounted inside the Optimizer panel.
    expect(
      document
        .querySelector('[data-testid="optimizer-digest-strip"]')!
        .closest('[data-testid="optimizer-panel"]'),
    ).not.toBeNull();
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
});
