// frontend/web/src/routes/live-run-detail.test.tsx
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { LiveRunDetailRoute } from "./live-run-detail";
import * as agentRunsApi from "@/api/agent-runs";
import * as evalApi from "@/api/eval";
import type { AgentRunDetail, AgentRunSummary } from "@/api/types-agent-runs";
import type { RunDetail } from "@/api/types.gen";

vi.mock("@/api/agent-runs", async () => {
  const actual = await vi.importActual<typeof import("@/api/agent-runs")>(
    "@/api/agent-runs",
  );
  return { ...actual, getAgentRun: vi.fn() };
});

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>(
    "@/api/eval",
  );
  return { ...actual, getRun: vi.fn() };
});

function makeSummary(over: Partial<AgentRunSummary> = {}): AgentRunSummary {
  return {
    run_id: "run_live1",
    objective: "Trade BTC live",
    strategy_id: null,
    agent_id: "agent_1",
    started_at: "2026-06-10T10:00:00Z",
    finished_at: null,
    status: "running",
    span_count: 3,
    model_call_count: 1,
    tool_call_count: 1,
    error_count: 0,
    total_cost_usd: 0.12,
    total_input_tokens: 1000,
    total_output_tokens: 200,
    duration_ms: null,
    financial_eval_id: "eval_1",
    retention_mode: "hash_only",
    is_live_money: true,
    eval_mode: "live",
    eval_run_status: "running",
    ...over,
  };
}

function makeDetail(over: Partial<AgentRunSummary> = {}): AgentRunDetail {
  return {
    summary: makeSummary(over),
    spans: [
      {
        span_id: "s1",
        parent_span_id: null,
        name: "agent.run",
        kind: "agent.run",
        started_at: "2026-06-10T10:00:00Z",
        finished_at: null,
        status: "in_progress",
        attributes: {},
      },
    ],
    model_calls: [],
    tool_calls: [],
  };
}

function makeEvalDetail(): RunDetail {
  return {
    summary: {
      id: "eval_1",
      agent_id: "agent_1",
      scenario_id: "",
      strategy: null,
      scenario: { id: "", display_name: "BTC momentum (live)" },
      mode: "live",
      status: "running",
      started_at: "2026-06-10T10:00:00Z",
      completed_at: null,
      sharpe: null,
      max_drawdown_pct: null,
      total_return_pct: 1.42,
      error: null,
      actual_input_tokens: null,
      actual_output_tokens: null,
      inference_cost_quote_total: null,
      net_return_pct: null,
      filter_summaries: [],
      auto_fire_review: false,
      review_model: null,
      max_annotations_per_review: null,
      paused: false,
      paused_at: null,
      flatten_requested: false,
      unrealized_pnl_usd: null,
      skipped_dispatches: 0,
      delayed_decisions: 0,
      forced_cancels: 0,
    },
    decisions: [],
    equity_curve: [],
    filter_events: [],
    filter_summaries: [],
  };
}

function renderAt(path: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/live/runs/:runId" element={<LiveRunDetailRoute />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(evalApi.getRun).mockResolvedValue(makeEvalDetail());
});
afterEach(() => cleanup());

describe("LiveRunDetailRoute", () => {
  test("renders the LIVE badge, deployment name, pnl, and timeline for a live run", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(makeDetail());
    renderAt("/live/runs/run_live1");

    await waitFor(() =>
      expect(screen.getByTestId("live-run-header")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("live-badge")).toHaveTextContent("LIVE");
    // Deployment name comes from the linked eval's synthesized scenario
    // (live_config.display_name).
    await waitFor(() =>
      expect(screen.getByText("BTC momentum (live)")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("live-pnl")).toHaveTextContent("+1.42%");
    // Timeline pieces mounted.
    expect(screen.getAllByTestId(/^span-row-/).length).toBeGreaterThan(0);
  });

  test("back link targets /live, not the eval-runs list", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(makeDetail());
    renderAt("/live/runs/run_live1");
    await waitFor(() =>
      expect(screen.getByTestId("topbar-back")).toHaveAttribute("href", "/live"),
    );
  });

  test("paused + flatten chips render from the run summary", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(
      makeDetail({ paused: true, flatten_requested: true }),
    );
    renderAt("/live/runs/run_live1");
    await waitFor(() =>
      expect(screen.getByTestId("live-paused-pill")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("live-flatten-pill")).toBeInTheDocument();
  });

  test("a non-live agent run renders NOT LIVE instead of dressing up as live money", async () => {
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue(
      makeDetail({
        is_live_money: false,
        eval_mode: "backtest",
        financial_eval_id: null,
      }),
    );
    renderAt("/live/runs/run_live1");
    await waitFor(() =>
      expect(screen.getByTestId("not-live-badge")).toBeInTheDocument(),
    );
    expect(screen.queryByTestId("live-badge")).not.toBeInTheDocument();
  });

  test("unknown id renders the not-found state", async () => {
    const { ApiError } = await import("@/api/client");
    vi.mocked(agentRunsApi.getAgentRun).mockRejectedValue(
      new ApiError(404, "not_found", "missing"),
    );
    renderAt("/live/runs/missing");
    await waitFor(() =>
      expect(screen.getByText(/not found/i)).toBeInTheDocument(),
    );
  });
});
