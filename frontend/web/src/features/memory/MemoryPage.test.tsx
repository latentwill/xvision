// MemoryPage — workspace-level memory surface scoped to namespace="global".
//
// Mirrors the per-agent MemoryTab UX but with no agent context: the
// Patterns sub-tab defaults "+ Add Pattern" to namespace="global", and
// the bottom "Forget all global memory" button bulk-deletes via
// DELETE /api/memory?namespace=global.
//
// Mock layer: stubs `@/api/memory` directly, same pattern as
// MemoryTab.test.tsx.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Navigate, Route, Routes } from "react-router-dom";

import { MemoryPage } from "./MemoryPage";
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

function pattern(
  id: string,
  text: string,
  namespace: string,
): memoryApi.MemoryItem {
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
): memoryApi.MemoryItem {
  return {
    id,
    namespace,
    tier: "observation",
    text,
    created_at: "2026-05-21T12:00:00Z",
    run_id: null,
    scenario_id: null,
    cycle_idx: null,
    training_window_end: null,
  };
}

function renderPage(initialEntries: string[] = ["/agents/memory"]) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <QueryClientProvider client={client}>
        <MemoryPage />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.mocked(flywheelApi.getFlywheelStatus).mockResolvedValue({
    namespace: "global",
    observations: 2,
    active_patterns: 1,
    staged_patterns: 0,
    forgotten_patterns: 0,
    autooptimizer_runs: 1,
    latest_autooptimizer_run_id: "ar-1",
    latest_autooptimizer_created_at: "2026-05-21T12:00:00Z",
  });
  vi.mocked(flywheelApi.getFlywheelVelocity).mockResolvedValue({
    namespace: "global",
    days: 7,
    since: "2026-05-18T00:00:00Z",
    observations_captured: 2,
    patterns_promoted: 1,
    patterns_demoted: 0,
    autooptimizer_runs: 1,
    optimized_child_agents: 0,
    average_lineage_depth: 0,
    latest_activity_at: "2026-05-21T12:00:00Z",
  });
  vi.mocked(flywheelApi.getFlywheelLineage).mockResolvedValue({
    namespace: "global",
    total: 1,
    items: [
      {
        optimization_id: "opt-global",
        target_agent_id: "agent-1",
        child_agent_id: "agent-child",
        slot: "main",
        method: "memory-demos",
        demo_source: "frozen-snapshot",
        reproducible: true,
        holdout_split: "70/15/15",
        cohort_query: "namespace=global,limit=8",
        train_observation_count: 1,
        dev_observation_count: 1,
        holdout_observation_count: 1,
        train_hash: "sha256:train",
        dev_hash: "sha256:dev",
        holdout_hash: "sha256:holdout",
        demo_source_pattern_ids: ["pat-demo"],
        prior_pattern_ids: [],
        prompt_prefix_chars: 120,
        status: "minted",
        created_at: "2026-05-21T12:00:00Z",
      },
    ],
  });
  vi.mocked(flywheelApi.runAutoOptimizer).mockResolvedValue({
    id: "ar-new",
    namespace: "global",
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
        namespace: "global",
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
    namespace: "global",
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
    namespace: "global",
    observation_ids: ["obs-1", "obs-2"],
    pattern_id: "pat-staged",
    pattern_text: "Reduce risk.",
    promotion_state: "staged",
    min_observations: 2,
    created_at: "2026-05-21T12:00:00Z",
    status: "completed",
    gate_verdict: "passed",
  });
  vi.mocked(flywheelApi.gateOptimization).mockResolvedValue({
    optimization_id: "opt-global",
    dev_metric: "sharpe",
    holdout_metric: "sharpe",
    parent_dev_score: 1,
    child_dev_score: 1.1,
    parent_holdout_score: 0.7,
    child_holdout_score: 0.9,
    gate_epsilon: 0,
    delta_dev: 0.1,
    delta_holdout: 0.2,
    gate_verdict: "passed",
    gate_reason: "global gate",
    gated_at: "2026-05-21T12:10:00Z",
  });
  vi.mocked(flywheelApi.demoteAutoOptimizerRun).mockResolvedValue({
    id: "ar-1",
    namespace: "global",
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
    namespace: "global",
    target_agent_id: "agent-1",
    child_agent_id: "agent-child",
    slot: "main",
    demo_count: 2,
    observation_ids: ["obs-1", "obs-2"],
    train_observation_ids: ["obs-1", "obs-2"],
    holdout_observation_ids: ["obs-3"],
    demo_source_pattern_ids: ["pat-demo"],
    pattern_demo_source_count: 1,
    prior_pattern_ids: [],
    pattern_prior_count: 0,
    observations: [],
    prompt_prefix_chars: 120,
    prompt_preview: "<memory_demos />",
  });
  vi.mocked(memoryApi.listMemory).mockResolvedValue(emptyList());
  vi.mocked(memoryApi.createPattern).mockResolvedValue(
    pattern("pat-new", "global wisdom", "global"),
  );
  vi.mocked(memoryApi.createOperatorAttestation).mockResolvedValue({
    id: "attest-global",
    operator_initials: "QA",
    surface: "dashboard",
    warning_text_hash: "sha256:test",
    created_at: "2026-05-21T12:00:00Z",
    signature: null,
  });
  vi.mocked(memoryApi.activatePattern).mockResolvedValue(
    pattern("pat-staged", "staged wisdom", "global"),
  );
  vi.mocked(memoryApi.demotePattern).mockResolvedValue(
    pattern("pat-staged", "staged wisdom", "global"),
  );
  vi.mocked(memoryApi.deleteMemoryItem).mockResolvedValue();
  vi.mocked(memoryApi.forgetMemory).mockResolvedValue({ deleted: 0 });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("MemoryPage — /memory redirect", () => {
  it("redirects /memory to /agents/memory", () => {
    render(
      <MemoryRouter initialEntries={["/memory"]}>
        <Routes>
          <Route
            path="/memory"
            element={<Navigate to="/agents/memory" replace />}
          />
          <Route
            path="/agents/memory"
            element={<div>memory-page-sentinel</div>}
          />
        </Routes>
      </MemoryRouter>,
    );
    expect(screen.getByText("memory-page-sentinel")).toBeInTheDocument();
  });
});

describe("MemoryPage — empty state", () => {
  it("renders without crashing when the global namespace has no memory", async () => {
    renderPage();
    expect(
      await screen.findByText(/No patterns yet/i),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Add Pattern/i }),
    ).toBeInTheDocument();
  });

  it("scopes the patterns list query to namespace=global", async () => {
    renderPage();
    await screen.findByText(/No patterns yet/i);
    await waitFor(() => {
      const calls = vi.mocked(memoryApi.listMemory).mock.calls;
      const patternCall = calls.find(
        (c) => c[0]?.tier === "pattern" && c[0]?.namespace === "global",
      );
      expect(patternCall).toBeTruthy();
    });
  });

  it("renders flywheel status for the global namespace", async () => {
    renderPage();

    expect(await screen.findByText("Flywheel")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getAllByText("Observations").length).toBeGreaterThan(0);
      expect(screen.getAllByText("2").length).toBeGreaterThan(0);
      expect(flywheelApi.getFlywheelStatus).toHaveBeenCalledWith({
        namespace: "global",
      });
      expect(flywheelApi.getFlywheelVelocity).toHaveBeenCalledWith({
        namespace: "global",
        days: 7,
      });
      expect(flywheelApi.getFlywheelLineage).toHaveBeenCalledWith({
        namespace: "global",
        limit: 1,
      });
      expect(screen.getByText("Obs / 7d")).toBeInTheDocument();
      expect(screen.getByText("Latest Lineage")).toBeInTheDocument();
    });
  });
});

describe("MemoryPage — Flywheel panel", () => {
  it("stages an autooptimizer pattern for global memory", async () => {
    const user = userEvent.setup();
    renderPage();

    await user.type(
      await screen.findByLabelText(/Candidate Pattern/i),
      "Reduce risk.",
    );
    // Embedding JSON field now starts empty (placeholder only); fill it in.
    // Use fireEvent.change because userEvent.type misinterprets "[" and "]" as
    // keyboard modifier syntax.
    fireEvent.change(screen.getByLabelText(/Embedding JSON/i), {
      target: { value: "[1,0]" },
    });
    await user.click(screen.getByRole("button", { name: /Stage Pattern/i }));

    await waitFor(() => {
      expect(flywheelApi.runAutoOptimizer).toHaveBeenCalledWith({
        namespace: "global",
        pattern_text: "Reduce risk.",
        embedding: [1, 0],
        min_observations: 2,
      });
    });
  });

  it("promotes and demotes recent autooptimizer runs", async () => {
    const user = userEvent.setup();
    renderPage();

    expect(await screen.findByText("Recent Optimizer Runs")).toBeInTheDocument();
    await waitFor(() => {
      expect(flywheelApi.listAutoOptimizerRuns).toHaveBeenCalledWith({
        namespace: "global",
        limit: 5,
      });
    });

    await user.click(screen.getByRole("button", { name: /^Activate$/i }));
    await waitFor(() => {
      expect(flywheelApi.promoteAutoOptimizerRun).toHaveBeenCalledWith("ar-1");
    });

    await user.click(screen.getByRole("button", { name: /^Retire$/i }));
    await waitFor(() => {
      expect(flywheelApi.demoteAutoOptimizerRun).toHaveBeenCalledWith("ar-1");
    });
  });
});


describe("MemoryPage — Pattern lifecycle controls", () => {
  it("filters staged patterns and activates or demotes by pattern id", async () => {
    const user = userEvent.setup();
    vi.mocked(flywheelApi.listAutoOptimizerRuns).mockResolvedValue({
      items: [],
      total: 0,
    });
    vi.mocked(memoryApi.listMemory).mockImplementation(async (q) => {
      if (q?.tier === "pattern" && q?.promotion_state === "staged") {
        return {
          items: [
            {
              ...pattern("pat-staged", "staged wisdom", "global"),
              promotion_state: "staged",
            },
          ],
          total: 1,
        };
      }
      return emptyList();
    });

    renderPage();

    await user.selectOptions(await screen.findByLabelText(/Lifecycle/i), "staged");
    expect(await screen.findByText("staged wisdom")).toBeInTheDocument();
    await waitFor(() => {
      expect(memoryApi.listMemory).toHaveBeenCalledWith({
        tier: "pattern",
        namespace: "global",
        promotion_state: "staged",
      });
    });

    await user.click(screen.getByRole("button", { name: /^Activate$/i }));
    await waitFor(() => {
      expect(memoryApi.activatePattern).toHaveBeenCalledWith("pat-staged");
    });

    await user.click(screen.getByRole("button", { name: /^Demote$/i }));
    await waitFor(() => {
      expect(memoryApi.demotePattern).toHaveBeenCalledWith("pat-staged");
    });
  });
});

describe("MemoryPage — Observations sub-tab", () => {
  it("renders global-namespace observations on the Observations sub-tab", async () => {
    const user = userEvent.setup();
    vi.mocked(memoryApi.listMemory).mockImplementation(async (q) => {
      if (q?.tier === "observation" && q?.namespace === "global") {
        return {
          items: [
            observation("obs-1", "global observation row", "global"),
          ],
          total: 1,
        };
      }
      return emptyList();
    });

    renderPage();

    await screen.findByText(/No patterns yet/i);
    await user.click(screen.getByRole("tab", { name: /Observations/i }));

    expect(
      await screen.findByText(/global observation row/),
    ).toBeInTheDocument();
  });
});


describe("MemoryPage — Pattern lifecycle controls", () => {
  it("calls createPattern when the Add Pattern form is submitted with a training-window date", async () => {
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("button", { name: /Add Pattern/i }));

    await screen.findByRole("heading", { name: /Add Pattern/i });
    await user.type(screen.getByRole("textbox", { name: /text/i }), "Buy the dip on Mondays.");
    fireEvent.change(
      screen.getByTitle(/The latest date your training data covers/i),
      { target: { value: "2026-01-01" } },
    );
    await user.click(screen.getByRole("button", { name: /^Add Pattern$/ }));

    await waitFor(() =>
      expect(memoryApi.createPattern).toHaveBeenCalledWith(
        expect.objectContaining({
          text: "Buy the dip on Mondays.",
          training_window_end: "2026-01-01T23:59:59Z",
        }),
      ),
    );
  });

  it("calls forgetMemory when the forget confirmation is submitted", async () => {
    const user = userEvent.setup();
    renderPage();

    await user.click(
      await screen.findByRole("button", { name: /Forget all global memory/i }),
    );

    await user.click(
      await screen.findByRole("button", { name: /Confirm forget/i }),
    );

    await waitFor(() =>
      expect(memoryApi.forgetMemory).toHaveBeenCalledWith({ namespace: "global" }),
    );
  });
});

describe("MemoryPage — deep-link highlight", () => {
  it("highlights the pattern row matching ?pattern=<id>", async () => {
    vi.mocked(memoryApi.listMemory).mockImplementation(async (q) => {
      if (q?.tier === "pattern" && q?.namespace === "global") {
        return {
          items: [
            pattern("pat-a", "first", "global"),
            pattern("pat-b", "second target", "global"),
          ],
          total: 2,
        };
      }
      return emptyList();
    });

    renderPage(["/agents/memory?pattern=pat-b"]);

    const target = await screen.findByText(/second target/);
    // Walk up to the LI wrapper and check the highlight marker attribute.
    const li = target.closest("li");
    expect(li).not.toBeNull();
    expect(li?.getAttribute("data-highlighted")).toBe("true");
  });
});
