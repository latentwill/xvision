// MemoryTab — vitest coverage for the per-agent Memory tab UI surface.
//
// Mock layer: we stub `@/api/memory` directly (same pattern as
// `agents.test.tsx` uses for `@/api/agents`). No msw — the codebase
// has no msw dependency and the API helpers are thin enough that
// module-level mocks give us the same coverage with less setup.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { MemoryTab } from "./MemoryTab";
import * as flywheelApi from "@/api/flywheel";
import * as memoryApi from "@/api/memory";

vi.mock("@/api/memory", async () => {
  const actual = await vi.importActual<typeof import("@/api/memory")>(
    "@/api/memory",
  );
  return {
    ...actual,
    listMemory: vi.fn(),
    createPattern: vi.fn(),
    createOperatorAttestation: vi.fn(),
    activatePattern: vi.fn(),
    demotePattern: vi.fn(),
    deleteMemoryItem: vi.fn(),
    forgetMemory: vi.fn(),
  };
});

vi.mock("@/api/flywheel", async () => {
  const actual = await vi.importActual<typeof import("@/api/flywheel")>(
    "@/api/flywheel",
  );
  return {
    ...actual,
    getFlywheelStatus: vi.fn(),
    getFlywheelVelocity: vi.fn(),
    getFlywheelLineage: vi.fn(),
    listAutoOptimizerRuns: vi.fn(),
    runAutoOptimizer: vi.fn(),
    getAutoOptimizerRun: vi.fn(),
    gateAutoOptimizerRun: vi.fn(),
    gateOptimization: vi.fn(),
    promoteAutoOptimizerRun: vi.fn(),
    demoteAutoOptimizerRun: vi.fn(),
    optimizeMemoryDemos: vi.fn(),
  };
});

function emptyList(): memoryApi.MemoryListResponse {
  return { items: [], total: 0 };
}

function pattern(id: string, text: string, namespace: string): memoryApi.MemoryItem {
  return {
    id,
    namespace,
    tier: "pattern",
    text,
    created_at: "2026-05-21T12:00:00Z",
    run_id: null,
    scenario_id: null,
    cycle_idx: null,
    training_window_end: null,
  };
}

function observation(
  id: string,
  text: string,
  namespace: string,
  extra: Partial<memoryApi.MemoryItem> = {},
): memoryApi.MemoryItem {
  return {
    id,
    namespace,
    tier: "observation",
    text,
    created_at: "2026-05-21T12:00:00Z",
    run_id: extra.run_id ?? null,
    scenario_id: extra.scenario_id ?? null,
    cycle_idx: extra.cycle_idx ?? null,
    training_window_end: null,
  };
}

function renderTab(agentId = "agent-1") {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <MemoryTab agentId={agentId} />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.mocked(flywheelApi.getFlywheelStatus).mockResolvedValue({
    namespace: "agent:agent-1",
    observations: 3,
    active_patterns: 1,
    staged_patterns: 1,
    forgotten_patterns: 0,
    autooptimizer_runs: 2,
    latest_autooptimizer_run_id: "ar-1",
    latest_autooptimizer_created_at: "2026-05-21T12:00:00Z",
  });
  vi.mocked(flywheelApi.getFlywheelVelocity).mockResolvedValue({
    namespace: "agent:agent-1",
    days: 7,
    since: "2026-05-18T00:00:00Z",
    observations_captured: 3,
    patterns_promoted: 1,
    patterns_demoted: 0,
    autooptimizer_runs: 2,
    optimized_child_agents: 1,
    average_lineage_depth: 1,
    latest_activity_at: "2026-05-21T12:00:00Z",
  });
  vi.mocked(flywheelApi.getFlywheelLineage).mockResolvedValue({
    namespace: "agent:agent-1",
    total: 1,
    items: [
      {
        optimization_id: "opt-agent",
        target_agent_id: "agent-1",
        child_agent_id: "agent-child",
        slot: "main",
        method: "memory-demos",
        demo_source: "frozen-snapshot",
        reproducible: true,
        holdout_split: "70/15/15",
        cohort_query: "namespace=agent:agent-1,limit=8",
        train_observation_count: 1,
        dev_observation_count: 1,
        holdout_observation_count: 1,
        train_hash: "sha256:train",
        dev_hash: "sha256:dev",
        holdout_hash: "sha256:holdout",
        demo_source_pattern_ids: ["pat-demo"],
        prior_pattern_ids: ["pat-prior"],
        prompt_prefix_chars: 120,
        status: "minted",
        created_at: "2026-05-21T12:00:00Z",
        gate_verdict: "passed",
        delta_dev: 0.2,
        delta_holdout: 0.3,
        gate_reason: "child beat parent",
      },
    ],
  });
  vi.mocked(flywheelApi.runAutoOptimizer).mockResolvedValue({
    id: "ar-new",
    namespace: "agent:agent-1",
    observation_ids: ["obs-1", "obs-2"],
    pattern_id: "pat-staged",
    pattern_text: "Reduce risk.",
    promotion_state: "staged",
    min_observations: 2,
    created_at: "2026-05-21T12:00:00Z",
    status: "completed",
  });
  vi.mocked(flywheelApi.listAutoOptimizerRuns).mockResolvedValue({
    items: [
      {
        id: "ar-1",
        namespace: "agent:agent-1",
        observation_ids: ["obs-1", "obs-2"],
        pattern_id: "pat-staged",
        pattern_text: "Reduce risk.",
        promotion_state: "staged",
        gate_passed: true,
        finding_blind: true,
        min_observations: 2,
        created_at: "2026-05-21T12:00:00Z",
        status: "completed",
      },
    ],
    total: 1,
  });
  vi.mocked(flywheelApi.promoteAutoOptimizerRun).mockResolvedValue({
    id: "ar-1",
    namespace: "agent:agent-1",
    observation_ids: ["obs-1", "obs-2"],
    pattern_id: "pat-staged",
    pattern_text: "Reduce risk.",
    promotion_state: "active",
    min_observations: 2,
    created_at: "2026-05-21T12:00:00Z",
    status: "completed",
  });
  vi.mocked(flywheelApi.gateAutoOptimizerRun).mockResolvedValue({
    id: "ar-1",
    namespace: "agent:agent-1",
    observation_ids: ["obs-1", "obs-2"],
    pattern_id: "pat-staged",
    pattern_text: "Reduce risk.",
    promotion_state: "staged",
    min_observations: 2,
    created_at: "2026-05-21T12:00:00Z",
    status: "completed",
    gate_verdict: "passed",
    gate_reason: "day and holdout improved",
    parent_day_score: 1,
    child_day_score: 1.25,
    parent_holdout_score: 0.8,
    child_holdout_score: 0.95,
    gate_epsilon: 0.01,
    delta_day: 0.25,
    delta_holdout: 0.15,
  });
  vi.mocked(flywheelApi.gateOptimization).mockResolvedValue({
    optimization_id: "opt-agent",
    dev_metric: "sharpe",
    holdout_metric: "sharpe",
    parent_dev_score: 1,
    child_dev_score: 1.2,
    parent_holdout_score: 0.8,
    child_holdout_score: 1.1,
    gate_epsilon: 0.01,
    delta_dev: 0.2,
    delta_holdout: 0.3,
    gate_verdict: "passed",
    gate_reason: "child beat parent",
    gated_at: "2026-05-21T12:10:00Z",
  });
  vi.mocked(flywheelApi.demoteAutoOptimizerRun).mockResolvedValue({
    id: "ar-1",
    namespace: "agent:agent-1",
    observation_ids: ["obs-1", "obs-2"],
    pattern_id: "pat-staged",
    pattern_text: "Reduce risk.",
    promotion_state: "demoted",
    min_observations: 2,
    created_at: "2026-05-21T12:00:00Z",
    status: "completed",
  });
  vi.mocked(flywheelApi.optimizeMemoryDemos).mockResolvedValue({
    status: "minted",
    namespace: "agent:agent-1",
    target_agent_id: "agent-1",
    child_agent_id: "agent-child",
    slot: "main",
    demo_count: 2,
    observation_ids: ["obs-1", "obs-2"],
    train_observation_ids: ["obs-1", "obs-2"],
    holdout_observation_ids: ["obs-3"],
    demo_source_pattern_ids: ["pat-demo"],
    pattern_demo_source_count: 1,
    prior_pattern_ids: ["pat-prior"],
    pattern_prior_count: 1,
    observations: [],
    prompt_prefix_chars: 120,
    prompt_preview: "<memory_demos />",
  });
  vi.mocked(memoryApi.listMemory).mockResolvedValue(emptyList());
  vi.mocked(memoryApi.createPattern).mockResolvedValue(
    pattern("pat-new", "fresh wisdom", "agent:agent-1"),
  );
  vi.mocked(memoryApi.createOperatorAttestation).mockResolvedValue({
    id: "attest-1",
    operator_initials: "QA",
    surface: "dashboard",
    warning_text_hash: "sha256:test",
    created_at: "2026-05-21T12:00:00Z",
    signature: null,
  });
  vi.mocked(memoryApi.activatePattern).mockResolvedValue(
    pattern("pat-staged", "staged wisdom", "agent:agent-1"),
  );
  vi.mocked(memoryApi.demotePattern).mockResolvedValue(
    pattern("pat-staged", "staged wisdom", "agent:agent-1"),
  );
  vi.mocked(memoryApi.deleteMemoryItem).mockResolvedValue();
  vi.mocked(memoryApi.forgetMemory).mockResolvedValue({ deleted: 0 });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("MemoryTab — empty state", () => {
  it("renders without crashing when the agent has no memory", async () => {
    renderTab();
    // Patterns is the default sub-tab. Wait for the empty-state copy.
    expect(
      await screen.findByText(/No patterns yet/i),
    ).toBeInTheDocument();
    // "+ Add Pattern" button is always present on the Patterns sub-tab.
    expect(
      screen.getByRole("button", { name: /Add Pattern/i }),
    ).toBeInTheDocument();
  });

  it("queries flywheel status for the agent namespace", async () => {
    renderTab();

    expect(await screen.findByText("Flywheel")).toBeInTheDocument();
    await waitFor(() => {
      expect(flywheelApi.getFlywheelStatus).toHaveBeenCalledWith({
        agent: "agent-1",
      });
      expect(flywheelApi.getFlywheelVelocity).toHaveBeenCalledWith({
        agent: "agent-1",
        days: 7,
      });
      expect(flywheelApi.getFlywheelLineage).toHaveBeenCalledWith({
        agent: "agent-1",
        limit: 1,
      });
      expect(screen.getByText("New versions / 7d")).toBeInTheDocument();
      expect(screen.getByText("Latest Lineage")).toBeInTheDocument();
    });
  });
});

describe("MemoryTab — Flywheel panel", () => {
  it("mints a memory-demo child agent", async () => {
    const user = userEvent.setup();
    renderTab();

    await user.type(
      await screen.findByLabelText(/Child Agent Name/i),
      "Agent child",
    );
    await user.click(screen.getByLabelText(/Include patterns I've already learned/i));
    await user.click(screen.getByRole("button", { name: /Train new version/i }));

    await waitFor(() => {
      expect(flywheelApi.optimizeMemoryDemos).toHaveBeenCalledWith({
        target_agent_id: "agent-1",
        demo_source: "frozen-snapshot",
        holdout_split: "70/15/15",
        auto_prior_patterns: true,
        prior_pattern_limit: 5,
        apply: true,
        child_name: "Agent child",
      });
    });
    expect(await screen.findByText("Example patterns")).toBeInTheDocument();
    expect(screen.getByText("Background patterns")).toBeInTheDocument();
    expect(screen.getAllByText(/Decision: Kept/).length).toBeGreaterThan(0);
    expect(screen.getByText(/untouched test 0.300/)).toBeInTheDocument();
  });

  it("records the day and holdout gate for a staged autooptimizer run", async () => {
    const user = userEvent.setup();
    renderTab();

    await user.type(await screen.findByLabelText(/Baseline today's score ar-1/i), "1");
    await user.type(screen.getByLabelText(/Candidate today's score ar-1/i), "1.25");
    await user.type(screen.getByLabelText(/Baseline untouched-period score ar-1/i), "0.8");
    await user.type(screen.getByLabelText(/Candidate untouched-period score ar-1/i), "0.95");
    await user.clear(screen.getByLabelText(/Min improvement ar-1/i));
    await user.type(screen.getByLabelText(/Min improvement ar-1/i), "0.01");
    await user.type(
      screen.getByLabelText(/Gate reason ar-1/i),
      "day and holdout improved",
    );
    await user.click(
      screen.getByRole("button", { name: /Record gate decision/i }),
    );

    await waitFor(() => {
      expect(flywheelApi.gateAutoOptimizerRun).toHaveBeenCalledWith("ar-1", {
        parent_day_score: 1,
        child_day_score: 1.25,
        parent_holdout_score: 0.8,
        child_holdout_score: 0.95,
        gate_epsilon: 0.01,
        gate_reason: "day and holdout improved",
      });
    });
  });

  it("records the dev and holdout gate for an optimization lineage row", async () => {
    vi.mocked(flywheelApi.getFlywheelLineage).mockResolvedValue({
      namespace: "agent:agent-1",
      total: 1,
      items: [
        {
          optimization_id: "opt-agent",
          target_agent_id: "agent-1",
          child_agent_id: "agent-child",
          slot: "main",
          method: "memory-demos",
          demo_source: "frozen-snapshot",
          reproducible: true,
          holdout_split: "70/15/15",
          cohort_query: "namespace=agent:agent-1,limit=8",
          train_observation_count: 1,
          dev_observation_count: 1,
          holdout_observation_count: 1,
          train_hash: "sha256:train",
          dev_hash: "sha256:dev",
          holdout_hash: "sha256:holdout",
          demo_source_pattern_ids: ["pat-demo"],
          prior_pattern_ids: ["pat-prior"],
          prompt_prefix_chars: 120,
          status: "minted",
          created_at: "2026-05-21T12:00:00Z",
        },
      ],
    });
    const user = userEvent.setup();
    renderTab();

    await user.type(
      await screen.findByLabelText(/Baseline validation score opt-agent/i),
      "1",
    );
    await user.type(screen.getByLabelText(/Candidate validation score opt-agent/i), "1.2");
    await user.type(
      screen.getByLabelText(/Baseline untouched-period score opt-agent/i),
      "0.8",
    );
    await user.type(
      screen.getByLabelText(/Candidate untouched-period score opt-agent/i),
      "1.1",
    );
    await user.clear(
      screen.getByLabelText(/Min improvement opt-agent/i),
    );
    await user.type(
      screen.getByLabelText(/Min improvement opt-agent/i),
      "0.01",
    );
    await user.type(
      screen.getByLabelText(/Optimization gate reason opt-agent/i),
      "child beat parent",
    );
    await user.click(
      screen.getByRole("button", {
        name: /Record gate decision for opt-agent/i,
      }),
    );

    await waitFor(() => {
      expect(flywheelApi.gateOptimization).toHaveBeenCalledWith("opt-agent", {
        parent_dev_score: 1,
        child_dev_score: 1.2,
        parent_holdout_score: 0.8,
        child_holdout_score: 1.1,
        gate_epsilon: 0.01,
        gate_reason: "child beat parent",
      });
    });
  });
});


describe("MemoryTab — Observations sub-tab", () => {
  it("shows a read-only observation list with no per-item delete", async () => {
    const user = userEvent.setup();
    vi.mocked(memoryApi.listMemory).mockImplementation(async (q) => {
      if (q?.tier === "observation") {
        return {
          items: [
            observation("obs-1", "regime broke at 09:32", "agent:agent-1", {
              run_id: "01HZRUN1",
              scenario_id: "btc-bull-q1",
              cycle_idx: 7,
            }),
          ],
          total: 1,
        };
      }
      return emptyList();
    });

    renderTab();

    await screen.findByText(/No patterns yet/i);
    await user.click(
      screen.getByRole("tab", { name: /Observations/i }),
    );

    expect(
      await screen.findByText(/regime broke at 09:32/),
    ).toBeInTheDocument();
    // No per-row delete button anywhere in the Observations panel.
    const panel = screen.getByRole("tabpanel", { name: /Observations/i });
    expect(
      within(panel).queryByRole("button", { name: /^Delete$/i }),
    ).not.toBeInTheDocument();
    expect(
      within(panel).queryByRole("button", { name: /^Remove$/i }),
    ).not.toBeInTheDocument();
  });

  it("filters observations by scenario_id and run_id", async () => {
    const user = userEvent.setup();
    vi.mocked(memoryApi.listMemory).mockResolvedValue(emptyList());
    renderTab();

    await screen.findByText(/No patterns yet/i);
    await user.click(screen.getByRole("tab", { name: /Observations/i }));

    const scenarioInput = await screen.findByLabelText(/Scenario id/i);
    await user.type(scenarioInput, "btc-bull-q1");
    const runInput = screen.getByLabelText(/Run id/i);
    await user.type(runInput, "01HZRUN1");

    await waitFor(() => {
      const calls = vi.mocked(memoryApi.listMemory).mock.calls;
      const obsCall = calls.find(
        (c) =>
          c[0]?.tier === "observation" &&
          c[0]?.scenario_id === "btc-bull-q1" &&
          c[0]?.run_id === "01HZRUN1",
      );
      expect(obsCall).toBeTruthy();
    });
  });
});

