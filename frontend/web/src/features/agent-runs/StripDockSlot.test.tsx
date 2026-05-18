// frontend/web/src/features/agent-runs/StripDockSlot.test.tsx
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { StripDockSlot } from "./StripDockSlot";
import { useTraceDock } from "@/stores/trace-dock";
import * as agentRunsApi from "@/api/agent-runs";
import type { AgentRunDetail, AgentRunSummary } from "@/api/types-agent-runs";

vi.mock("@/api/agent-runs", async () => {
  const actual = await vi.importActual<typeof import("@/api/agent-runs")>(
    "@/api/agent-runs",
  );
  return {
    ...actual,
    getAgentRun: vi.fn(),
  };
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
      selectedSpanId: null,
      activeRunId: null,
      mode: "post-hoc",
      lastOpenHeight: "working",
    });
  });
  afterEach(() => cleanup());

  test("renders nothing when activeRunId is null", () => {
    renderSlot();
    expect(screen.queryByTestId("run-status-strip")).toBeNull();
    expect(screen.queryByTestId("trace-dock")).toBeNull();
  });

  test("renders RunStatusStrip when activeRunId set and height=collapsed", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(makeDetail());
    useTraceDock.setState({
      activeRunId: "run_abc1234",
      height: "collapsed",
      mode: "live",
    });
    renderSlot();
    await waitFor(() =>
      expect(screen.getByTestId("run-status-strip")).toBeInTheDocument(),
    );
  });

  test("renders TraceDock when height is non-collapsed", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(makeDetail());
    useTraceDock.setState({
      activeRunId: "run_abc1234",
      height: "working",
      mode: "live",
    });
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
    useTraceDock.setState({
      activeRunId: "run_abc1234",
      height: "collapsed",
      mode: "post-hoc",
    });
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
    useTraceDock.setState({
      activeRunId: "run_abc1234",
      height: "collapsed",
      mode: "post-hoc",
    });
    renderSlot();

    const strip = await screen.findByTestId("run-status-strip");
    // 32_000 ms → "32.0s" via fmtPostHoc.
    expect(strip.textContent ?? "").toMatch(/32\.0s/);
  });
});
