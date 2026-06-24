import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import type { RunSummary } from "@/api/types.gen";
import * as chartApi from "@/api/chart";
import type { LivenessCounts } from "@/features/live/strip-status";
import { PulseBand } from "./PulseBand";

vi.mock("@/api/chart", async () => {
  const actual = await vi.importActual<typeof import("@/api/chart")>("@/api/chart");
  return { ...actual, getRunChart: vi.fn(), getCompareChart: vi.fn() };
});

// The uPlot pane needs a real canvas; stub it for jsdom.
vi.mock("./PulseEquityChart", () => ({
  PulseEquityChart: () => <div data-testid="pulse-equity-chart" />,
}));

// Stub the view-specific chart components — they have canvas/uPlot deps.
vi.mock("./views/PulseDrawdownChart", () => ({
  PulseDrawdownChart: () => <div data-testid="pulse-drawdown-chart" />,
}));
vi.mock("./views/PulseTradesChart", () => ({
  PulseTradesChart: () => <div data-testid="pulse-trades-chart" />,
}));
vi.mock("./views/PulseHoldChart", () => ({
  PulseHoldChart: () => <div data-testid="pulse-hold-chart" />,
}));
vi.mock("./views/PulseFieldChart", () => ({
  PulseFieldChart: () => <div data-testid="pulse-field-chart" />,
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
    unrealized_pnl_usd: null,
    skipped_dispatches: 0,
    delayed_decisions: 0,
    forced_cancels: 0,
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

afterEach(() => {
  localStorage.clear();
});

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

  it("shows the central no-live-capital chip when nothing is live-money", async () => {
    renderBand();
    const chip = await screen.findByTestId("execution-chip");
    expect(chip).toHaveTextContent(/no live capital · paper\/sim only/i);
  });

  it("shows a glowing live chip when live-money runs exist", async () => {
    renderBand({
      liveness: { liveActive: 2, livePaused: 1, paper: 0, stale: 0 },
    });
    const chip = await screen.findByTestId("execution-chip");
    expect(chip).toHaveTextContent(/live capital deployed · 3/i);
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

  it("lets the main graph switch among the five latest evaluated strategies", async () => {
    const user = userEvent.setup();
    renderBand({
      runs: [
        run({ id: "run-alpha", agent_id: "alpha", completed_at: "2026-06-10T09:00:00Z" }),
        run({ id: "run-beta", agent_id: "beta", completed_at: "2026-06-10T08:00:00Z" }),
        run({ id: "run-gamma", agent_id: "gamma", completed_at: "2026-06-10T07:00:00Z" }),
        run({ id: "run-delta", agent_id: "delta", completed_at: "2026-06-10T06:00:00Z" }),
        run({ id: "run-epsilon", agent_id: "epsilon", completed_at: "2026-06-10T05:00:00Z" }),
        run({ id: "run-zeta", agent_id: "zeta", completed_at: "2026-06-10T04:00:00Z" }),
      ],
      strategies: [
        { agent_id: "alpha", display_name: "Alpha", template: "blank", decision_cadence_minutes: 60 },
        { agent_id: "beta", display_name: "Beta", template: "blank", decision_cadence_minutes: 60 },
        { agent_id: "gamma", display_name: "Gamma", template: "blank", decision_cadence_minutes: 60 },
        { agent_id: "delta", display_name: "Delta", template: "blank", decision_cadence_minutes: 60 },
        { agent_id: "epsilon", display_name: "Epsilon", template: "blank", decision_cadence_minutes: 60 },
        { agent_id: "zeta", display_name: "Zeta", template: "blank", decision_cadence_minutes: 60 },
      ],
    });

    await waitFor(() => {
      expect(chartApi.getRunChart).toHaveBeenCalledWith("run-alpha", [
        "equity",
        "markers",
      ]);
    });

    const selector = screen.getByTestId("pulse-strategy-selector");
    expect(selector.querySelectorAll("button")).toHaveLength(5);
    expect(screen.queryByRole("button", { name: /zeta/i })).toBeNull();

    await user.click(screen.getByRole("button", { name: /beta/i }));

    await waitFor(() => {
      expect(chartApi.getRunChart).toHaveBeenCalledWith("run-beta", [
        "equity",
        "markers",
      ]);
    });
    expect(screen.getByRole("button", { name: /beta/i })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("renders the view switcher and persists the selection", async () => {
    const user = userEvent.setup();
    renderBand();

    // Wait for the band to finish loading so the switcher appears.
    await screen.findByTestId("pulse-equity-chart");

    const switcher = screen.getByTestId("pulse-view-switcher");
    expect(switcher).toBeInTheDocument();

    // Click the "Drawdown" chip.
    await user.click(screen.getByRole("button", { name: /drawdown/i }));

    // localStorage should be updated.
    expect(localStorage.getItem("xvn:pulse-view")).toBe("drawdown");

    // Drawdown chart should appear.
    expect(await screen.findByTestId("pulse-drawdown-chart")).toBeInTheDocument();
  });

  it("initial view comes from localStorage", async () => {
    localStorage.setItem("xvn:pulse-view", "drawdown");
    renderBand();

    // The drawdown chip should start as pressed.
    await waitFor(() => {
      const btn = screen.getByRole("button", { name: /drawdown/i });
      expect(btn).toHaveAttribute("aria-pressed", "true");
    });
  });

  it("failed lazy view fetch shows an inline retry, not a crash", async () => {
    const user = userEvent.setup();

    // The default equity fetch resolves fine; the bars+markers fetch rejects.
    vi.mocked(chartApi.getRunChart).mockImplementation(
      (_id: string, include?: string[]) => {
        if (include && include.includes("bars")) {
          return Promise.reject(new Error("network error"));
        }
        return Promise.resolve({
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
      },
    );

    renderBand();

    // Wait for the switcher to appear.
    await screen.findByTestId("pulse-equity-chart");

    // Click "Price + trades".
    await user.click(screen.getByRole("button", { name: /price \+ trades/i }));

    // Error UI should appear with a Retry button.
    expect(await screen.findByTestId("pulse-view-error")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /retry/i })).toBeInTheDocument();
  });
});
