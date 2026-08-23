import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import userEvent from "@testing-library/user-event";

import { AttachedAgentRow, AuthoringRoute } from "./authoring";
import * as strategyApi from "@/api/strategies";
import type { RouteContextField } from "@/api/strategies";
import * as agentApi from "@/api/agents";
import * as settingsApi from "@/api/settings";
import * as chartApi from "@/api/chart";

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return {
    ...actual,
    getStrategy: vi.fn(),
    getStrategyRequirements: vi.fn(),
    patchStrategyMetadata: vi.fn(),
    validateDraft: vi.fn(),
    setRiskConfig: vi.fn(),
    updateSlot: vi.fn(),
    setStrategyPipeline: vi.fn(),
    addStrategyAgent: vi.fn(),
    renameStrategyAgentRole: vi.fn(),
    removeStrategyAgent: vi.fn(),
    cloneStrategy: vi.fn(),
    setStrategyRoute: vi.fn(),
    validateStrategyRoute: vi.fn(),
  };
});

vi.mock("@/api/agents", async () => {
  const actual = await vi.importActual<typeof import("@/api/agents")>(
    "@/api/agents",
  );
  return {
    ...actual,
    listAgents: vi.fn(),
    createAgent: vi.fn(),
  };
});

vi.mock("@/api/chart", () => ({
  strategyChartKeys: {
    strategy: (id: string) => ["strategy-chart", id],
  },
  getStrategyChart: vi.fn().mockResolvedValue({ series: [] }),
}));

vi.mock("@/components/chart/v2/surfaces/StrategyHistoryChartV2", () => ({
  StrategyHistoryChartV2: () => <div data-testid="strategy-chart" />,
}));

vi.mock("@/api/settings", () => ({
  settingsKeys: {
    providers: () => ["settings", "providers"],
    profile: () => ["settings", "profile"],
  },
  listProviders: vi.fn(),
  getProfile: vi.fn().mockResolvedValue({ display_name: null, persisted: false }),
}));

const baseStrategy = {
  manifest: {
    id: "01TEST",
    display_name: "Agent Stack",
    template: "custom",
    creator: "@t",
    plain_summary: "",
    regime_fit: [],
    asset_universe: [],
    decision_cadence_minutes: 240,
    attested_with: [],
    required_tools: [],
    risk_preset_or_config: "balanced",
    published_at: null,
  },
  agents: [{ agent_id: "01DEEPSEEK", role: "trader" }],
  pipeline: { kind: "single" as const },
  regime_slot: null,
  trader_slot: null,
  risk: {
    risk_pct_per_trade: 0.015,
    max_concurrent_positions: 2,
    max_leverage: 3,
    stop_loss_atr_multiple: 2,
    daily_loss_kill_pct: 0.05,
  },
};

const baseAgent = {
  agent_id: "01DEEPSEEK",
  name: "DeepSeek trader",
  description: "",
  tags: [],
  slots: [
    {
      name: "main",
      provider: "openrouter",
      model: "deepseek/deepseek-v4-flash",
      system_prompt: "Trade with discipline.",
      skill_ids: [],
    allowed_tools: [],
      max_tokens: 4096,
    },
  ],
  archived: false,
  created_at: "2026-05-13T14:52:21Z",
  updated_at: "2026-05-13T14:52:21Z",
};

const readyRouteReadiness = {
  routed: true,
  context_fields: [] as RouteContextField[],
  launchable: true,
  reasons: [],
};


function renderRoute() {
  return render(
    <MemoryRouter initialEntries={["/authoring/01TEST"]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <Routes>
          <Route path="/authoring/:id" element={<AuthoringRoute />} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  localStorage.clear();
  vi.mocked(agentApi.listAgents).mockReset();
  vi.mocked(strategyApi.getStrategy).mockReset();
  vi.mocked(strategyApi.patchStrategyMetadata).mockReset();
  vi.mocked(strategyApi.validateDraft).mockReset();
  vi.mocked(strategyApi.removeStrategyAgent).mockReset();
  vi.mocked(strategyApi.renameStrategyAgentRole).mockReset();
  vi.mocked(strategyApi.cloneStrategy).mockReset();
  vi.mocked(strategyApi.setStrategyRoute).mockReset();
  vi.mocked(strategyApi.validateStrategyRoute).mockReset();
  vi.mocked(chartApi.getStrategyChart).mockReset();
  vi.mocked(settingsApi.listProviders).mockResolvedValue({ providers: [] ,
      default_model: null,
  });

  vi.mocked(agentApi.listAgents).mockResolvedValue([baseAgent]);
  vi.mocked(strategyApi.getStrategy).mockResolvedValue(baseStrategy);
  vi.mocked(strategyApi.getStrategyRequirements).mockResolvedValue({
    requirements: [],
    all_models_satisfied: true,
  });
  vi.mocked(chartApi.getStrategyChart).mockResolvedValue({
    strategy_id: "01TEST",
    run_series: [],
    scenarios: [],
  });
  vi.mocked(strategyApi.validateDraft).mockResolvedValue({
    id: "01TEST",
    ok: true,
    errors: [],
  });
  vi.mocked(strategyApi.setStrategyRoute).mockResolvedValue({
    strategy: baseStrategy,
    readiness: readyRouteReadiness,
  });
  vi.mocked(strategyApi.validateStrategyRoute).mockResolvedValue({
    strategy: baseStrategy,
    readiness: readyRouteReadiness,
  });
});

afterEach(() => {
  cleanup();
});

describe("AuthoringRoute — clone strategy", () => {
  it("renders a Clone strategy button between Delete and Run eval", async () => {
    renderRoute();
    const clone = await screen.findByTestId("inspector-clone");
    expect(clone).toHaveTextContent("Clone strategy");

    const del = screen.getByRole("button", { name: /delete strategy/i });
    const runEval = screen.getByRole("link", { name: /run eval/i });

    // DOM order is Delete → Clone → Run eval.
    expect(
      del.compareDocumentPosition(clone) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      clone.compareDocumentPosition(runEval) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("clones via the engine endpoint with a (clone) name", async () => {
    vi.mocked(strategyApi.cloneStrategy).mockResolvedValue({
      ...baseStrategy,
      manifest: {
        ...baseStrategy.manifest,
        id: "01CLONE",
        display_name: "Agent Stack (clone)",
      },
    });

    renderRoute();
    const clone = await screen.findByTestId("inspector-clone");
    fireEvent.click(clone);

    await waitFor(() =>
      expect(vi.mocked(strategyApi.cloneStrategy)).toHaveBeenCalledWith(
        "01TEST",
        { display_name: "Agent Stack (clone)" },
      ),
    );
  });
});

describe("AuthoringRoute attached-agent row collapse + inline detail", () => {
  it("shows quick performance before setup fields using strategy eval chart data", async () => {
    vi.mocked(chartApi.getStrategyChart).mockResolvedValue({
      strategy_id: "01TEST",
      scenarios: [["btc-4h", "BTC 4H"]],
      run_series: [
        {
          run_id: "run-a",
          label: "Run A",
          scenario_id: "btc-4h",
          final_pnl_usd: 1250,
          max_drawdown_pct: -4.2,
          sharpe: 1.84,
          equity_normalised: [
            { time: 1, equity_usd: 100 },
            { time: 2, equity_usd: 112.5 },
          ],
        },
        {
          run_id: "run-b",
          label: "Run B",
          scenario_id: "btc-4h",
          final_pnl_usd: -120,
          max_drawdown_pct: -8.1,
          sharpe: 0.44,
          equity_normalised: [
            { time: 1, equity_usd: 100 },
            { time: 2, equity_usd: 98.8 },
          ],
        },
      ],
    });

    renderRoute();

    const quick = await screen.findByText("Quick performance");
    const manifest = screen.getByText("Manifest");
    expect(
      quick.compareDocumentPosition(manifest) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(await screen.findByText("2 evals")).toBeInTheDocument();
    expect(screen.getByText("+$1,250.00")).toBeInTheDocument();
    expect(screen.getByText("1.84")).toBeInTheDocument();
    expect(screen.getByText("−8.10%")).toBeInTheDocument();
    expect(screen.getByTestId("strategy-chart")).toBeInTheDocument();
  });

  it("places eval readiness after the primary setup sections", async () => {
    renderRoute();

    const agents = await screen.findByText("Strategy agents");
    const risk = screen.getByText("Risk");
    const readiness = screen.getByText("Eval readiness");

    expect(
      agents.compareDocumentPosition(readiness) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      risk.compareDocumentPosition(readiness) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("renders the model in the bar even when the row is collapsed", async () => {
    // Pre-set storage so the row mounts in collapsed state.
    localStorage.setItem("xvn:authoring:agent-collapsed:01TEST:trader", "1");

    renderRoute();

    // Bar shows provider/model regardless of collapse state.
    expect(
      await screen.findByText("openrouter / deepseek/deepseek-v4-flash"),
    ).toBeInTheDocument();

    // Detail body is hidden when collapsed — the agent_id only renders in the
    // expanded body, not in the bar.
    expect(screen.queryByText("01DEEPSEEK")).not.toBeInTheDocument();

    const toggle = screen.getByRole("button", { name: "Expand agent" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
  });

  it("toggles collapse state and persists the choice", async () => {
    renderRoute();

    // Default = expanded (no stored preference).
    const collapseBtn = await screen.findByRole("button", {
      name: "Collapse agent",
    });
    expect(collapseBtn).toHaveAttribute("aria-expanded", "true");
    // Wait for agent pool to settle (bar transitions from agent_id to agent name)
    await screen.findByText("DeepSeek trader");
    expect(screen.getByText("01DEEPSEEK")).toBeInTheDocument();

    fireEvent.click(collapseBtn);

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: "Expand agent" }),
      ).toHaveAttribute("aria-expanded", "false");
    });

    expect(
      localStorage.getItem("xvn:authoring:agent-collapsed:01TEST:trader"),
    ).toBe("1");
    expect(screen.queryByText("01DEEPSEEK")).not.toBeInTheDocument();

    // Toggle back to expanded.
    fireEvent.click(screen.getByRole("button", { name: "Expand agent" }));
    await waitFor(() => {
      expect(screen.getByText("01DEEPSEEK")).toBeInTheDocument();
    });
    expect(
      localStorage.getItem("xvn:authoring:agent-collapsed:01TEST:trader"),
    ).toBe("0");
  });

  it("shows model + system prompt inline in the expanded row (no overlay)", async () => {
    // qa-strategy-popup-to-accordion (2026-05-17): the "Open in window"
    // overlay was removed per the dashboard no-popups rule. Agent detail
    // now lives in the row's existing inline expansion. The expanded
    // state must render the same content the old dialog used to show.
    renderRoute();

    // Wait for the agent pool to load so the row's model/system-prompt
    // detail (sourced from `agentById.get`) populates.
    const modelMatches = await screen.findAllByText(
      "openrouter / deepseek/deepseek-v4-flash",
    );
    expect(modelMatches.length).toBeGreaterThanOrEqual(1);

    // No overlay dialog with the old name should exist.
    expect(
      screen.queryByRole("dialog", { name: "Agent trader details" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Open agent in window" }),
    ).not.toBeInTheDocument();

    // Inline detail renders the agent id and system prompt.
    expect(screen.getByText("01DEEPSEEK")).toBeInTheDocument();
    expect(screen.getByText("Trade with discipline.")).toBeInTheDocument();
  });
});

describe("AttachedAgentRow cross-strategy resync", () => {
  const sharedAgentRef = { agent_id: "01DEEPSEEK", role: "trader" };

  function renderRow(strategyId: string) {
    return render(
      <MemoryRouter>
        <AttachedAgentRow
          strategyId={strategyId}
          agentRef={sharedAgentRef}
          index={1}
          agent={baseAgent}
          onRenameRole={() => {}}
          onRemove={() => {}}
        />
      </MemoryRouter>,
    );
  }

  it("reloads collapse state from storage when strategyId changes", async () => {
    // Strategy A: collapsed. Strategy B: expanded (no storage entry).
    localStorage.setItem("xvn:authoring:agent-collapsed:01STRAT_A:trader", "1");

    const { rerender } = renderRow("01STRAT_A");

    expect(
      screen.getByRole("button", { name: "Expand agent" }),
    ).toHaveAttribute("aria-expanded", "false");
    // Detail body (agent_id text) only renders when expanded — confirm hidden.
    expect(screen.queryByText("01DEEPSEEK")).not.toBeInTheDocument();

    // Same React key (`${agent_id}:${role}`) — component instance reused.
    rerender(
      <MemoryRouter>
        <AttachedAgentRow
          strategyId="01STRAT_B"
          agentRef={sharedAgentRef}
          index={1}
          agent={baseAgent}
          onRenameRole={() => {}}
          onRemove={() => {}}
        />
      </MemoryRouter>,
    );

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: "Collapse agent" }),
      ).toHaveAttribute("aria-expanded", "true");
    });
    expect(screen.getByText("01DEEPSEEK")).toBeInTheDocument();
  });
});

const filterAgent = {
  ...baseAgent,
  agent_id: "01FILTER",
  name: "Regime filter agent",
};

const routerAgent = {
  ...baseAgent,
  agent_id: "01ROUTER",
  name: "Router agent",
};

const trendAgent = {
  ...baseAgent,
  agent_id: "01TREND",
  name: "Trend trader agent",
};

const rangeAgent = {
  ...baseAgent,
  agent_id: "01RANGE",
  name: "Range trader agent",
};
const riskAgent = {
  ...baseAgent,
  agent_id: "01RISK",
  name: "Risk reviewer agent",
};

const routedStrategy = {
  ...baseStrategy,
  agents: [
    { agent_id: "01FILTER", role: "regime_filter", activates: "filter" as const },
    { agent_id: "01ROUTER", role: "router", activates: "router" as const },
    { agent_id: "01TREND", role: "trend_trader", activates: "trader" as const },
    { agent_id: "01RANGE", role: "range_trader", activates: "trader" as const },
  ],
  pipeline: {
    kind: "graph" as const,
    route: {
      router_role: "router",
      branches: [
        { target_role: "trend_trader" },
        { target_role: "range_trader" },
      ],
      graph_edges: [
        {
          from_role: "regime_filter",
          to_role: "trend_trader",
          condition: { eq: { signal_field: "regime", value: "trend" } },
        },
      ],
      context_fields: ["market_snapshot", "tool_state", "available_targets"] as RouteContextField[],
      trace_mode: "compact" as const,
    },
    edges: [],
  },
};

const routedStrategyWithDownstreamReviewer = {
  ...routedStrategy,
  agents: [
    { agent_id: "01FILTER", role: "regime_filter", activates: "filter" as const },
    { agent_id: "01ROUTER", role: "router", activates: "router" as const },
    { agent_id: "01TREND", role: "trend_trader", activates: "trader" as const },
    { agent_id: "01RISK", role: "risk_reviewer", activates: "trader" as const },
    { agent_id: "01RANGE", role: "range_trader", activates: "trader" as const },
  ],
  pipeline: {
    ...routedStrategy.pipeline,
    route: {
      ...routedStrategy.pipeline.route,
      graph_edges: [
        {
          from_role: "router",
          to_role: "trend_trader",
          condition: { eq: { signal_field: "regime", value: "trend" } },
        },
      ],
    },
    edges: [
      {
        from_role: "trend_trader",
        to_role: "risk_reviewer",
      },
    ],
  },
};
const routedStrategyWithoutRoute = {
  ...routedStrategy,
  pipeline: { kind: "graph" as const, route: null, edges: [] },
};
const routeSaveResult = {
  strategy: routedStrategy,
  readiness: {
    routed: true,
    context_fields: ["market_snapshot", "tool_state"] as RouteContextField[],
    launchable: true,
    reasons: [],
  },
};

const routeJsonWithGraphEdgeAndContext = {
  router_role: "router",
  branches: [
    { target_role: "trend_trader" },
    { target_role: "range_trader" },
  ],
  graph_edges: [
    {
      from_role: "regime_filter",
      to_role: "trend_trader",
      condition: { eq: { signal_field: "regime", value: "trend" } },
    },
  ],
  context_fields: ["market_snapshot", "tool_state", "available_targets"] as RouteContextField[],
  trace_mode: "compact" as const,
};

async function chooseRouteOption(
  user: ReturnType<typeof userEvent.setup>,
  triggerName: RegExp,
  optionName: RegExp,
) {
  await user.click(screen.getByRole("button", { name: triggerName }));
  await user.click(await screen.findByRole("option", { name: optionName }));
}

describe("AuthoringRoute — Route Builder", () => {
  beforeEach(() => {
    vi.mocked(agentApi.listAgents).mockResolvedValue([
      filterAgent,
      routerAgent,
      trendAgent,
      riskAgent,
      rangeAgent,
    ]);
  });

  it("renders product-language Route Builder controls and saves router branches through setStrategyRoute", async () => {
    const user = userEvent.setup();
    vi.mocked(strategyApi.getStrategy).mockResolvedValue(routedStrategyWithoutRoute);
    vi.mocked(strategyApi.setStrategyRoute).mockResolvedValue(routeSaveResult);

    renderRoute();

    expect(await screen.findByText(/route builder/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /router/i })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /branch targets/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /add graph route/i }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/pipelinekind/i)).not.toBeInTheDocument();
    expect(
      screen.queryByText(/graph strategies are view-only here/i),
    ).not.toBeInTheDocument();

    await chooseRouteOption(user, /router/i, /router agent|router/i);
    await user.click(screen.getByRole("button", { name: /branch targets/i }));
    await user.click(await screen.findByRole("option", { name: /trend trader/i }));
    await user.click(await screen.findByRole("option", { name: /range trader/i }));
    await user.click(screen.getByRole("button", { name: /add graph route/i }));
    await user.click(screen.getByRole("button", { name: /route source/i }));
    expect(
      await screen.findByRole("option", { name: /router agent|router/i }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("option", { name: /regime_filter|regime filter/i }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("option", { name: /router agent|router/i }));
    await chooseRouteOption(user, /route target/i, /trend_trader|trend trader/i);
    await user.type(
      screen.getByRole("textbox", { name: /condition field/i }),
      "regime",
    );
    await chooseRouteOption(user, /condition operator/i, /equals/i);
    await user.type(
      screen.getByRole("textbox", { name: /condition value/i }),
      "trend",
    );
    await user.click(screen.getByRole("checkbox", { name: /market snapshot/i }));
    await user.click(screen.getByRole("checkbox", { name: /tool state/i }));
    expect(screen.getByText(/router cannot see/i)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /save route/i }));

    await waitFor(() =>
      expect(strategyApi.setStrategyRoute).toHaveBeenCalledWith(
        "01TEST",
        expect.objectContaining({
          router_role: "router",
          branches: expect.arrayContaining([
            expect.objectContaining({ target_role: "trend_trader" }),
            expect.objectContaining({ target_role: "range_trader" }),
          ]),
          graph_edges: expect.arrayContaining([
            expect.objectContaining({
              from_role: "router",
              to_role: "trend_trader",
            }),
          ]),
          context_fields: expect.arrayContaining([
            "market_snapshot",
            "tool_state",
          ]),
        }),
      ),
    );
  });

  it("defaults guided graph route sources to the selected router before save", async () => {
    const user = userEvent.setup();
    vi.mocked(strategyApi.getStrategy).mockResolvedValue(routedStrategyWithoutRoute);
    vi.mocked(strategyApi.setStrategyRoute).mockResolvedValue(routeSaveResult);

    renderRoute();

    expect(await screen.findByText(/route builder/i)).toBeInTheDocument();
    await chooseRouteOption(user, /router/i, /router agent|router/i);
    await user.click(screen.getByRole("button", { name: /branch targets/i }));
    await user.click(await screen.findByRole("option", { name: /trend trader/i }));
    await user.click(screen.getByRole("button", { name: /add graph route/i }));
    await chooseRouteOption(user, /route target/i, /trend_trader|trend trader/i);
    await user.type(
      screen.getByRole("textbox", { name: /condition field/i }),
      "regime",
    );
    await chooseRouteOption(user, /condition operator/i, /equals/i);
    await user.type(
      screen.getByRole("textbox", { name: /condition value/i }),
      "trend",
    );
    await user.click(screen.getByRole("button", { name: /save route/i }));

    await waitFor(() =>
      expect(strategyApi.setStrategyRoute).toHaveBeenCalledWith(
        "01TEST",
        expect.objectContaining({
          graph_edges: [
            expect.objectContaining({
              from_role: "router",
              to_role: "trend_trader",
            }),
          ],
        }),
      ),
    );
    expect(strategyApi.setStrategyRoute).not.toHaveBeenCalledWith(
      "01TEST",
      expect.objectContaining({
        graph_edges: expect.arrayContaining([
          expect.objectContaining({ from_role: "regime_filter" }),
        ]),
      }),
    );
  });

  it.each([
    {
      name: "missing router and branch target",
      strategy: { ...baseStrategy, agents: [], pipeline: { kind: "single" as const } },
      copy:
        "Attach a router and at least one downstream trader target before building a route.",
    },
    {
      name: "one attached agent",
      strategy: {
        ...baseStrategy,
        agents: [{ agent_id: "01ROUTER", role: "router", activates: "router" as const }],
        pipeline: { kind: "sequential" as const },
      },
      copy: "Routing needs a router and at least one branch target.",
    },
    {
      name: "missing router capability",
      strategy: {
        ...baseStrategy,
        agents: [
          { agent_id: "01TREND", role: "trend_trader", activates: "trader" as const },
          { agent_id: "01RANGE", role: "range_trader", activates: "trader" as const },
        ],
        pipeline: { kind: "sequential" as const },
      },
      copy: "Choose an agent that can route, or mark this attached role as a Router.",
    },
    {
      name: "missing downstream trader target",
      strategy: {
        ...baseStrategy,
        agents: [
          { agent_id: "01FILTER", role: "regime_filter", activates: "filter" as const },
          { agent_id: "01ROUTER", role: "router", activates: "router" as const },
        ],
        pipeline: { kind: "graph" as const, route: null, edges: [] },
      },
      copy: "Every route must reach at least one trader target before it can launch.",
    },
    {
      name: "unsupported graph shape",
      strategy: {
        ...routedStrategy,
        pipeline: {
          kind: "graph" as const,
          route: null,
          edges: [{ from_role: "trend_trader", to_role: "regime_filter" }],
        },
      },
      copy:
        "This graph is preserved and exportable. Route Builder can safely edit forward conditioned routes; use JSON import for this unsupported shape.",
    },
  ])("shows route readiness disabled copy for $name", async ({ strategy, copy }) => {
    vi.mocked(strategyApi.getStrategy).mockResolvedValue(strategy);

    renderRoute();

    expect(await screen.findByText(copy)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /save route/i })).toBeDisabled();
  });

  it("surfaces stale-save backend diagnostics without hiding guided controls", async () => {
    const user = userEvent.setup();
    vi.mocked(strategyApi.getStrategy).mockResolvedValue(routedStrategy);
    vi.mocked(strategyApi.setStrategyRoute).mockRejectedValue(
      new Error("ROUTE_STALE: Strategy route changed on the server. Reload and try again."),
    );

    renderRoute();

    expect(await screen.findByText(/route builder/i)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /save route/i }));

    expect(
      await screen.findByText(/strategy route changed on the server/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/reload and try again/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /router/i })).toBeInTheDocument();
  });

  it("validates pasted advanced JSON through route dry-run before saving and keeps guided controls as the default view", async () => {
    const user = userEvent.setup();
    vi.mocked(strategyApi.getStrategy).mockResolvedValue(routedStrategy);
    vi.mocked(strategyApi.validateStrategyRoute).mockResolvedValue({
      strategy: routedStrategy,
      readiness: {
        routed: true,
        context_fields: ["market_snapshot", "tool_state", "available_targets"],
        launchable: true,
        reasons: [],
      },
    });
    vi.mocked(strategyApi.setStrategyRoute).mockResolvedValue({
      strategy: routedStrategy,
      readiness: {
        routed: true,
        context_fields: ["market_snapshot", "tool_state", "available_targets"],
        launchable: true,
        reasons: [],
      },
    });

    renderRoute();

    expect(await screen.findByText(/route builder/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /router/i })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /advanced json/i }));
    expect(screen.getByRole("button", { name: /router/i })).toBeInTheDocument();
    expect((screen.getByLabelText(/route json/i) as HTMLTextAreaElement).value).toContain(
      "\"router_role\"",
    );

    fireEvent.change(screen.getByLabelText(/route json/i), {
      target: { value: JSON.stringify(routeJsonWithGraphEdgeAndContext) },
    });
    await user.click(screen.getByRole("button", { name: /validate json/i }));

    await waitFor(() =>
      expect(strategyApi.validateStrategyRoute).toHaveBeenCalledWith(
        "01TEST",
        expect.objectContaining({ graph_edges: expect.any(Array) }),
      ),
    );
    expect(await screen.findByText(/routed=true/i)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /save json route/i }));

    await waitFor(() =>
      expect(strategyApi.setStrategyRoute).toHaveBeenCalledWith(
        "01TEST",
        expect.objectContaining({ graph_edges: expect.any(Array) }),
      ),
    );
  });

  it("previews actual downstream path semantics after a branch target", async () => {
    vi.mocked(strategyApi.getStrategy).mockResolvedValue(
      routedStrategyWithDownstreamReviewer,
    );

    renderRoute();

    expect(await screen.findByText(/selected path/i)).toBeInTheDocument();
    expect(screen.getByText(/Router:\s*router/i)).toBeInTheDocument();
    expect(
      screen.getByText(/Trend\s*→\s*trend_trader\s*→\s*risk_reviewer/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/Range\s*→\s*range_trader/i)).toBeInTheDocument();
    expect(
      screen.getByText(/router\s*--\s*regime = trend\s*-->\s*trend_trader/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/router can see:\s*market snapshot,\s*tool state,\s*available targets/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/router cannot see:\s*credentials,\s*broker secrets/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/trace:\s*compact/i)).toBeInTheDocument();
    expect(screen.getByText(/actual graph semantics/i)).toBeInTheDocument();
    expect(screen.getByText(/forward test/i)).toBeInTheDocument();
    expect(screen.getByText(/live trading/i)).toBeInTheDocument();
  });
});
