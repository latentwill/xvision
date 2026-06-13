import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { ExperimentDetail } from "./ExperimentDetail";
import * as api from "../api";
import type {
  ExperimentDetailResponse,
  GateRecord,
  ExperimentFinding,
  RegimeResult,
  RegimeMetrics,
} from "../api";

vi.mock("../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api")>();
  return { ...actual, useExperimentDetail: vi.fn() };
});

const metrics = (over: Partial<RegimeMetrics> = {}): RegimeMetrics => ({
  total_return_pct: 4.2,
  sharpe: 1.1,
  max_drawdown_pct: -3.5,
  win_rate: 0.62,
  n_trades: 18,
  ...over,
});

const FULL_GATE: GateRecord = {
  bundle_hash: "abc123def456",
  parent_day_score: 1.1,
  child_day_score: 1.42,
  parent_holdout_score: 0.9,
  child_holdout_score: 1.05,
  gate_epsilon: 0.05,
  delta_day: 0.32,
  delta_holdout: 0.15,
  drawdown_ratio: 0.88,
  verdict: "passed",
  reason: "Improvement exceeds the min-improvement threshold on both windows.",
};

const FINDINGS: ExperimentFinding[] = [
  {
    id: 1,
    bundle_hash: "abc123def456",
    severity: "info",
    code: "EDGE_CONFIRMED",
    summary: "Candidate beats parent on the holdout window",
    detail: "The candidate's holdout Sharpe is materially higher than the parent's.",
    model: "claude-judge",
  },
  {
    id: 2,
    bundle_hash: "abc123def456",
    severity: "risk",
    code: "DRAWDOWN_WATCH",
    summary: "Drawdown crept up in the bear regime",
    detail: null,
    model: null,
  },
];

const REGIMES: RegimeResult[] = [
  {
    regime_label: "Bull market",
    side: "bull",
    delta_sharpe: 0.4,
    verdict: "passed",
    metrics_day: metrics({ total_return_pct: 6.1 }),
    metrics_untouched: metrics(),
  },
  {
    regime_label: "Chop regime",
    side: "chop",
    delta_sharpe: -0.1,
    verdict: "failed",
    metrics_day: metrics({ total_return_pct: -1.2 }),
    metrics_untouched: metrics(),
  },
];

const FULL_DETAIL: ExperimentDetailResponse = {
  lineage_node: {
    bundle_hash: "abc123def456",
    parent_hash: "parent0099",
    gate_verdict: "Pass",
    status: "active",
    cycle_id: "cyc-77",
    created_at: "2026-06-10T00:00:00Z",
  },
  rationale:
    "The parent over-traded in chop; this experiment tightens the entry filter to cut whipsaw.",
  gate_record: FULL_GATE,
  findings: FINDINGS,
  regime_results: REGIMES,
};

type UseExperimentDetailReturn = ReturnType<typeof api.useExperimentDetail>;

function mockDetail(over: Partial<UseExperimentDetailReturn>) {
  vi.mocked(api.useExperimentDetail).mockReturnValue({
    data: undefined,
    isLoading: false,
    isError: false,
    ...over,
  } as UseExperimentDetailReturn);
}

const wrap = ({ children }: { children: React.ReactNode }) => (
  <QueryClientProvider
    client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
  >
    <MemoryRouter initialEntries={["/optimizer/experiment/abc123def456"]}>
      <Routes>
        <Route path="/optimizer/experiment/:hash" element={<>{children}</>} />
        <Route path="/optimizer" element={<div>optimizer-home</div>} />
      </Routes>
    </MemoryRouter>
  </QueryClientProvider>
);

describe("ExperimentDetail", () => {
  beforeEach(() => vi.clearAllMocks());

  it("renders all five sections with a representative datum from each", () => {
    mockDetail({ data: FULL_DETAIL });
    render(<ExperimentDetail />, { wrapper: wrap });

    // 1) WHY TESTED
    expect(
      screen.getByRole("heading", { name: /why (this was )?tested/i }),
    ).toBeTruthy();
    expect(screen.getByText(/tightens the entry filter/i)).toBeTruthy();

    // 2) WHAT HAPPENED — per-regime evaluation
    expect(
      screen.getByRole("heading", { name: /what happened/i }),
    ).toBeTruthy();
    expect(screen.getByText("Bull market")).toBeTruthy();

    // 3) GATE SCORECARD
    expect(
      screen.getByRole("heading", { name: /gate scorecard/i }),
    ).toBeTruthy();
    // ScoreBar renders the +delta value
    expect(screen.getByText("+0.32")).toBeTruthy();

    // 4) DECISION — gate verdict badge
    expect(screen.getByRole("heading", { name: /^decision$/i })).toBeTruthy();
    // formatGateVerdict("Pass") → "Accepted" → GateBadge bucket "Kept"
    // (rendered twice: hero + decision section)
    expect(screen.getAllByText("Kept").length).toBeGreaterThanOrEqual(1);

    // 5) REVIEWER NOTES — findings
    expect(
      screen.getByRole("heading", { name: /^reviewer notes$/i }),
    ).toBeTruthy();
    expect(screen.getByText("EDGE_CONFIRMED")).toBeTruthy();
    expect(screen.getByText("DRAWDOWN_WATCH")).toBeTruthy();
  });

  it("renders all five sections and graceful fallbacks when data is missing", () => {
    mockDetail({
      data: {
        lineage_node: {
          bundle_hash: "abc123def456",
          parent_hash: null,
          gate_verdict: null,
          status: "active",
          cycle_id: null,
          created_at: "2026-06-10T00:00:00Z",
        },
        rationale: null,
        gate_record: null,
        findings: [],
        regime_results: [],
      },
    });
    render(<ExperimentDetail />, { wrapper: wrap });

    // All five headings still present
    expect(
      screen.getByRole("heading", { name: /why (this was )?tested/i }),
    ).toBeTruthy();
    expect(
      screen.getByRole("heading", { name: /what happened/i }),
    ).toBeTruthy();
    expect(
      screen.getByRole("heading", { name: /gate scorecard/i }),
    ).toBeTruthy();
    expect(screen.getByRole("heading", { name: /^decision$/i })).toBeTruthy();
    expect(
      screen.getByRole("heading", { name: /^reviewer notes$/i }),
    ).toBeTruthy();

    // Fallback copy from children + the why-tested fallback
    expect(screen.getByText(/no rationale recorded/i)).toBeTruthy();
    expect(screen.getByText(/gate data not recorded/i)).toBeTruthy();
    expect(screen.getByText(/no reviewer notes for this experiment/i)).toBeTruthy();
  });

  it("shows a loading state without crashing", () => {
    mockDetail({ data: undefined, isLoading: true });
    render(<ExperimentDetail />, { wrapper: wrap });
    expect(screen.getByText(/loading/i)).toBeTruthy();
  });

  it("shows a friendly error state when the fetch fails", () => {
    mockDetail({ data: undefined, isError: true });
    render(<ExperimentDetail />, { wrapper: wrap });
    expect(screen.getByText(/couldn.t load|couldn't load/i)).toBeTruthy();
  });
});
