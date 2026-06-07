// Tests for the Phase 3.7 optimizer run-detail surface + the
// "Improve this agent" panel.
//
// Asserts:
//  - the run detail renders the candidate table, before/after prompt diff,
//    metric delta, holdout split column, and accept action;
//  - a FAILED run still renders its partial candidates (no empty/error state);
//  - the accept action calls the API and flips to the revert affordance;
//  - the surface is NOT a popup (no dialog/modal role) — it routes inline;
//  - the ImproveAgentPanel lists runs and links each to the detail route.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  render,
  screen,
  act,
  fireEvent,
  waitFor,
} from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  getOptimization,
  listOptimizations,
  acceptOptimization,
  recordOptimizationHoldout,
  type RunDetail,
  type OptimizationRun,
} from "@/api/optimizations";
import { getAgent, type Agent } from "@/api/agents";
import { OptimizationDetailRoute } from "./optimizations-detail";
import { ImproveAgentPanel } from "@/components/agent/ImproveAgentPanel";

vi.mock("@/api/optimizations", async () => {
  const actual = await vi.importActual<typeof import("@/api/optimizations")>(
    "@/api/optimizations",
  );
  return {
    ...actual,
    getOptimization: vi.fn(),
    listOptimizations: vi.fn(),
    acceptOptimization: vi.fn(),
    revertOptimization: vi.fn(),
    recordOptimizationHoldout: vi.fn(),
    waiveOptimizationOverfit: vi.fn(),
  };
});

vi.mock("@/api/agents", async () => {
  const actual = await vi.importActual<typeof import("@/api/agents")>(
    "@/api/agents",
  );
  return {
    ...actual,
    getAgent: vi.fn(),
  };
});

function makeQC() {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const PARENT_PROMPT = "You are a careful trader. Respect stop-losses.";
const SELECTED_INSTRUCTION = "OPTIMIZED: be decisive; size by conviction.";

function sampleAgent(): Agent {
  return {
    agent_id: "01AGENTPARENT",
    name: "Parent Trader",
    description: "",
    tags: [],
    slots: [
      {
        name: "trader",
        provider: "anthropic",
        model: "claude-sonnet-4-6",
        system_prompt: PARENT_PROMPT,
        skill_ids: [],
    allowed_tools: [],
        max_tokens: null,
      } as Agent["slots"][number],
    ],
    archived: false,
    created_at: "2026-05-24T00:00:00Z",
    updated_at: "2026-05-24T00:00:00Z",
  } as Agent;
}

function sampleDetail(status = "completed"): RunDetail {
  return {
    run: {
      id: "01RUN",
      agent_id: "01AGENTPARENT",
      slot_name: "trader",
      capability: "trader",
      optimizer: "mipro",
      metric: "delta_sharpe",
      corpus_query: "scenario:bull-2024",
      rng_seed: 42,
      model_provider: "dummy",
      model_name: "dummy",
      signature_hash: "abc123",
      optimizer_version: "dspy-rs-0.7.3",
      status,
      created_at: "2026-05-24T00:00:00Z",
    },
    candidates: [
      {
        id: "c0",
        run_id: "01RUN",
        candidate_index: 0,
        instruction: "baseline instruction",
        metric_value: 0.1,
        split: "train",
        demo_set: null,
        selected: false,
      },
      {
        id: "c1",
        run_id: "01RUN",
        candidate_index: 1,
        instruction: SELECTED_INSTRUCTION,
        metric_value: 0.42,
        split: "holdout",
        demo_set: null,
        selected: true,
      },
    ],
    snapshots: [
      {
        id: "01SNAP",
        run_id: "01RUN",
        snapshot_json: "{}",
        signature_hash: "abc123",
        demo_set: null,
        accepted: false,
        created_at: "2026-05-24T00:00:00Z",
      },
    ],
    holdouts: [
      {
        snapshot_id: "01SNAP",
        run_id: "01RUN",
        metric: "delta_sharpe",
        train_metric_value: 0.42,
        holdout_metric_value: 0.4,
        overfit_warning: false,
        overfit_ratio: 0.0476,
        overfit_waiver_reason: null,
        created_at: "2026-05-24T00:00:00Z",
      },
    ],
    lineage: [],
  };
}

function sampleDetailWithoutHoldout(status = "completed"): RunDetail {
  const detail = sampleDetail(status);
  detail.holdouts = [];
  return detail;
}

function renderDetail(qc = makeQC()) {
  return render(
    <MemoryRouter initialEntries={["/agents/01AGENTPARENT/optimizations/01RUN"]}>
      <QueryClientProvider client={qc}>
        <Routes>
          <Route
            path="/agents/:id/optimizations/:runId"
            element={<OptimizationDetailRoute />}
          />
          <Route path="/agents/:id" element={<div>agent page</div>} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("OptimizationDetailRoute", () => {
  it("renders candidate table, prompt diff, metric delta, and holdout split", async () => {
    vi.mocked(getOptimization).mockResolvedValue(sampleDetail());
    vi.mocked(getAgent).mockResolvedValue(sampleAgent());

    renderDetail();

    await screen.findByTestId("optimization-detail");

    // Candidate table: both candidates present.
    const table = screen.getByTestId("candidate-table");
    expect(table).toBeTruthy();
    expect(screen.getByTestId("candidate-row-0")).toBeTruthy();
    expect(screen.getByTestId("candidate-row-1")).toBeTruthy();

    // Holdout split column value is shown.
    expect(screen.getByText("holdout")).toBeTruthy();

    // Metric delta = 0.42 - 0.1 = +0.32.
    const delta = screen.getByTestId("opt-delta");
    expect(delta.textContent).toContain("+0.3200");

    // Before/after prompt diff.
    expect(screen.getByTestId("prompt-before").textContent).toContain(
      PARENT_PROMPT,
    );
    expect(screen.getByTestId("prompt-after").textContent).toContain(
      SELECTED_INSTRUCTION,
    );

    // Accept affordance present; not yet accepted.
    expect(screen.getByTestId("accept-button")).toBeTruthy();
    // Evidence export link points at the JSON detail endpoint.
    const exportLink = screen.getByTestId("evidence-export") as HTMLAnchorElement;
    expect(exportLink.getAttribute("href")).toContain(
      "/api/optimizations/01RUN",
    );
  });

  it("is not a popup — renders inline with no dialog role", async () => {
    vi.mocked(getOptimization).mockResolvedValue(sampleDetail());
    vi.mocked(getAgent).mockResolvedValue(sampleAgent());
    renderDetail();
    await screen.findByTestId("optimization-detail");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("renders partial candidates for a FAILED run (preserves evidence)", async () => {
    vi.mocked(getOptimization).mockResolvedValue(sampleDetail("failed"));
    vi.mocked(getAgent).mockResolvedValue(sampleAgent());

    renderDetail();
    await screen.findByTestId("optimization-detail");

    // Failed banner shown.
    expect(screen.getByText(/optimization run failed/i)).toBeTruthy();
    // …but candidates still render.
    expect(screen.getByTestId("candidate-row-0")).toBeTruthy();
    expect(screen.getByTestId("candidate-row-1")).toBeTruthy();
    expect(screen.getByTestId("opt-status").textContent).toContain("failed");
  });

  it("accept calls the API and invalidates", async () => {
    vi.mocked(getOptimization).mockResolvedValue(sampleDetail());
    vi.mocked(getAgent).mockResolvedValue(sampleAgent());
    vi.mocked(acceptOptimization).mockResolvedValue({
      child_agent: { ...sampleAgent(), agent_id: "01CHILD" },
      lineage: {
        child_agent_id: "01CHILD",
        parent_agent_id: "01AGENTPARENT",
        optimization_run_id: "01RUN",
        created_at: "2026-05-24T00:00:00Z",
      },
      snapshot_id: "01SNAP",
      accepted: true,
      holdout_present: true,
      override_reason: null,
      overfit_warning: false,
    });

    renderDetail();
    await screen.findByTestId("optimization-detail");

    await act(async () => {
      fireEvent.click(screen.getByTestId("accept-button"));
    });

    await waitFor(() => {
      expect(acceptOptimization).toHaveBeenCalledWith(
        "01RUN",
        "01SNAP",
        undefined,
        undefined,
      );
    });
  });

  it("surfaces holdout recording before accept when the snapshot has no holdout", async () => {
    vi.mocked(getOptimization).mockResolvedValue(sampleDetailWithoutHoldout());
    vi.mocked(getAgent).mockResolvedValue(sampleAgent());
    vi.mocked(recordOptimizationHoldout).mockResolvedValue({
      snapshot_id: "01SNAP",
      run_id: "01RUN",
      metric: "delta_sharpe",
      train_metric_value: 0.42,
      holdout_metric_value: 0.39,
      overfit_warning: false,
      overfit_ratio: 0.0714,
      overfit_waiver_reason: null,
      created_at: "2026-05-24T00:00:00Z",
    });

    renderDetail();
    await screen.findByTestId("optimization-detail");

    expect(screen.getByTestId("accept-button")).toHaveAttribute("disabled");
    fireEvent.change(screen.getByTestId("holdout-train-input"), {
      target: { value: "0.42" },
    });
    fireEvent.change(screen.getByTestId("holdout-value-input"), {
      target: { value: "0.39" },
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId("record-holdout-button"));
    });

    await waitFor(() => {
      expect(recordOptimizationHoldout).toHaveBeenCalledWith(
        "01RUN",
        "01SNAP",
        {
          metric: "delta_sharpe",
          trainMetricValue: 0.42,
          holdoutMetricValue: 0.39,
        },
      );
    });
  });

  it("passes an override reason to accept when no holdout is recorded", async () => {
    vi.mocked(getOptimization).mockResolvedValue(sampleDetailWithoutHoldout());
    vi.mocked(getAgent).mockResolvedValue(sampleAgent());
    vi.mocked(acceptOptimization).mockResolvedValue({
      child_agent: { ...sampleAgent(), agent_id: "01CHILD" },
      lineage: {
        child_agent_id: "01CHILD",
        parent_agent_id: "01AGENTPARENT",
        optimization_run_id: "01RUN",
        created_at: "2026-05-24T00:00:00Z",
      },
      snapshot_id: "01SNAP",
      accepted: true,
      holdout_present: false,
      override_reason: "operator reviewed manually",
      overfit_warning: false,
    });

    renderDetail();
    await screen.findByTestId("optimization-detail");

    fireEvent.change(screen.getByTestId("holdout-override-input"), {
      target: { value: "operator reviewed manually" },
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId("accept-button"));
    });

    await waitFor(() => {
      expect(acceptOptimization).toHaveBeenCalledWith(
        "01RUN",
        "01SNAP",
        undefined,
        "operator reviewed manually",
      );
    });
  });

  it("shows revert affordance when an accepted snapshot + lineage exist", async () => {
    const detail = sampleDetail();
    detail.snapshots[0].accepted = true;
    detail.lineage = [
      {
        child_agent_id: "01CHILD",
        parent_agent_id: "01AGENTPARENT",
        optimization_run_id: "01RUN",
        created_at: "2026-05-24T00:00:00Z",
      },
    ];
    vi.mocked(getOptimization).mockResolvedValue(detail);
    vi.mocked(getAgent).mockResolvedValue(sampleAgent());

    renderDetail();
    await screen.findByTestId("optimization-detail");

    expect(screen.getByTestId("revert-button")).toBeTruthy();
    expect(screen.getByTestId("view-child-agent")).toBeTruthy();
    // The plain accept button is gone once accepted.
    expect(screen.queryByTestId("accept-button")).toBeNull();
  });

  it("MIPRO/GEPA internals are hidden until Advanced detail is toggled", async () => {
    vi.mocked(getOptimization).mockResolvedValue(sampleDetail());
    vi.mocked(getAgent).mockResolvedValue(sampleAgent());

    renderDetail();
    await screen.findByTestId("optimization-detail");

    // Advanced block not shown initially.
    expect(screen.queryByTestId("opt-advanced")).toBeNull();

    await act(async () => {
      fireEvent.click(screen.getByTestId("opt-advanced-toggle"));
    });
    const advanced = screen.getByTestId("opt-advanced");
    expect(advanced.textContent).toContain("mipro");
    expect(advanced.textContent).toContain("delta_sharpe");
  });
});

function renderPanel(qc = makeQC()) {
  return render(
    <MemoryRouter>
      <QueryClientProvider client={qc}>
        <ImproveAgentPanel agentId="01AGENTPARENT" />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("ImproveAgentPanel", () => {
  it("lists optimization runs and links each to the detail route", async () => {
    const runs: OptimizationRun[] = [sampleDetail().run];
    vi.mocked(listOptimizations).mockResolvedValue(runs);

    renderPanel();
    await screen.findByTestId("improve-agent-panel");

    const link = (await screen.findByTestId(
      "improve-run-link-01RUN",
    )) as HTMLAnchorElement;
    expect(link.getAttribute("href")).toContain(
      "/agents/01AGENTPARENT/optimizations/01RUN",
    );
  });

  it("shows an empty hint when there are no runs", async () => {
    vi.mocked(listOptimizations).mockResolvedValue([]);
    renderPanel();
    await screen.findByTestId("improve-agent-panel");
    // Wait for the list query to settle past its loading state.
    expect(await screen.findByText(/No optimization runs yet/i)).toBeTruthy();
  });
});
