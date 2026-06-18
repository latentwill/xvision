import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import type {
  MutatorScore,
  StatsRow,
  StatusResponse,
} from "@/features/autooptimizer/api";
import {
  useLadder,
  useOptimizerStats,
  useOptimizerStatus,
  useSessionList,
} from "@/features/autooptimizer/api";
import { useAutoresearchRuns } from "@/api/nanochat";
import { OptimizerPanel } from "./OptimizerPanel";

vi.mock("@/features/autooptimizer/api", () => ({
  useLadder: vi.fn(),
  useOptimizerStats: vi.fn(),
  useOptimizerStatus: vi.fn(),
  useSessionList: vi.fn(),
}));

vi.mock("@/api/nanochat", () => ({
  useAutoresearchRuns: vi.fn(),
}));

// 8wn: the nested OptimizerDigestStrip now pulls the operator budget cap via a
// real useQuery(getCostBudget). Stub the fetcher (UNSET cap) so the panel's
// pure-render tests stay network-free; the digest itself returns null here
// (empty session list) but the hook still runs.
vi.mock("@/api/cost", () => ({
  costKeys: { all: ["cost"], budget: () => ["cost", "budget"] },
  getCostBudget: vi.fn().mockResolvedValue({ daily_cap_usd: null }),
}));

const LADDER: MutatorScore[] = [
  {
    provider: "openrouter",
    model: "google/gemini-3.1-flash-lite",
    prompt_version: "v1",
    proposals: 5,
    accepted: 2,
    rejected_overfit: 3,
    avg_delta_sharpe: 0.54,
  },
  {
    provider: "ollama",
    model: "lfm2.5:8b",
    prompt_version: "v1",
    proposals: 8,
    accepted: 0,
    rejected_overfit: 8,
    avg_delta_sharpe: 0,
  },
];

const STATS: StatsRow[] = [
  {
    cycle_id: "c1",
    session_id: "s1",
    ts: "2026-06-10T09:00:00Z",
    kept: 1,
    suspect: 1,
    dropped: 2,
    best_delta_holdout: null,
    cost_usd: 0.2,
    cum_cost_usd: 0.2,
  },
  {
    cycle_id: "c2",
    session_id: "s1",
    ts: "2026-06-10T10:00:00Z",
    kept: 0,
    suspect: 0,
    dropped: 2,
    best_delta_holdout: null,
    cost_usd: 0.43,
    cum_cost_usd: 0.63,
  },
];

function setHooks({
  ladder = LADDER,
  stats = STATS,
  status = undefined,
  ladderPending = false,
  statsPending = false,
}: {
  ladder?: MutatorScore[] | undefined;
  stats?: StatsRow[] | undefined;
  status?: StatusResponse | undefined;
  ladderPending?: boolean;
  statsPending?: boolean;
} = {}) {
  vi.mocked(useLadder).mockReturnValue({
    data: ladder,
    isPending: ladderPending,
  } as unknown as ReturnType<typeof useLadder>);
  vi.mocked(useOptimizerStats).mockReturnValue({
    data: stats,
    isPending: statsPending,
  } as unknown as ReturnType<typeof useOptimizerStats>);
  vi.mocked(useOptimizerStatus).mockReturnValue(status);
  vi.mocked(useSessionList).mockReturnValue({
    data: [],
  } as unknown as ReturnType<typeof useSessionList>);
  vi.mocked(useAutoresearchRuns).mockReturnValue({
    data: [],
    isPending: false,
  } as unknown as ReturnType<typeof useAutoresearchRuns>);
}

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <OptimizerPanel />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("OptimizerPanel", () => {
  it("renders the accepted/rejected experiments meter with totals", () => {
    setHooks();
    renderPanel();
    const meter = screen.getByTestId("optimizer-acceptance");
    expect(meter).toHaveTextContent("2/13");
    expect(meter).toHaveTextContent("11 rejected (overfit)");
  });

  it("renders the writer mini-ladder ranked by avg ΔSharpe with green positive delta", () => {
    setHooks();
    renderPanel();
    const ladder = screen.getByTestId("optimizer-ladder");
    const rows = ladder.querySelectorAll("li");
    expect(rows[0]).toHaveTextContent("gemini-3.1-flash-lite");
    expect(rows[0]).toHaveTextContent("2/5");
    expect(rows[0]).toHaveTextContent("+0.54");
    expect(rows[0].querySelector(".text-gold")).not.toBeNull();
    expect(rows[1]).toHaveTextContent("lfm2.5:8b");
  });

  it("renders cycle trend bars and cumulative spend", () => {
    setHooks();
    renderPanel();
    expect(
      screen.getByTestId("optimizer-cycle-trend").children.length,
    ).toBe(2);
    expect(screen.getByTestId("optimizer-spend")).toHaveTextContent("$0.63");
  });

  it("shows an honest idle line naming the last cycle — never 'Waiting for connection'", () => {
    setHooks();
    renderPanel();
    const idle = screen.getByTestId("optimizer-idle");
    expect(idle).toHaveTextContent(/idle · last cycle/i);
    expect(idle).toHaveTextContent(/0 kept · 0 suspect · 2 dropped/i);
    expect(screen.queryByText(/waiting for connection/i)).toBeNull();
  });

  it("shows a running pill when a session is active", () => {
    setHooks({
      status: {
        active_session: {
          session_id: "s1",
          strategy_id: "strat",
          state: "running",
          mode: "explore",
          cycles_completed: 4,
          kept_count: 1,
          suspect_count: 0,
          dropped_count: 3,
        },
        last_event_seq: 9,
      },
    });
    renderPanel();
    expect(screen.getByTestId("optimizer-status-pill")).toHaveTextContent(
      /running · 4 experiments/i,
    );
  });

  it("renders the designed empty state when no data exists", () => {
    setHooks({ ladder: [], stats: [] });
    renderPanel();
    expect(screen.getByTestId("optimizer-empty")).toHaveTextContent(
      /no optimizer cycles recorded yet/i,
    );
  });

  it("links to the Optimizer page", () => {
    setHooks();
    renderPanel();
    expect(
      screen.getByRole("link", { name: /open optimizer/i }),
    ).toHaveAttribute("href", "/optimizer");
  });
});
