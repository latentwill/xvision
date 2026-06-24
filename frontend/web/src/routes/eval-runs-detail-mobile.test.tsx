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
import * as agentsApi from "@/api/agents";
import * as agentRunsApi from "@/api/agent-runs";
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
    getStrategy: vi.fn(),
  };
});

vi.mock("@/api/agents", async () => {
  const actual = await vi.importActual<typeof import("@/api/agents")>(
    "@/api/agents",
  );
  return {
    ...actual,
    listAgents: vi.fn(),
  };
});

vi.mock("@/api/agent-runs", async () => {
  const actual = await vi.importActual<typeof import("@/api/agent-runs")>(
    "@/api/agent-runs",
  );
  return {
    ...actual,
    getAgentRun: vi.fn(),
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
      strategy: null,
      scenario: null,
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
      inference_cost_quote_total: null,
      net_return_pct: null,
      filter_summaries: [],
      auto_fire_review: false,
      review_model: null,
      max_annotations_per_review: 8,
      paused: false,
      paused_at: null,
      flatten_requested: false,
    },
    decisions: [decision()],
    equity_curve: [
      { timestamp: "2026-05-13T14:00:00Z", equity_usd: 10000 },
      { timestamp: "2026-05-13T14:10:00Z", equity_usd: 10250 },
      { timestamp: "2026-05-13T14:30:00Z", equity_usd: 10642 },
    ],
    filter_events: [],
    filter_summaries: [],
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
    vi.mocked(strategyApi.getStrategy).mockResolvedValue({
      agents: [{ agent_id: "agent-trader-1", role: "trader" }],
    } as any);
    vi.mocked(agentsApi.listAgents).mockResolvedValue([
      {
        agent_id: "agent-trader-1",
        name: "Trader Agent",
      } as any,
    ]);
    vi.mocked(agentRunsApi.getAgentRun).mockResolvedValue({
      summary: {
        total_cost_usd: 0.000567,
      },
      spans: [],
      model_calls: [],
      tool_calls: [],
    } as any);
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
    // Full eval id is surfaced as its own row in the hero (no `run ` prefix
    // after QA22 / `eval-id-resurface-no-truncate`).
    expect(screen.getAllByText("01LIVE").length).toBeGreaterThan(0);

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
    const tape = screen.getByTestId("mobile-eval-decision-tape");
    expect(within(tape).getByText("BUY")).toBeInTheDocument();
    const equity = screen.getByRole("img", { name: "Equity curve" });
    expect(equity).toBeInTheDocument();
    expect(
      tape.compareDocumentPosition(equity) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("renders mobile context links and total inference cost", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderRoute();

    const strip = await screen.findByTestId("mobile-eval-inspector-context-strip");
    expect(
      within(strip).getByRole("link", {
        name: /open strategy mean reversion v3/i,
      }),
    ).toHaveAttribute("href", "/strategies/mean-reversion-v3");
    expect(
      within(strip).getByRole("link", {
        name: /open scenario flash crash 2024/i,
      }),
    ).toHaveAttribute("href", "/scenarios/flash-crash-2024-08");
    expect(
      await within(strip).findByRole("link", {
        name: /open trader trader agent/i,
      }),
    ).toHaveAttribute("href", "/agents/agent-trader-1");
    expect(await screen.findByText("$0.000567")).toBeInTheDocument();
    expect(agentRunsApi.getAgentRun).toHaveBeenCalledWith("01LIVE");
  });

  it("switches to DECISIONS tab and renders decision cards with action pill + conviction", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderRoute();

    fireEvent.click(await screen.findByRole("tab", { name: "DECISIONS" }));

    // Step / trader-call / trade counter row. The legacy format read
    // "N STEPS · M TRADES" where N was the per-asset row count; on a
    // 5-step / 5-asset run it said "22 STEPS" instead of "5 STEPS". The
    // chip now reports distinct decision steps as the primary count and
    // surfaces the per-asset trader-call count alongside it.
    expect(
      screen.getByText(/1 STEP · 1 TRADER CALL · 1 TRADE/),
    ).toBeInTheDocument();
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
    // Full eval id moved out of the meta strip and is rendered (untruncated)
    // immediately below the strategy-name title. See QA22 /
    // `eval-id-resurface-no-truncate`.
    const idEl = await screen.findByTestId("mobile-eval-run-id");
    expect(idEl.textContent).toBe("01LIVE");
    // Old meta strip no longer carries the run id.
    expect(meta.textContent ?? "").not.toMatch(/01LIVE/);
    // The redundant `strategy <id>` chip is gone from the mobile hero.
    expect(meta.textContent ?? "").not.toMatch(/strategy mean-reversion-v3/);
  });

  it("renders the mobile action row as one quiet toolbar sharing the ACTION_BTN base", async () => {
    // Failed + terminal so all three actions (Retry + Download + Delete)
    // render on the same row. Each shares the quiet ACTION_BTN base — soft
    // border on the elevated surface, accent only on hover — so the rest
    // className begins with that prefix and carries no `min-w-[16ch]`
    // floor or loud at-rest colored box (QA30 inspector redesign; replaces
    // the prior uniform-min-width row from the PR #265 qa-review).
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        summary: {
          ...detail().summary,
          status: "failed",
          completed_at: "2026-05-13T14:30:00Z",
          error: "boom",
        },
      }),
    );

    renderRoute();

    const retry = await screen.findByRole("button", {
      name: /retry eval run 01LIVE/i,
    });
    const download = await screen.findByRole("button", {
      name: /download eval run 01LIVE as json/i,
    });
    const del = await screen.findByRole("button", {
      name: /delete eval run 01LIVE/i,
    });
    for (const button of [retry, download, del]) {
      expect(button.className).toMatch(
        /^inline-flex items-center justify-center gap-1\.5 rounded-sm border border-border-soft bg-surface-elev\b/,
      );
      expect(button.className).not.toContain("min-w-[16ch]");
    }
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
    // Mobile tablist must not appear in desktop layout; the desktop Signal
    // topbar (mobile uses a LIVE strip instead) is the positive sentinel.
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
    expect(screen.getByTestId("eval-topbar")).toBeInTheDocument();
  });

  it("DECISIONS counter reports STEPS as distinct timestamps, not per-asset row count", async () => {
    // Regression guard for the multi-asset case. Two steps (TS_A, TS_B), each
    // fanned out into BTC + ETH = 4 trader-call rows. The legacy chip read
    // "4 STEPS · …" because it used decisions.length. After the fix:
    //   "2 STEPS · 4 TRADER CALLS · M TRADES"
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        decisions: [
          decision({
            decision_index: 0,
            timestamp: "2024-01-01T20:00:00Z",
            asset: "BTC/USD",
            pnl_realized: null,
          }),
          decision({
            decision_index: 1,
            timestamp: "2024-01-01T20:00:00Z",
            asset: "ETH/USD",
            pnl_realized: null,
          }),
          decision({
            decision_index: 2,
            timestamp: "2024-01-07T13:00:00Z",
            asset: "BTC/USD",
            pnl_realized: 100,
          }),
          decision({
            decision_index: 3,
            timestamp: "2024-01-07T13:00:00Z",
            asset: "ETH/USD",
            pnl_realized: 50,
          }),
        ],
      }),
    );

    renderRoute();
    fireEvent.click(await screen.findByRole("tab", { name: "DECISIONS" }));

    expect(
      await screen.findByText(/2 STEPS · 4 TRADER CALLS · 2 TRADES/),
    ).toBeInTheDocument();
  });

  it("DECISIONS tab renders SHORT / COVER pills resolved against prior side (QA22 round-4)", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(
      detail({
        decisions: [
          decision({
            decision_index: 0,
            action: "short_open",
            fill_size: 0.5,
            fill_price: 60_000,
          }),
          decision({
            decision_index: 1,
            action: "flat",
            fill_size: 0.5,
            fill_price: 59_000,
            pnl_realized: 500,
          }),
        ],
      }),
    );

    renderRoute();

    fireEvent.click(await screen.findByRole("tab", { name: "DECISIONS" }));

    // Pill #0 should read SHORT (short_open from flat).
    expect(await screen.findByText("SHORT")).toBeInTheDocument();
    // Pill #1 should read COVER (flat closing a short).
    expect(screen.getByText("COVER")).toBeInTheDocument();
  });
});
