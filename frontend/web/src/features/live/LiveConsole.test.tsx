import { describe, expect, test, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import type { AgentRunSummary } from "@/api/types-agent-runs";
import { LiveConsole } from "./LiveConsole";

// Mock the agent-runs API (list polling) so tests are isolated.
vi.mock("@/api/agent-runs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/api/agent-runs")>();
  return { ...actual, listAgentRuns: vi.fn() };
});

// Heavy chart surface (canvas/WebGL) — stub it.
vi.mock("@/components/chart/v2/surfaces/LiveChartV2Container", () => ({
  LiveChartV2Container: ({ runId }: { runId: string }) => (
    <div data-testid="live-chart-stub">{runId}</div>
  ),
}));

// The SSE hook — no network in jsdom.
vi.mock("@/components/chart/use-run-stream", () => ({
  useRunStream: () => ({ data: undefined, status: "snapshot" }),
}));

// Wallet — controllable per test.
let mockAddress: string | null = "0xabc";
vi.mock("@/features/marketplace/lib/wallet", () => ({
  useWallet: () => ({
    address: mockAddress,
    connecting: false,
    connect: vi.fn(),
    disconnect: vi.fn(),
  }),
}));

import { listAgentRuns } from "@/api/agent-runs";

function mkRun(over: Partial<AgentRunSummary> = {}): AgentRunSummary {
  return {
    run_id: "run_aaaa1111",
    objective: "BTC Momentum",
    strategy_id: "strat_1",
    agent_id: null,
    started_at: "2026-06-09T10:00:00Z",
    finished_at: null,
    status: "running",
    span_count: 0,
    model_call_count: 5,
    tool_call_count: 3,
    error_count: 0,
    total_cost_usd: 0,
    total_input_tokens: 0,
    total_output_tokens: 0,
    duration_ms: null,
    financial_eval_id: null,
    retention_mode: "hash_only",
    ...over,
  };
}

function renderConsole(path: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/live" element={<LiveConsole />} />
          <Route path="/live/:id" element={<ConsoleWithParam />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

// Tiny adapter so the :id param flows into the console, mirroring LiveRoute.
import { useParams } from "react-router-dom";
function ConsoleWithParam() {
  const { id } = useParams();
  return <LiveConsole runId={id || undefined} />;
}

beforeEach(() => {
  mockAddress = "0xabc";
  localStorage.clear();
});
afterEach(() => vi.restoreAllMocks());

describe("LiveConsole", () => {
  test("renders Live Trading title + strategy strip", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([mkRun()]);
    renderConsole("/live");
    await waitFor(() =>
      expect(screen.getByText("Live Trading")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("strategy-strip")).toBeInTheDocument();
    expect(screen.getByText("Deploy strategy →")).toBeInTheDocument();
  });

  test("/live auto-selects most recently started live run into viewport", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([
      mkRun({ run_id: "old", started_at: "2026-06-09T08:00:00Z" }),
      mkRun({ run_id: "newest", started_at: "2026-06-09T12:00:00Z" }),
    ]);
    renderConsole("/live");
    await waitFor(() =>
      expect(screen.getByTestId("live-chart-stub")).toHaveTextContent("newest"),
    );
  });

  test("/live/:id preselects the given run", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([
      mkRun({ run_id: "run_a", started_at: "2026-06-09T08:00:00Z" }),
      mkRun({ run_id: "run_b", started_at: "2026-06-09T12:00:00Z" }),
    ]);
    renderConsole("/live/run_a");
    await waitFor(() =>
      expect(screen.getByTestId("live-chart-stub")).toHaveTextContent("run_a"),
    );
  });

  test("wallet banner shown only when address is null", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([mkRun()]);
    mockAddress = null;
    renderConsole("/live");
    await waitFor(() =>
      expect(screen.getByTestId("wallet-banner")).toBeInTheDocument(),
    );
    expect(
      screen.getByText(/Wallet not connected/i),
    ).toBeInTheDocument();
  });

  test("wallet banner hidden when connected", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([mkRun()]);
    mockAddress = "0xabc";
    renderConsole("/live");
    await waitFor(() =>
      expect(screen.getByTestId("strategy-strip")).toBeInTheDocument(),
    );
    expect(screen.queryByTestId("wallet-banner")).not.toBeInTheDocument();
  });

  test("empty list shows no-deployments state", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([]);
    renderConsole("/live");
    await waitFor(() =>
      expect(
        screen.getByText(/No active live deployments/i),
      ).toBeInTheDocument(),
    );
  });

  test("B-II slot seam is present under the chart", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([mkRun()]);
    renderConsole("/live");
    await waitFor(() =>
      expect(
        screen.getByTestId("live-stats-positions-slot"),
      ).toBeInTheDocument(),
    );
  });

  test("ArenaStandingIndicator renders inline when a run is selected", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([mkRun()]);
    renderConsole("/live");
    await waitFor(() =>
      expect(screen.getByTestId("arena-standing-indicator")).toBeInTheDocument(),
    );
    // Chips are present (conservative defaults: not-active state)
    expect(screen.getByTestId("chip-trading-via-arena")).toBeInTheDocument();
    expect(screen.getByTestId("chip-ai-pot-in-view")).toBeInTheDocument();
  });

  test("ArenaStandingIndicator is absent when no run is selected (empty state)", async () => {
    vi.mocked(listAgentRuns).mockResolvedValue([]);
    renderConsole("/live");
    await waitFor(() =>
      expect(
        screen.getByText(/No active live deployments/i),
      ).toBeInTheDocument(),
    );
    expect(
      screen.queryByTestId("arena-standing-indicator"),
    ).not.toBeInTheDocument();
  });
});
