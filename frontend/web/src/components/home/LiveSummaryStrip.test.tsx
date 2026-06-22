import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import * as agentRunsApi from "@/api/agent-runs";
import { MOCK_RUN_LIVE } from "@/features/agent-runs/mock-fixtures";
import type { AgentRunSummary } from "@/api/types-agent-runs";
import { LiveSummaryStrip } from "./LiveSummaryStrip";

vi.mock("@/api/agent-runs", async () => {
  const actual = await vi.importActual<typeof import("@/api/agent-runs")>(
    "@/api/agent-runs",
  );
  return {
    ...actual,
    listAgentRuns: vi.fn(),
  };
});

function renderStrip() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <LiveSummaryStrip />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

// MOCK_RUN_LIVE carries the live-money discriminator (is_live_money: true,
// parent eval run live + running), so `live()` is a GENUINE live run.
const live = (over: Partial<AgentRunSummary> = {}): AgentRunSummary => ({
  ...MOCK_RUN_LIVE.summary,
  status: "running",
  ...over,
});

// A paper/sim run: running, but no live-money signal (backtest child or
// parentless agent run).
const paper = (over: Partial<AgentRunSummary> = {}): AgentRunSummary => ({
  ...MOCK_RUN_LIVE.summary,
  status: "running",
  eval_mode: "backtest",
  eval_run_status: "running",
  is_live_money: false,
  ...over,
});

// A stale orphan: agent run stuck in `running` while the parent eval run is
// long terminal (the xvision-9pi shape).
const stale = (over: Partial<AgentRunSummary> = {}): AgentRunSummary => ({
  ...MOCK_RUN_LIVE.summary,
  status: "running",
  eval_mode: "live",
  eval_run_status: "failed",
  is_live_money: false,
  ...over,
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("LiveSummaryStrip", () => {
  it("shows the empty state + 'Launch eval' CTA when there are no live runs", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([]);

    renderStrip();

    await screen.findByText(/no live strategies running/i);
    const cta = screen.getByRole("link", { name: /launch eval/i });
    expect(cta).toHaveAttribute("href", "/eval-runs?start=1");
  });

  it("counts ACTIVE live-money strategies and links to /live", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([
      live({ run_id: "a" }),
      live({ run_id: "b" }),
    ]);

    renderStrip();

    const liveCount = await screen.findByTestId("live-count");
    expect(liveCount).toHaveTextContent("2");
    expect(liveCount).toHaveTextContent(/live/i);

    const cta = screen.getByRole("link", { name: /go to live trading/i });
    expect(cta).toHaveAttribute("href", "/live");
  });

  it("separates ACTIVE from PAUSED counts", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([
      live({ run_id: "a" }),
      live({ run_id: "p", paused: true }),
    ]);

    renderStrip();

    // One active, one paused.
    const liveCount = await screen.findByTestId("live-count");
    expect(liveCount).toHaveTextContent("1");
    expect(screen.getByTestId("paused-count")).toHaveTextContent("1");
  });

  it("does NOT render a paused count when none are paused", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([live({ run_id: "a" })]);

    renderStrip();

    await screen.findByTestId("live-count");
    expect(screen.queryByTestId("paused-count")).toBeNull();
  });

  it("excludes terminal (completed) runs from the live counts", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([
      live({ run_id: "a" }),
      live({ run_id: "done", status: "completed" }),
    ]);

    renderStrip();

    // Only one live run → "1 live", and the strip is in live (not empty) mode.
    const liveCount = await screen.findByTestId("live-count");
    expect(liveCount).toHaveTextContent("1");
    expect(screen.queryByText(/no live strategies running/i)).toBeNull();
  });

  it("counts non-live running rows separately — they are NOT live", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([
      live({ run_id: "a" }),
      paper({ run_id: "bt1" }),
      paper({ run_id: "bt2" }),
    ]);

    renderStrip();

    const liveCount = await screen.findByTestId("live-count");
    expect(liveCount).toHaveTextContent("1");
    const nonLiveCount = screen.getByTestId("non-live-count");
    expect(nonLiveCount).toHaveTextContent("2");
    expect(nonLiveCount).toHaveTextContent(/non-live/i);
    expect(nonLiveCount).not.toHaveTextContent(/paper/i);
  });

  it("renders stale orphans as stale — never live (xvision-9pi)", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([
      stale({ run_id: "s1" }),
      stale({ run_id: "s2" }),
      stale({ run_id: "s3" }),
    ]);

    renderStrip();

    const staleCount = await screen.findByTestId("stale-count");
    expect(staleCount).toHaveTextContent("3");
    expect(staleCount).toHaveTextContent(/stale/i);
    // Zero live money — no live count rendered, no fake "active" claims.
    expect(screen.queryByTestId("live-count")).toBeNull();
    expect(screen.queryByText(/no live strategies running/i)).toBeNull();
  });

  it("runs with running status but NO live-money signal never count as live", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([
      // Defensive: even `status: "running"` + absent discriminator fields
      // (older backend) must not be counted as live money.
      paper({
        run_id: "legacy",
        eval_mode: undefined,
        eval_run_status: undefined,
        is_live_money: undefined,
      }),
    ]);

    renderStrip();

    const nonLiveCount = await screen.findByTestId("non-live-count");
    expect(nonLiveCount).toHaveTextContent("1");
    expect(nonLiveCount).toHaveTextContent(/non-live/i);
    expect(screen.queryByTestId("live-count")).toBeNull();
  });

  it("fetches only non-terminal runs (status=running,queued) with a wide limit so counts are honest", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([]);

    renderStrip();

    await screen.findByText(/no live strategies running/i);
    // Counting over the newest-20-of-any-status window undercounts when
    // many runs are terminal; the strip must scope the query to the
    // non-terminal population (which is all live/paper/stale ever are).
    expect(agentRunsApi.listAgentRuns).toHaveBeenCalledWith({
      status: "running,queued",
      limit: 100,
    });
  });

  it("renders a loading indicator while pending and stays in the DOM", () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockReturnValue(new Promise(() => {}));

    renderStrip();

    expect(screen.getByTestId("live-summary-strip")).toBeInTheDocument();
    expect(screen.getByTestId("live-summary-loading")).toBeInTheDocument();
  });

  it("ALWAYS renders the strip + 'Live trading' label, even when empty", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([]);

    renderStrip();

    await screen.findByText(/no live strategies running/i);
    expect(screen.getByTestId("live-summary-strip")).toBeInTheDocument();
    expect(screen.getByText(/live trading/i)).toBeInTheDocument();
    expect(screen.queryByText(/real money/i)).toBeNull();
    expect(screen.queryByTestId("paper-count")).toBeNull();
  });

  // n0k honesty annotation: the live count must carry "· simulated" so the label
  // does not suggest real money when all runs are paper/testnet.
  it("qualifies the live count as simulated (honesty annotation)", async () => {
    vi.mocked(agentRunsApi.listAgentRuns).mockResolvedValue([
      live({ run_id: "a" }),
      live({ run_id: "b" }),
    ]);

    renderStrip();

    const liveCount = await screen.findByTestId("live-count");
    // The live-count span must include the simulated qualifier
    expect(liveCount.textContent).toMatch(/simulated/i);
  });
});
