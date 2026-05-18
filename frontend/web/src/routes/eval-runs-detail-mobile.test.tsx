import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { EvalRunDetailRoute } from "./eval-runs-detail";
import * as chartApi from "@/api/chart";
import * as evalApi from "@/api/eval";
import * as evalReviewApi from "@/api/eval-review";
import * as scenariosApi from "@/api/scenarios";
import * as strategyApi from "@/api/strategies";
import type { DecisionRowDto, RunDetail } from "@/api/types.gen";

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>(
    "@/api/eval",
  );
  return {
    ...actual,
    getRun: vi.fn(),
    cancelRun: vi.fn(),
    downloadEvalRunExport: vi.fn(),
    retryRun: vi.fn(),
    listRuns: vi.fn(),
  };
});

vi.mock("@/api/eval-review", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval-review")>(
    "@/api/eval-review",
  );
  return {
    ...actual,
    listReviewsForRun: vi.fn(),
    getReview: vi.fn(),
    generateReview: vi.fn(),
  };
});

vi.mock("@/api/chart", () => ({
  chartKeys: { run: (id: string) => ["chart", "run", id] },
  getRunChart: vi.fn(),
  openRunStream: vi.fn(
    (runId: string) => new EventSource(`/stream/${runId}`),
  ),
}));

vi.mock("@/api/scenarios", async () => {
  const actual = await vi.importActual<typeof import("@/api/scenarios")>(
    "@/api/scenarios",
  );
  return {
    ...actual,
    listScenarios: vi.fn(),
  };
});

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return {
    ...actual,
    listStrategies: vi.fn(),
  };
});

function stubMatchMedia(matchesPhone: boolean) {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string) => ({
      matches: query.includes("max-width") ? matchesPhone : !matchesPhone,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

function decision(overrides: Partial<DecisionRowDto> = {}): DecisionRowDto {
  return {
    decision_index: 14,
    timestamp: "2026-05-13T14:14:14Z",
    asset: "SPY",
    action: "long_open",
    conviction: 0.82,
    justification: "Mean-reversion signal at 4218; sized to half-Kelly.",
    reasoning: null,
    order_size: 220,
    fill_price: 4218,
    fill_size: 220,
    fee: 0.4,
    pnl_realized: 2150,
    ...overrides,
  };
}

function detail(overrides: Partial<RunDetail> = {}): RunDetail {
  return {
    summary: {
      id: "01LIVE",
      agent_id: "mean-reversion-v3",
      scenario_id: "flash-crash-2024-08",
      mode: "backtest",
      status: "completed",
      started_at: "2026-05-13T14:00:00Z",
      completed_at: "2026-05-13T14:30:00Z",
      sharpe: 2.14,
      max_drawdown_pct: -2.81,
      total_return_pct: 6.42,
      actual_input_tokens: 12500,
      actual_output_tokens: 1820,
      error: null,
    },
    decisions: [decision()],
    equity_curve: [
      { timestamp: "2026-05-13T14:00:00Z", equity_usd: 10000 },
      { timestamp: "2026-05-13T14:10:00Z", equity_usd: 10250 },
      { timestamp: "2026-05-13T14:30:00Z", equity_usd: 10642 },
    ],
    ...overrides,
  };
}

function renderRoute() {
  return render(
    <MemoryRouter initialEntries={["/eval-runs/01LIVE"]}>
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <Routes>
          <Route path="/eval-runs/:runId" element={<EvalRunDetailRoute />} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("EvalRunDetailRoute (mobile layout)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(chartApi.getRunChart).mockResolvedValue(null as never);
    vi.mocked(evalReviewApi.listReviewsForRun).mockResolvedValue([]);
    vi.mocked(strategyApi.listStrategies).mockResolvedValue([
      {
        agent_id: "mean-reversion-v3",
        display_name: "Mean reversion V3",
        template: "mean_reversion",
        decision_cadence_minutes: 240,
      },
    ]);
    vi.mocked(scenariosApi.listScenarios).mockResolvedValue([
      {
        id: "flash-crash-2024-08",
        display_name: "Flash crash 2024",
      } as any,
    ]);
    vi.mocked(evalApi.cancelRun).mockResolvedValue({
      ...detail().summary,
      status: "cancelled",
      completed_at: "2026-05-13T14:01:00Z",
    });
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    stubMatchMedia(true);
  });

  afterEach(() => {
    cleanup();
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      writable: true,
      value: undefined,
    });
  });

  it("renders the LIVE strip, tab bar, and Summary tab by default on phone", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderRoute();

    // LIVE strip: COMPLETED state shows "COMPLETED" and the friendly run labels.
    expect(await screen.findByText("COMPLETED")).toBeInTheDocument();
    expect(screen.getAllByText("Mean reversion V3").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Flash crash 2024").length).toBeGreaterThan(0);
    expect(screen.getAllByText("run 01LIVE").length).toBeGreaterThan(0);

    // Tab bar: four tabs as the design specifies
    const tablist = screen.getByRole("tablist");
    for (const t of ["SUMMARY", "DECISIONS", "TRACE", "REVIEW"]) {
      expect(within(tablist).getByRole("tab", { name: t })).toBeInTheDocument();
    }
    expect(within(tablist).getByRole("tab", { name: "SUMMARY" })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    // Summary tab body: KPI labels + meta + equity card
    expect(screen.getByText("PNL")).toBeInTheDocument();
    expect(screen.getByText("MAX DD")).toBeInTheDocument();
    expect(screen.getByText("SHARPE")).toBeInTheDocument();
    expect(screen.getByText("WIN RATE")).toBeInTheDocument();
    expect(screen.getByText("META")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Equity curve" })).toBeInTheDocument();
  });

  it("switches to DECISIONS tab and renders decision cards with action pill + conviction", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderRoute();

    fireEvent.click(await screen.findByRole("tab", { name: "DECISIONS" }));

    // Step/trade counter row
    expect(screen.getByText(/1 STEPS · 1 TRADES/)).toBeInTheDocument();
    // Decision card: action pill, conviction bar, justification
    expect(screen.getByText("#14")).toBeInTheDocument();
    expect(screen.getByText("BUY")).toBeInTheDocument();
    expect(screen.getByText("82%")).toBeInTheDocument();
    expect(
      screen.getByText(/mean-reversion signal at 4218/i),
    ).toBeInTheDocument();
  });

  it("TRACE tab deep-links to the agent-run trace surface", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderRoute();

    fireEvent.click(await screen.findByRole("tab", { name: "TRACE" }));

    const link = screen.getByRole("link", { name: /view full trace/i });
    expect(link).toHaveAttribute("href", "/agent-runs/01LIVE");
  });

  it("shows HALT button + LIVE label while the run is active", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "running",
          completed_at: null,
          sharpe: null,
          total_return_pct: null,
          max_drawdown_pct: null,
        },
      }),
    );

    renderRoute();

    expect(await screen.findByText("LIVE")).toBeInTheDocument();
    const halt = screen.getByRole("button", { name: /halt eval run 01LIVE/i });
    expect(halt).toHaveTextContent(/HALT/);
    fireEvent.click(halt);
    await waitFor(() => expect(evalApi.cancelRun).toHaveBeenCalled());
    expect(vi.mocked(evalApi.cancelRun).mock.calls[0]?.[0]).toBe("01LIVE");
  });

  it("renders the disambiguator in the mobile hero and drops the duplicate strategy-id chip", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      detail().summary,
      {
        ...detail().summary,
        id: "01OLDER",
        started_at: "2026-05-13T13:00:00Z",
      },
    ]);

    renderRoute();

    const meta = await screen.findByTestId("mobile-eval-run-meta");
    await waitFor(() =>
      expect(meta.textContent ?? "").toMatch(/Run #2\/2/),
    );
    expect(meta.textContent ?? "").toMatch(/run 01LIVE/);
    // The redundant `strategy <id>` chip is gone from the mobile hero.
    expect(meta.textContent ?? "").not.toMatch(/strategy mean-reversion-v3/);
  });

  it("falls back to the desktop layout when matchMedia reports non-phone", async () => {
    stubMatchMedia(false);
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderRoute();

    expect(
      await screen.findByRole("button", {
        name: /download eval run 01LIVE as json/i,
      }),
    ).toBeInTheDocument();
    // Mobile tablist must not appear in desktop layout
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
    expect(screen.queryByText("PNL")).not.toBeInTheDocument();
  });
});
