import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import userEvent from "@testing-library/user-event";

import { AuthoringRoute } from "./authoring";
import * as strategyApi from "@/api/strategies";
import * as agentApi from "@/api/agents";
import * as settingsApi from "@/api/settings";
import type * as AgentsApiModule from "@/api/agents";
import type * as StrategiesApiModule from "@/api/strategies";

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof StrategiesApiModule>(
    "@/api/strategies",
  );
  return {
    ...actual,
    getStrategy: vi.fn(),
    getStrategyRequirements: vi.fn(),
    patchStrategyMetadata: vi.fn(),
    validateDraft: vi.fn(),
    deleteStrategy: vi.fn(),
    setRiskConfig: vi.fn(),
    setMechanisticConfig: vi.fn(),
    updateSlot: vi.fn(),
    setStrategyPipeline: vi.fn(),
    addStrategyAgent: vi.fn(),
  };
});

vi.mock("@/api/agents", async () => {
  const actual = await vi.importActual<typeof AgentsApiModule>(
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
  getStrategyChart: vi.fn().mockResolvedValue({
    series: [],
  }),
}));

vi.mock("@/components/chart/v2/surfaces/StrategyHistoryChartV2", () => ({
  StrategyHistoryChartV2: () => <div data-testid="strategy-chart" />,
}));

// ModelPicker uses a custom button-based dropdown; replace with a native <select>
// so tests can use fireEvent.change to set provider::model without simulating
// complex pointer interactions inside the floating menu.
vi.mock("@/components/ModelPicker", () => ({
  ModelPicker: ({
    provider,
    model,
    onChange,
    ariaLabel,
  }: {
    provider: string | null;
    model: string;
    onChange: (provider: string | null, model: string) => void;
    ariaLabel?: string;
  }) => (
    <select
      aria-label={ariaLabel}
      value={provider && model ? `${provider}::${model}` : ""}
      onChange={(e) => {
        const [p, ...rest] = e.target.value.split("::");
        onChange(p || null, rest.join("::"));
      }}
    >
      <option value="">— pick a model —</option>
      {provider && model && (
        <option value={`${provider}::${model}`}>{`${provider}::${model}`}</option>
      )}
      <option value="openrouter::deepseek/deepseek-v4-flash">
        openrouter::deepseek/deepseek-v4-flash
      </option>
    </select>
  ),
  ModelPickerDropdown: ({
    provider,
    model,
    onChange,
    ariaLabel,
  }: {
    provider: string | null;
    model: string;
    onChange: (provider: string | null, model: string) => void;
    ariaLabel?: string;
  }) => (
    <select
      aria-label={ariaLabel}
      value={provider && model ? `${provider}::${model}` : ""}
      onChange={(e) => {
        const [p, ...rest] = e.target.value.split("::");
        onChange(p || null, rest.join("::"));
      }}
    >
      <option value="">— pick a model —</option>
      <option value="openrouter::deepseek/deepseek-v4-flash">
        openrouter::deepseek/deepseek-v4-flash
      </option>
    </select>
  ),
}));

vi.mock("@/api/settings", () => ({
  settingsKeys: {
    providers: () => ["settings", "providers"],
    profile: () => ["settings", "profile"],
  },
  listProviders: vi.fn(),
  getProfile: vi.fn().mockResolvedValue({ display_name: null, persisted: false }),
}));

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
  vi.mocked(agentApi.listAgents).mockReset();
  vi.mocked(strategyApi.getStrategy).mockReset();
  vi.mocked(strategyApi.patchStrategyMetadata).mockReset();
  vi.mocked(strategyApi.validateDraft).mockReset();
  vi.mocked(strategyApi.deleteStrategy).mockReset();
  vi.mocked(strategyApi.setRiskConfig).mockReset();
  vi.mocked(strategyApi.setMechanisticConfig).mockReset();
  vi.mocked(strategyApi.setStrategyPipeline).mockReset();
  vi.mocked(strategyApi.addStrategyAgent).mockReset();
  vi.mocked(agentApi.createAgent).mockReset();
  vi.mocked(settingsApi.listProviders).mockResolvedValue({ providers: [] ,
      default_model: null,
  });
  vi.mocked(strategyApi.deleteStrategy).mockResolvedValue(undefined);
  vi.mocked(strategyApi.getStrategyRequirements).mockResolvedValue({
    requirements: [],
    all_models_satisfied: true,
  });
});

afterEach(() => {
  cleanup();
});

describe("AuthoringRoute risk editor", () => {
  it("renders editable manifest fields and hides mechanical params", async () => {
    vi.mocked(agentApi.listAgents).mockResolvedValue([]);
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
      manifest: {
        id: "01TEST",
        display_name: "Trend 4H",
        template: "trend_follower",
        creator: "@t",
        plain_summary: "",
        regime_fit: [],
        asset_universe: ["BTC/USD"],
        decision_cadence_minutes: 240,
        attested_with: [],
        required_tools: [],
        risk_preset_or_config: "balanced",
        published_at: null,
      },
      regime_slot: null,
      trader_slot: null,
      risk: {
        risk_pct_per_trade: 0.015,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });

    renderRoute();

    expect(await screen.findByLabelText("Display name")).toHaveValue("Trend 4H");
    // Assets field is a chip editor (not an <input>); verify the chip is rendered
    expect(screen.getByText("BTC/USD")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Time frame" })).toHaveTextContent("4h");
    expect(screen.getByLabelText(/Strategy ID 01TEST/)).toHaveValue("01TEST");
    expect(screen.getByText("No saved filter")).toBeInTheDocument();
    expect(screen.queryByText("Mechanical params")).not.toBeInTheDocument();
  });

  it("edits Agentic/Mechanistic decision mode inside the Filter module", async () => {
    vi.mocked(agentApi.listAgents).mockResolvedValue([]);
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
      manifest: {
        id: "01TEST",
        display_name: "Trend 4H",
        template: "trend_follower",
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
      decision_mode: "agentic",
      regime_slot: null,
      trader_slot: null,
      risk: {
        risk_pct_per_trade: 0.015,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });
    vi.mocked(strategyApi.setMechanisticConfig).mockResolvedValue({
      manifest: {
        id: "01TEST",
        display_name: "Trend 4H",
        template: "trend_follower",
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
      decision_mode: "mechanistic",
      mechanistic_config: { entry_rules: [], close_policies: [] },
      regime_slot: null,
      trader_slot: null,
      risk: {
        risk_pct_per_trade: 0.015,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });

    renderRoute();

    const filterCard = await screen.findByTestId("strategy-filter-card");
    expect(
      screen.queryByText(/Who makes trade decisions/i),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Agent-direct")).not.toBeInTheDocument();
    const decisionModeGroups = await screen.findAllByRole("group", {
      name: /decision mode/i,
    });
    expect(decisionModeGroups).toHaveLength(1);
    expect(filterCard).toContainElement(decisionModeGroups[0]);

    fireEvent.click(
      within(filterCard).getByRole("button", { name: /mechanistic/i }),
    );
    fireEvent.click(within(filterCard).getByRole("button", { name: /save mode/i }));

    await waitFor(() => {
      expect(strategyApi.setMechanisticConfig).toHaveBeenCalledWith("01TEST", {
        decision_mode: "mechanistic",
        mechanistic_config: { entry_rules: [], close_policies: [] },
      });
    });
  });

  it("does not render the old validation box in the Inspector rail", async () => {
    vi.mocked(agentApi.listAgents).mockResolvedValue([]);
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
      manifest: {
        id: "01TEST",
        display_name: "Trend 4H",
        template: "trend_follower",
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
      regime_slot: null,
      trader_slot: null,
      risk: {
        risk_pct_per_trade: 0.015,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });
    vi.mocked(strategyApi.validateDraft).mockResolvedValue({
      id: "01TEST",
      ok: false,
      errors: ["single-agent pipeline cannot include multiple agents"],
    });

    renderRoute();

    expect(await screen.findByText("Risk per trade (%)")).toBeInTheDocument();
    expect(screen.queryByText("Validation")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check eval readiness" })).toBeInTheDocument();
    expect(screen.queryByText("single-agent pipeline cannot include multiple agents")).not.toBeInTheDocument();
  });

  it("deletes a strategy from the inspector action bar", async () => {
    vi.mocked(agentApi.listAgents).mockResolvedValue([]);
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
      manifest: {
        id: "01TEST",
        display_name: "Trend 4H",
        template: "trend_follower",
        creator: "@t",
        plain_summary: "",
        regime_fit: [],
        asset_universe: ["BTC/USD"],
        decision_cadence_minutes: 240,
        attested_with: [],
        required_tools: [],
        risk_preset_or_config: "balanced",
        published_at: null,
      },
      regime_slot: null,
      trader_slot: null,
      risk: {
        risk_pct_per_trade: 0.015,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);

    renderRoute();

    fireEvent.click(await screen.findByRole("button", { name: /Delete strategy 01TEST/ }));

    await waitFor(() => {
      expect(strategyApi.deleteStrategy).toHaveBeenCalledWith("01TEST");
    });
    confirm.mockRestore();
  });

  it("edits explicit risk fields and saves them", async () => {
    vi.mocked(agentApi.listAgents).mockResolvedValue([]);
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
      manifest: {
        id: "01TEST",
        display_name: "Trend 4H",
        template: "trend_follower",
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
      regime_slot: null,
      trader_slot: null,
      risk: {
        risk_pct_per_trade: 0.015,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });
    vi.mocked(strategyApi.validateDraft).mockResolvedValue({
      id: "01TEST",
      ok: true,
      errors: [],
    });
    vi.mocked(strategyApi.setRiskConfig).mockResolvedValue({
      id: "01TEST",
      applied: "explicit",
    });

    renderRoute();

    const input = (await screen.findByLabelText(
      "Risk per trade (%)",
    )) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "2.50" } });
    fireEvent.click(screen.getByRole("button", { name: "Save risk" }));

    await waitFor(() => {
      expect(strategyApi.setRiskConfig).toHaveBeenCalledWith("01TEST", {
        explicit: {
          risk_pct_per_trade: 0.025,
          max_concurrent_positions: 2,
          max_leverage: 3,
          stop_loss_atr_multiple: 2,
          daily_loss_kill_pct: 0.05,
        },
      });
    });
  });
});

describe("AuthoringRoute agent composition", () => {
  it("surfaces missing agent setup before opening the eval launcher", async () => {
    vi.mocked(agentApi.listAgents).mockResolvedValue([]);
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
      manifest: {
        id: "01TEST",
        display_name: "Agentless Draft",
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
      agents: [],
      pipeline: { kind: "single" },
      regime_slot: null,
      trader_slot: null,
      risk: {
        risk_pct_per_trade: 0.015,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });
    vi.mocked(strategyApi.validateDraft).mockResolvedValue({
      id: "01TEST",
      ok: true,
      errors: [],
    });

    renderRoute();

    // RunEvalCard was removed by qa-ui-micro-fixes (2026-05-17); the
    // "no agents attached" path now surfaces solely through
    // InspectorActions at the top of the route.
    expect(
      await screen.findByText(/no strategy agent is attached yet/i),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: /go to agents/i }),
    ).toHaveAttribute("href", "#strategy-agents");
    // The launch button must be absent until an agent is attached.
    expect(
      screen.queryByRole("link", { name: /^run eval/i }),
    ).not.toBeInTheDocument();
  });

  it("attaches an existing agent with a role derived from the agent name", async () => {
    const user = userEvent.setup();
    vi.mocked(agentApi.listAgents).mockResolvedValue([
      {
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
      },
    ]);
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
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
      agents: [],
      pipeline: { kind: "single" },
      regime_slot: null,
      trader_slot: null,
      risk: {
        risk_pct_per_trade: 0.015,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });
    vi.mocked(strategyApi.addStrategyAgent).mockResolvedValue({
      strategy_id: "01TEST",
      agents: [{ agent_id: "01DEEPSEEK", role: "deepseek-trader" }],
      pipeline: { kind: "single" },
    });

    renderRoute();

    const picker = await screen.findByRole("button", { name: /existing agent/i });
    await user.click(picker);
    await user.type(
      screen.getByRole("textbox", { name: /search existing agent/i }),
      "01DEEPSEEK",
    );
    await user.click(await screen.findByRole("option", { name: /DeepSeek trader/i }));
    const addButton = screen.getByRole("button", { name: "Add Agent" });
    await waitFor(() => expect(addButton).not.toBeDisabled());
    await user.click(addButton);

    await waitFor(() => {
      expect(strategyApi.addStrategyAgent).toHaveBeenCalledWith("01TEST", {
        agent_id: "01DEEPSEEK",
        role: "deepseek-trader",
      });
    });
  });

  it("shows attached agent name and provider/model when available", async () => {
    vi.mocked(agentApi.listAgents).mockResolvedValue([
      {
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
      },
    ]);
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
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
      pipeline: { kind: "single" },
      regime_slot: null,
      trader_slot: null,
      risk: {
        risk_pct_per_trade: 0.015,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });

    renderRoute();

    expect(await screen.findByText("DeepSeek trader")).toBeInTheDocument();
    // After qa-strategy-popup-to-accordion (2026-05-17), the model
    // label renders in both the bar and the inline detail panel
    // (replacing the removed overlay dialog). Use getAllByText since
    // both surfaces show the same string.
    expect(
      screen.getAllByText("openrouter / deepseek/deepseek-v4-flash").length,
    ).toBeGreaterThanOrEqual(1);
  });

  it("shows AgentRefs in pipeline order with current pipeline kind", async () => {
    vi.mocked(agentApi.listAgents).mockResolvedValue([]);
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
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
      agents: [
        { agent_id: "01INTERN", role: "analyst" },
        { agent_id: "01TRADER", role: "trader" },
      ],
      pipeline: { kind: "sequential" },
      regime_slot: null,
      trader_slot: null,
      risk: {
        risk_pct_per_trade: 0.015,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });
    vi.mocked(strategyApi.validateDraft).mockResolvedValue({
      id: "01TEST",
      ok: true,
      errors: [],
    });

    renderRoute();

    expect(await screen.findByText("Pipeline kind")).toBeInTheDocument();
    expect(screen.getAllByText("sequential").length).toBeGreaterThan(0);
    // When agents pool is empty, bar falls back to agent_id; detail also shows it
    expect(screen.getAllByText("01INTERN").length).toBeGreaterThan(0);
    expect(screen.getAllByText("01TRADER").length).toBeGreaterThan(0);
    expect(screen.getAllByText("1").length).toBeGreaterThan(0);
    expect(screen.getAllByText("2").length).toBeGreaterThan(0);
  });

  it("sets the pipeline kind through the strategy pipeline API", async () => {
    vi.mocked(agentApi.listAgents).mockResolvedValue([]);
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
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
      agents: [
        { agent_id: "01INTERN", role: "analyst" },
        { agent_id: "01TRADER", role: "trader" },
      ],
      pipeline: { kind: "single" },
      regime_slot: null,
      trader_slot: null,
      risk: {
        risk_pct_per_trade: 0.015,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });
    vi.mocked(strategyApi.validateDraft).mockResolvedValue({
      id: "01TEST",
      ok: false,
      errors: ["single-agent pipeline cannot include multiple agents"],
    });
    vi.mocked(strategyApi.setStrategyPipeline).mockResolvedValue({
      strategy_id: "01TEST",
      agents: [
        { agent_id: "01INTERN", role: "analyst" },
        { agent_id: "01TRADER", role: "trader" },
      ],
      pipeline: { kind: "sequential" },
    });

    renderRoute();

    const user = userEvent.setup();
    const pipelineSelect = await screen.findByRole("button", {
      name: /pipeline kind/i,
    });
    await user.click(pipelineSelect);
    const disabledSingle = await screen.findByRole("option", {
      name: /filter-gated agent/i,
    });
    expect(disabledSingle).toBeDisabled();
    expect(screen.queryByRole("option", { name: /^single$/i })).toBeNull();
    expect(
      screen.getByText(/first AgentRef is the gated trader/i),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("option", { name: /sequential/i }));

    await waitFor(() => {
      expect(strategyApi.setStrategyPipeline).toHaveBeenCalledWith("01TEST", {
        kind: "sequential",
        edges: [],
      });
    });
  });

  it("creates and attaches an agent with the configured provider/model picker", async () => {
    vi.mocked(agentApi.listAgents).mockResolvedValue([]);
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        {
          name: "openrouter",
          kind: "openai-compat",
          base_url: "https://openrouter.ai/api/v1",
          api_key_env: "OPENROUTER_API_KEY",
          api_key_set: true,
          synthetic: false,
          is_default: false,
          enabled_models: ["deepseek/deepseek-v4-flash"],
        },
      ],
    
        default_model: null,
    });
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
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
      agents: [],
      pipeline: { kind: "single" },
      regime_slot: null,
      trader_slot: null,
      risk: {
        risk_pct_per_trade: 0.015,
        max_concurrent_positions: 2,
        max_leverage: 3,
        stop_loss_atr_multiple: 2,
        daily_loss_kill_pct: 0.05,
      },
    });
    vi.mocked(strategyApi.validateDraft).mockResolvedValue({
      id: "01TEST",
      ok: true,
      errors: [],
    });
    vi.mocked(agentApi.createAgent).mockResolvedValue({
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
          max_tokens: null,
        },
      ],
      archived: false,
      created_at: "2026-05-13T14:52:21Z",
      updated_at: "2026-05-13T14:52:21Z",
    });
    vi.mocked(strategyApi.addStrategyAgent).mockResolvedValue({
      strategy_id: "01TEST",
      agents: [{ agent_id: "01DEEPSEEK", role: "trader" }],
      pipeline: { kind: "single" },
    });

    renderRoute();

    // qa-strategy-popup-to-accordion (2026-05-17): the "Create and
    // attach" form is now a mode inside the AddAgentAccordion; switch
    // to it before filling fields.
    fireEvent.click(
      await screen.findByRole("button", { name: "Create new" }),
    );

    // Role field removed — just fill name, model, prompt
    fireEvent.change(await screen.findByLabelText("New agent name"), {
      target: { value: "DeepSeek trader" },
    });
    fireEvent.change(screen.getByLabelText("New agent model"), {
      target: { value: "openrouter::deepseek/deepseek-v4-flash" },
    });
    fireEvent.change(screen.getByLabelText("New agent system prompt"), {
      target: { value: "Trade with discipline." },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Create and attach agent" }),
    );

    await waitFor(() => {
      expect(agentApi.createAgent).toHaveBeenCalledWith({
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
            max_tokens: null,
          },
        ],
      });
    });
    // Role is auto-derived from agent name: nameToRole("DeepSeek trader") = "deepseek-trader"
    await waitFor(() => {
      expect(strategyApi.addStrategyAgent).toHaveBeenCalledWith("01TEST", {
        agent_id: "01DEEPSEEK",
        role: "deepseek-trader",
      });
    });
  });
});
