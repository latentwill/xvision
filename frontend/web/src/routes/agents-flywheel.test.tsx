import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { AgentsFlywheelRoute } from "./agents-flywheel";
import * as flywheelApi from "@/api/flywheel";

vi.mock("@/api/flywheel", async () => {
  const actual = await vi.importActual<typeof import("@/api/flywheel")>(
    "@/api/flywheel",
  );
  return {
    ...actual,
    getFlywheelStatus: vi.fn(),
    getFlywheelVelocity: vi.fn(),
    getFlywheelLineage: vi.fn(),
    listAutoresearchRuns: vi.fn(),
    runAutoresearch: vi.fn(),
    promoteAutoresearchRun: vi.fn(),
    demoteAutoresearchRun: vi.fn(),
    gateAutoresearchRun: vi.fn(),
    gateOptimization: vi.fn(),
    optimizeMemoryDemos: vi.fn(),
  };
});

function renderRoute(path = "/agents/agent-1/flywheel") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <Routes>
          <Route path="/agents/:id/flywheel" element={<AgentsFlywheelRoute />} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(flywheelApi.getFlywheelStatus).mockResolvedValue({
    namespace: "agent:agent-1",
    observations: 2,
    active_patterns: 1,
    staged_patterns: 0,
    forgotten_patterns: 0,
    autoresearch_runs: 1,
    latest_autoresearch_run_id: "ar-1",
    latest_autoresearch_created_at: "2026-05-21T12:00:00Z",
  });
  vi.mocked(flywheelApi.getFlywheelVelocity).mockResolvedValue({
    namespace: "agent:agent-1",
    days: 7,
    since: "2026-05-18T00:00:00Z",
    observations_captured: 2,
    patterns_promoted: 1,
    patterns_demoted: 0,
    autoresearch_runs: 1,
    optimized_child_agents: 0,
    average_lineage_depth: 0,
    latest_activity_at: "2026-05-21T12:00:00Z",
  });
  vi.mocked(flywheelApi.getFlywheelLineage).mockResolvedValue({
    namespace: "agent:agent-1",
    total: 2,
    items: [
      {
        optimization_id: "opt-2",
        target_agent_id: "agent-1",
        child_agent_id: "agent-child-2",
        slot: "main",
        method: "memory-demos",
        demo_source: "frozen-snapshot",
        reproducible: true,
        holdout_split: "70/15/15",
        cohort_query: "namespace=agent:agent-1,limit=8",
        train_observation_count: 2,
        dev_observation_count: 1,
        holdout_observation_count: 1,
        train_hash: "sha256:train2",
        dev_hash: "sha256:dev2",
        holdout_hash: "sha256:holdout2",
        demo_source_pattern_ids: ["pat-demo"],
        prior_pattern_ids: ["pat-prior"],
        prompt_prefix_chars: 120,
        status: "minted",
        created_at: "2026-05-22T12:00:00Z",
      },
      {
        optimization_id: "opt-1",
        target_agent_id: "agent-1",
        child_agent_id: "agent-child-1",
        slot: "main",
        method: "memory-demos",
        demo_source: "frozen-snapshot",
        reproducible: true,
        holdout_split: "70/15/15",
        cohort_query: "namespace=agent:agent-1,limit=8",
        train_observation_count: 1,
        dev_observation_count: 1,
        holdout_observation_count: 1,
        train_hash: "sha256:train1",
        dev_hash: "sha256:dev1",
        holdout_hash: "sha256:holdout1",
        demo_source_pattern_ids: [],
        prior_pattern_ids: [],
        prompt_prefix_chars: 80,
        status: "minted",
        created_at: "2026-05-21T12:00:00Z",
      },
    ],
  });
  vi.mocked(flywheelApi.listAutoresearchRuns).mockResolvedValue({
    items: [
      {
        id: "ar-1",
        namespace: "agent:agent-1",
        observation_ids: ["obs-1", "obs-2"],
        pattern_id: "pat-1",
        pattern_text: "Reduce risk.",
        promotion_state: "active",
        min_observations: 2,
        created_at: "2026-05-21T12:00:00Z",
        status: "completed",
        gate_verdict: "passed",
      },
    ],
    total: 1,
  });
  vi.mocked(flywheelApi.optimizeMemoryDemos).mockResolvedValue({
    status: "minted",
    namespace: "agent:agent-1",
    target_agent_id: "agent-1",
    child_agent_id: "agent-child",
    slot: "main",
    demo_count: 2,
    observation_ids: ["obs-1", "obs-2"],
    train_observation_ids: ["obs-1"],
    holdout_observation_ids: ["obs-2"],
    demo_source_pattern_ids: [],
    pattern_demo_source_count: 0,
    prior_pattern_ids: [],
    pattern_prior_count: 0,
    observations: [],
    prompt_prefix_chars: 120,
    prompt_preview: "<memory_demos />",
  });
  vi.mocked(flywheelApi.gateOptimization).mockResolvedValue({
    optimization_id: "opt-2",
    dev_metric: "sharpe",
    holdout_metric: "sharpe",
    parent_dev_score: 1,
    child_dev_score: 1.1,
    parent_holdout_score: 0.7,
    child_holdout_score: 0.95,
    gate_epsilon: 0,
    delta_dev: 0.1,
    delta_holdout: 0.25,
    gate_verdict: "passed",
    gate_reason: "route gate",
    gated_at: "2026-05-22T12:10:00Z",
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("AgentsFlywheelRoute", () => {
  it("renders the agent-scoped flywheel panel", async () => {
    renderRoute();

    expect(
      (await screen.findAllByRole("heading", { name: "Flywheel" })).length,
    ).toBeGreaterThan(0);
    await waitFor(() =>
      expect(flywheelApi.getFlywheelStatus).toHaveBeenCalledWith({
        agent: "agent-1",
      }),
    );
    expect(flywheelApi.getFlywheelVelocity).toHaveBeenCalledWith({
      agent: "agent-1",
      days: 7,
    });
    expect(flywheelApi.getFlywheelLineage).toHaveBeenCalledWith({
      agent: "agent-1",
      limit: 20,
    });
    expect(flywheelApi.listAutoresearchRuns).toHaveBeenCalledWith({
      agent: "agent-1",
      limit: 25,
    });
    expect(await screen.findByText("Training run history")).toBeInTheDocument();
    expect(screen.getByText("Autoresearch History")).toBeInTheDocument();
    expect(screen.getAllByText(/opt-2/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/opt-1/).length).toBeGreaterThan(0);
  });

  it("mints memory-demo children for the route agent", async () => {
    const user = userEvent.setup();
    renderRoute();

    await user.click(await screen.findByRole("button", { name: /Train new version/i }));

    await waitFor(() =>
      expect(flywheelApi.optimizeMemoryDemos).toHaveBeenCalledWith(
        expect.objectContaining({
          target_agent_id: "agent-1",
          apply: true,
        }),
      ),
    );
  });
});
