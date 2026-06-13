// frontend/web/src/features/agent-runs/StripDockSlot.test.tsx
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { StripDockSlot } from "./StripDockSlot";
import { useTraceDock } from "@/stores/trace-dock";
import type { DockMode } from "@/stores/trace-dock";
import * as agentRunsApi from "@/api/agent-runs";
import * as evalApi from "@/api/eval";
import type { AgentRunDetail, AgentRunSummary } from "@/api/types-agent-runs";
import type { RunDetail, RunSummary as EvalRunSummary } from "@/api/types.gen";

vi.mock("@/api/agent-runs", async () => {
  const actual = await vi.importActual<typeof import("@/api/agent-runs")>(
    "@/api/agent-runs",
  );
  return {
    ...actual,
    getAgentRun: vi.fn(),
  };
});

vi.mock("@/api/eval", async () => {
  const actual =
    await vi.importActual<typeof import("@/api/eval")>("@/api/eval");
  return {
    ...actual,
    listRuns: vi.fn(),
    getRun: vi.fn(),
  };
});

vi.mock("@/api/agents", async () => {
  const actual =
    await vi.importActual<typeof import("@/api/agents")>("@/api/agents");
  return { ...actual, listAgents: vi.fn().mockResolvedValue([]) };
});

vi.mock("@/api/scenarios", async () => {
  const actual =
    await vi.importActual<typeof import("@/api/scenarios")>("@/api/scenarios");
  return { ...actual, listScenarios: vi.fn().mockResolvedValue([]) };
});

function renderSlot() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <StripDockSlot />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

/**
 * Seed the eval scope slice — the default MemoryRouter path "/" maps to
 * the eval scope, which is the slice StripDockSlot reads here. The live
 * scope stays at its init state.
 */
function setEvalScope(slice: {
  activeRunId?: string | null;
  selectedSpanId?: string | null;
  mode?: DockMode;
  costOverrideUsd?: number | null;
}) {
  useTraceDock.setState((s) => ({
    byScope: {
      ...s.byScope,
      eval: {
        activeRunId: slice.activeRunId ?? null,
        selectedSpanId: slice.selectedSpanId ?? null,
        mode: slice.mode ?? "post-hoc",
        costOverrideUsd: slice.costOverrideUsd ?? null,
      },
    },
  }));
}

function makeSummary(overrides: Partial<AgentRunSummary> = {}): AgentRunSummary {
  return {
    run_id: "run_abc1234",
    objective: "demo run",
    strategy_id: null,
    agent_id: null,
    started_at: "2026-05-18T14:00:00Z",
    finished_at: null,
    status: "running",
    span_count: 5,
    model_call_count: 2,
    tool_call_count: 1,
    error_count: 0,
    total_cost_usd: 0.01,
    total_input_tokens: 1000,
    total_output_tokens: 500,
    duration_ms: null,
    financial_eval_id: null,
    retention_mode: "hash_only",
    ...overrides,
  };
}

function makeDetail(overrides: Partial<AgentRunSummary> = {}): AgentRunDetail {
  return {
    summary: makeSummary(overrides),
    spans: [],
    model_calls: [],
    tool_calls: [],
  } as unknown as AgentRunDetail;
}

describe("StripDockSlot", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useTraceDock.setState({
      height: "collapsed",
      lastOpenHeight: "working",
    });
    setEvalScope({ activeRunId: null, selectedSpanId: null, mode: "post-hoc" });
  });
  afterEach(() => cleanup());

  test("renders nothing when activeRunId is null", () => {
    renderSlot();
    expect(screen.queryByTestId("run-status-strip")).toBeNull();
    expect(screen.queryByTestId("trace-dock")).toBeNull();
  });

  test("renders RunStatusStrip when activeRunId set and height=collapsed", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(makeDetail());
    setEvalScope({ activeRunId: "run_abc1234", mode: "live" });
    useTraceDock.setState({ height: "collapsed" });
    renderSlot();
    await waitFor(() =>
      expect(screen.getByTestId("run-status-strip")).toBeInTheDocument(),
    );
  });

  test("renders TraceDock when height is non-collapsed", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(makeDetail());
    setEvalScope({ activeRunId: "run_abc1234", mode: "live" });
    useTraceDock.setState({ height: "working" });
    renderSlot();
    await waitFor(() =>
      expect(screen.getByTestId("trace-dock")).toBeInTheDocument(),
    );
    expect(screen.queryByTestId("run-status-strip")).toBeNull();
  });

  test("freezes the capsule timer when the eval inspector flips mode to post-hoc, even if agent-run summary still says running", async () => {
    // Backend lag scenario: the eval-run got cancelled and the inspector
    // set mode="post-hoc", but the agent_run summary hasn't propagated
    // the terminal status yet. The capsule must NOT continue counting.
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(
      makeDetail({
        status: "running",
        // Cancel landed but finished_at hasn't been flushed to the
        // agent-run summary; the strip should still freeze.
        duration_ms: null,
      }),
    );
    setEvalScope({ activeRunId: "run_abc1234", mode: "post-hoc" });
    useTraceDock.setState({ height: "collapsed" });
    renderSlot();

    const strip = await screen.findByTestId("run-status-strip");
    // The pulsing LIVE leading dot only renders when isLive=true.
    // The strip's leading-dot span carries `animate-pulse` exactly when live.
    expect(strip.querySelector(".animate-pulse")).toBeNull();
  });

  test("falls back to finished_at - started_at when the agent-run summary lacks duration_ms", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(
      makeDetail({
        status: "cancelled",
        started_at: "2026-05-18T14:00:00Z",
        finished_at: "2026-05-18T14:00:32Z",
        duration_ms: null,
      }),
    );
    setEvalScope({ activeRunId: "run_abc1234", mode: "post-hoc" });
    useTraceDock.setState({ height: "collapsed" });
    renderSlot();

    const strip = await screen.findByTestId("run-status-strip");
    // 32_000 ms → "32.0s" via fmtPostHoc.
    expect(strip.textContent ?? "").toMatch(/32\.0s/);
  });

  test("live-money run renders the LIVE capsule prefix; default renders EVAL", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(
      makeDetail({ is_live_money: true, eval_mode: "live" }),
    );
    setEvalScope({ activeRunId: "run_abc1234", mode: "live" });
    useTraceDock.setState({ height: "collapsed" });
    renderSlot();
    const label = await screen.findByTestId("capsule-kind-label");
    expect(label).toHaveTextContent("LIVE");
  });

  test("non-live run keeps the EVAL capsule prefix", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(
      makeDetail({ eval_mode: "backtest" }),
    );
    setEvalScope({ activeRunId: "run_abc1234", mode: "live" });
    useTraceDock.setState({ height: "collapsed" });
    renderSlot();
    const label = await screen.findByTestId("capsule-kind-label");
    expect(label).toHaveTextContent("EVAL");
  });

  test("does NOT poll sibling eval-runs when the focused agent-run has no financial_eval_id", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(
      makeDetail({ financial_eval_id: null }),
    );
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setEvalScope({ activeRunId: "run_abc1234", mode: "live" });
    useTraceDock.setState({ height: "collapsed" });
    renderSlot();
    await screen.findByTestId("run-status-strip");
    // Scope guard: live-strategy runs (no eval link) must not trigger the
    // eval-runs polling. listRuns is only the eval-runs endpoint, so it
    // should never be invoked here.
    expect(evalApi.listRuns).not.toHaveBeenCalled();
  });

  test("polls both running and failed eval-runs when an eval link is present", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(
      makeDetail({ financial_eval_id: "eval_focus_xyz" }),
    );
    vi.mocked(evalApi.getRun).mockResolvedValue({
      summary: {
        id: "eval_focus_xyz",
        agent_id: "ag1",
        scenario_id: "sc1",
        mode: "backtest",
        status: "running",
        started_at: "2026-05-19T20:00:00Z",
        completed_at: null,
        sharpe: null,
        max_drawdown_pct: null,
        total_return_pct: null,
        error: null,
        actual_input_tokens: null,
        actual_output_tokens: null,
      } as EvalRunSummary,
    } as RunDetail);
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setEvalScope({ activeRunId: "run_abc1234", mode: "live" });
    useTraceDock.setState({ height: "collapsed" });
    renderSlot();
    await screen.findByTestId("run-status-strip");
    // Both slices polled — running and failed — so freshly-errored
    // siblings can auto-promote.
    await waitFor(() => {
      expect(evalApi.listRuns).toHaveBeenCalledWith({ status: "running" });
      expect(evalApi.listRuns).toHaveBeenCalledWith({ status: "failed" });
    });
  });

  test("surfaces a recently-failed sibling so error promotion can fire", async () => {
    const nowIso = new Date(Date.now() - 30_000).toISOString();
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(
      makeDetail({ financial_eval_id: "eval_focus_xyz" }),
    );
    vi.mocked(evalApi.getRun).mockResolvedValue({
      summary: {
        id: "eval_focus_xyz",
        agent_id: "ag1",
        scenario_id: "sc1",
        mode: "backtest",
        status: "running",
        started_at: "2026-05-19T20:00:00Z",
        completed_at: null,
        sharpe: null,
        max_drawdown_pct: null,
        total_return_pct: null,
        error: null,
        actual_input_tokens: null,
        actual_output_tokens: null,
      } as EvalRunSummary,
    } as RunDetail);

    vi.mocked(evalApi.listRuns).mockImplementation((params) => {
      if (params?.status === "failed") {
        return Promise.resolve([
          {
            id: "eval_failed_sibling",
            agent_id: "ag-failed",
            scenario_id: "sc-failed",
            mode: "backtest",
            status: "failed",
            started_at: new Date(Date.now() - 60_000).toISOString(),
            completed_at: nowIso,
            sharpe: null,
            max_drawdown_pct: null,
            total_return_pct: null,
            error: "boom",
            actual_input_tokens: null,
            actual_output_tokens: null,
          },
        ] as EvalRunSummary[]);
      }
      return Promise.resolve([]);
    });

    setEvalScope({ activeRunId: "run_abc1234", mode: "live" });
    useTraceDock.setState({ height: "collapsed" });
    renderSlot();
    const strip = await screen.findByTestId("run-status-strip");
    // The failed sibling surfaces both as a row (one of N + the focused
    // row, so `2 EVALS RUNNING ON CLUSTER` appears in the footer) and as
    // an ERROR-toned line inside the expanded stack. The capsule auto-
    // opens on a new error, so the sibling row is visible without manual
    // interaction. `failed` is also the trailing 6-char id-slice for the
    // fixture's `agent_id="ag-failed"`, so it appears in the short tag.
    await waitFor(() => {
      expect(strip.textContent ?? "").toMatch(/2 EVALS RUNNING/);
      expect(strip.textContent ?? "").toMatch(/ERROR/);
    });
  });

  test("drops failed siblings whose completion is outside the recency window", async () => {
    const oldIso = new Date(Date.now() - 10 * 60_000).toISOString();
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(
      makeDetail({ financial_eval_id: "eval_focus_xyz" }),
    );
    vi.mocked(evalApi.getRun).mockResolvedValue({
      summary: {
        id: "eval_focus_xyz",
        agent_id: "ag1",
        scenario_id: "sc1",
        mode: "backtest",
        status: "running",
        started_at: "2026-05-19T20:00:00Z",
        completed_at: null,
        sharpe: null,
        max_drawdown_pct: null,
        total_return_pct: null,
        error: null,
        actual_input_tokens: null,
        actual_output_tokens: null,
      } as EvalRunSummary,
    } as RunDetail);
    vi.mocked(evalApi.listRuns).mockImplementation((params) => {
      if (params?.status === "failed") {
        return Promise.resolve([
          {
            id: "eval_stale_failure",
            agent_id: "ag-stale",
            scenario_id: "sc-stale",
            mode: "backtest",
            status: "failed",
            started_at: oldIso,
            completed_at: oldIso,
            sharpe: null,
            max_drawdown_pct: null,
            total_return_pct: null,
            error: "old",
            actual_input_tokens: null,
            actual_output_tokens: null,
          },
        ] as EvalRunSummary[]);
      }
      return Promise.resolve([]);
    });
    setEvalScope({ activeRunId: "run_abc1234", mode: "live" });
    useTraceDock.setState({ height: "collapsed" });
    renderSlot();
    const strip = await screen.findByTestId("run-status-strip");
    // Wait briefly to give the failedQ a chance to resolve, then assert
    // the stale failure never appears.
    await new Promise((r) => setTimeout(r, 50));
    expect(strip.textContent ?? "").not.toMatch(/c-stale/);
  });
});
