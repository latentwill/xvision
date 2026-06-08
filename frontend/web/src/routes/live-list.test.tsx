// frontend/web/src/routes/live-list.test.tsx
import { describe, expect, test, vi, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { LiveListRoute } from "./live-list";
import { LiveRoute } from "./live";
import type { AgentRunSummary } from "@/api/types-agent-runs";

// Mock the agent-runs API so tests are fully isolated.
vi.mock("@/api/agent-runs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/api/agent-runs")>();
  return {
    ...actual,
    listAgentRuns: vi.fn(),
  };
});

// Mock live.tsx's heavy LiveChartV2Container (requires canvas + WebGL)
vi.mock("@/components/chart/v2/surfaces/LiveChartV2Container", () => ({
  LiveChartV2Container: ({ runId }: { runId: string }) => (
    <div data-testid="live-chart-stub">{runId}</div>
  ),
}));

import { listAgentRuns } from "@/api/agent-runs";

const MOCK_SUMMARY: AgentRunSummary = {
  run_id: "run_live5678abcd",
  objective: "Trade BTC/USD live",
  strategy_id: "strat_abc",
  agent_id: null,
  started_at: "2026-06-07T10:00:00Z",
  finished_at: null,
  status: "running",
  span_count: 3,
  model_call_count: 1,
  tool_call_count: 2,
  error_count: 0,
  total_cost_usd: 0.002,
  total_input_tokens: 200,
  total_output_tokens: 100,
  duration_ms: null,
  financial_eval_id: null,
  retention_mode: "hash_only",
};

function renderLiveList() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/live"]}>
        <Routes>
          <Route path="/live" element={<LiveListRoute />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function renderWithBothRoutes(path: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/live" element={<LiveListRoute />} />
          <Route path="/live/:id" element={<LiveRoute />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("LiveListRoute", () => {
  test("Test 1: renders list of runs — shows truncated run_id and status", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([MOCK_SUMMARY]);
    renderLiveList();

    // Should show first 8 chars of run_id
    await waitFor(() =>
      expect(screen.getByText("run_live")).toBeInTheDocument(),
    );
    // Should show status
    expect(screen.getByText("running")).toBeInTheDocument();
  });

  test("Test 2: empty state shows 'No active live deployments'", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([]);
    renderLiveList();

    await waitFor(() =>
      expect(
        screen.getByText(/No active live deployments/i),
      ).toBeInTheDocument(),
    );
  });

  test("Test 3: route title shows 'Live strategies'", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([]);
    renderLiveList();

    await waitFor(() =>
      expect(screen.getByText("Live strategies")).toBeInTheDocument(),
    );
  });

  test("Test 4: each run row links to /live/:run_id", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([MOCK_SUMMARY]);
    renderLiveList();

    await waitFor(() =>
      expect(screen.getByText("run_live")).toBeInTheDocument(),
    );
    const link = screen.getByRole("link", { name: /run_live/i });
    expect(link).toHaveAttribute("href", `/live/${MOCK_SUMMARY.run_id}`);
  });

  test("Test 5: /live/:id cockpit route still renders correctly (regression)", async () => {
    // Ensure the LiveRoute (cockpit) works alongside LiveListRoute.
    renderWithBothRoutes("/live/run_live5678");

    // The cockpit Topbar renders "Live cockpit"
    await waitFor(() =>
      expect(screen.getByText("Live cockpit")).toBeInTheDocument(),
    );
    // And sub-title shows the run id (as the Topbar sub prop)
    expect(screen.getAllByText("run_live5678").length).toBeGreaterThan(0);
  });
});
