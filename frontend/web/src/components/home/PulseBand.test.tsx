import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import type { RunSummary } from "@/api/types.gen";
import * as chartApi from "@/api/chart";
import type { LivenessCounts } from "@/features/live/strip-status";
import { PulseBand } from "./PulseBand";

vi.mock("@/api/chart", async () => {
  const actual = await vi.importActual<typeof import("@/api/chart")>("@/api/chart");
  return { ...actual, getRunChart: vi.fn() };
});

// The uPlot pane needs a real canvas; stub it for jsdom.
vi.mock("./PulseEquityChart", () => ({
  PulseEquityChart: () => <div data-testid="pulse-equity-chart" />,
}));

function run(over: Partial<RunSummary>): RunSummary {
  return {
    id: "run-1",
    agent_id: "strat-1",
    scenario_id: "scn-1",
    strategy: null,
    scenario: null,
    mode: "backtest",
    status: "completed",
    started_at: "2026-06-10T09:00:00Z",
    completed_at: "2026-06-10T10:00:00Z",
    sharpe: -5.06,
    max_drawdown_pct: 0.08,
    total_return_pct: -0.05,
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
    ...over,
  };
}

const paperLiveness: LivenessCounts = {
  liveActive: 0,
  livePaused: 0,
  paper: 2,
  stale: 1,
};

function renderBand(props: Partial<Parameters<typeof PulseBand>[0]> = {}) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <PulseBand
          runs={[run({})]}
          strategies={[]}
          liveness={paperLiveness}
          {...props}
        />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.mocked(chartApi.getRunChart).mockResolvedValue({
    run_id: "run-1",
    scenario_id: "scn-1",
    asset: "BTC",
    granularity: "1h",
    time_window: { start: 0, end: 1 },
    bars: [],
    indicators: {} as never,
    equity: [
      { time: 1, equity_usd: 100_000 },
      { time: 2, equity_usd: 100_100 },
      { time: 3, equity_usd: 99_900 },
    ],
    drawdown: [],
    position: [],
    markers: { trades: [], vetoes: [], holds: [] },
  } as never);
});

describe("PulseBand", () => {
  it("renders KPI numerals from the hero run with honest tones", async () => {
    renderBand();

    expect(await screen.findByTestId("pulse-kpi-return")).toHaveTextContent(
      "-0.05%",
    );
    expect(screen.getByTestId("pulse-kpi-return").className).toContain(
      "text-danger",
    );
    expect(screen.getByTestId("pulse-kpi-drawdown")).toHaveTextContent("0.08%");
    expect(screen.getByTestId("pulse-kpi-sharpe")).toHaveTextContent("-5.06");
    expect(screen.getByTestId("pulse-kpi-evals")).toHaveTextContent("1");
  });

  it("renders the equity chart once the payload arrives", async () => {
    renderBand();
    expect(await screen.findByTestId("pulse-equity-chart")).toBeInTheDocument();
  });

  it("shows the honest paper chip when nothing is live-money", async () => {
    renderBand();
    const chip = await screen.findByTestId("execution-chip");
    expect(chip).toHaveTextContent(/paper · no live capital deployed/i);
  });

  it("shows a glowing live chip when live-money runs exist", async () => {
    renderBand({
      liveness: { liveActive: 2, livePaused: 1, paper: 0, stale: 0 },
    });
    const chip = await screen.findByTestId("execution-chip");
    expect(chip).toHaveTextContent(/live money · 3/i);
    expect(chip.className).toContain("xvn-live-glow");
  });

  it("renders the designed empty state when no chartable completed run exists", () => {
    renderBand({ runs: [run({ status: "running" })] });
    expect(screen.getByText(/no completed evals yet/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /start eval/i })).toHaveAttribute(
      "href",
      "/eval-runs?start=1",
    );
    // No KPI rail of em-dashes in the empty state.
    expect(screen.queryByTestId("pulse-kpi-return")).toBeNull();
  });

  it("renders a designed fallback when the run has no equity samples", async () => {
    vi.mocked(chartApi.getRunChart).mockResolvedValue({
      equity: [],
      drawdown: [],
      bars: [],
      position: [],
      markers: { trades: [], vetoes: [], holds: [] },
    } as never);
    renderBand();
    expect(
      await screen.findByTestId("pulse-chart-unavailable"),
    ).toHaveTextContent(/no equity samples recorded/i);
  });

  it("stamps freshness from the latest completed run", async () => {
    renderBand();
    expect(await screen.findByTestId("pulse-freshness")).toHaveTextContent(
      /updated .*ago|updated just now/i,
    );
  });
});
