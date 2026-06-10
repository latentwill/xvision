import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";

import type { RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import { StrategyOutcomesSummary } from "./StrategyOutcomesSummary";

// ─── Fixture helpers (mirror StrategyOutcomesList.test.tsx) ──────────────────

function makeStrategy(
  id: string,
  name: string,
  overrides: Partial<StrategyListItem> = {},
): StrategyListItem {
  return {
    agent_id: id,
    display_name: name,
    template: "default",
    decision_cadence_minutes: 60,
    ...overrides,
  };
}

function makeRun(
  id: string,
  strategyId: string,
  opts: {
    status?: string;
    sharpe?: number | null;
    max_drawdown_pct?: number | null;
    total_return_pct?: number | null;
    completed_at?: string | null;
  } = {},
): RunSummary {
  return {
    id,
    agent_id: strategyId,
    scenario_id: "scenario-1",
    strategy: { id: strategyId, display_name: "Strategy " + strategyId },
    scenario: null,
    mode: "backtest",
    status: opts.status ?? "completed",
    started_at: "2026-01-01T00:00:00Z",
    completed_at: opts.completed_at ?? "2026-01-01T01:00:00Z",
    sharpe: opts.sharpe !== undefined ? opts.sharpe : 1.2,
    max_drawdown_pct:
      opts.max_drawdown_pct !== undefined ? opts.max_drawdown_pct : 5.0,
    total_return_pct:
      opts.total_return_pct !== undefined ? opts.total_return_pct : 8.0,
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
  };
}

function renderSummary(strategies: StrategyListItem[], runs: RunSummary[]) {
  return render(
    <MemoryRouter>
      <StrategyOutcomesSummary strategies={strategies} runs={runs} />
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
});

describe("StrategyOutcomesSummary", () => {
  it("shows at most three strongest and three weakest strategies", () => {
    const strategies = [
      makeStrategy("s1", "Alpha"),
      makeStrategy("s2", "Bravo"),
      makeStrategy("s3", "Charlie"),
      makeStrategy("s4", "Delta"),
      makeStrategy("s5", "Echo"),
      makeStrategy("s6", "Foxtrot"),
      makeStrategy("s7", "Golf"),
      makeStrategy("s8", "Hotel"),
    ];
    const runs = [
      makeRun("r1", "s1", { total_return_pct: 22, sharpe: 2.1 }),
      makeRun("r2", "s2", { total_return_pct: 18, sharpe: 1.7 }),
      makeRun("r3", "s3", { total_return_pct: 11, sharpe: 1.4 }),
      makeRun("r4", "s4", { total_return_pct: 4, sharpe: 1.1 }),
      makeRun("r5", "s5", { total_return_pct: -2, sharpe: 0.6 }),
      makeRun("r6", "s6", { total_return_pct: -8, sharpe: 0.3 }),
      makeRun("r7", "s7", { total_return_pct: -13, sharpe: -0.1 }),
      makeRun("r8", "s8", { total_return_pct: -21, sharpe: -0.4 }),
    ];

    renderSummary(strategies, runs);

    expect(screen.getByText("Alpha")).toBeInTheDocument();
    expect(screen.getByText("Bravo")).toBeInTheDocument();
    expect(screen.getByText("Charlie")).toBeInTheDocument();
    expect(screen.getByText("Foxtrot")).toBeInTheDocument();
    expect(screen.getByText("Golf")).toBeInTheDocument();
    expect(screen.getByText("Hotel")).toBeInTheDocument();
    expect(screen.queryByText("Delta")).toBeNull();
    expect(screen.queryByText("Echo")).toBeNull();
  });

  it("shows awaiting-first-eval count without rendering every no-eval strategy", () => {
    const strategies = Array.from({ length: 10 }, (_, i) =>
      makeStrategy(`s${i + 1}`, `Strategy ${i + 1}`),
    );
    const runs = [
      makeRun("r1", "s1", { total_return_pct: 7, sharpe: 1.2 }),
      makeRun("r2", "s2", { total_return_pct: -4, sharpe: 0.5 }),
    ];

    renderSummary(strategies, runs);

    const link = screen.getByRole("link", {
      name: "8 user strategies awaiting first eval",
    });
    expect(link).toHaveAttribute("href", "/eval-runs");
    expect(screen.queryByText("Strategy 10")).toBeNull();
  });

  it("segments optimizer-generated strategies out of the awaiting count", () => {
    const strategies = [
      makeStrategy("s1", "Alpha"),
      makeStrategy("s2", "Bravo"), // user, never evaluated
      makeStrategy("s3", "Charlie", { origin: "optimizer" }),
      makeStrategy("s4", "Delta", { origin: "optimizer" }),
    ];
    const runs = [makeRun("r1", "s1", { total_return_pct: 7, sharpe: 1.2 })];

    renderSummary(strategies, runs);

    expect(
      screen.getByRole("link", { name: "1 user strategy awaiting first eval" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("2 optimizer-generated (evaluated in lineage)"),
    ).toBeInTheDocument();
  });

  it("counts hash-only CLI runs via bundle_hash and server-side evaluated flags", () => {
    const hash =
      "a472499597277873fc0a9018084098fceecd4ffc903329aac889c7e2cf3a36bc";
    const strategies = [
      makeStrategy("s1", "Alpha", { bundle_hash: hash }), // CLI run keyed by hash
      makeStrategy("s2", "Bravo", { evaluated: true }), // evals outside this page
      makeStrategy("s3", "Charlie"), // genuinely unevaluated
    ];
    const runs = [makeRun("r1", hash, { total_return_pct: 4, sharpe: 1.1 })];

    renderSummary(strategies, runs);

    // Only Charlie is awaiting; Alpha matched by hash, Bravo by server flag.
    expect(
      screen.getByRole("link", { name: "1 user strategy awaiting first eval" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("summary-row-s1")).toBeInTheDocument();
  });

  it("renders the headings and the link to the full strategies surface", () => {
    const strategies = [makeStrategy("s1", "Alpha")];
    const runs = [makeRun("r1", "s1", { total_return_pct: 9, sharpe: 1.3 })];

    renderSummary(strategies, runs);

    expect(screen.getByTestId("strategy-outcomes-summary")).toBeInTheDocument();
    expect(screen.getByText("Strategy outcomes")).toBeInTheDocument();
    expect(screen.getByText(/latest completed evals/i)).toBeInTheDocument();
    const link = screen.getByRole("link", { name: /view all strategies/i });
    expect(link).toHaveAttribute("href", "/strategies");
  });

  it("joins runs to strategies via agent_id when strategy metadata is absent", () => {
    // The list endpoint never enriches run.strategy — without the agent_id
    // fallback the dashboard claimed "no completed evals yet" while showing a
    // nonzero completed-eval count right above.
    const strategies = [makeStrategy("s1", "Alpha")];
    const run = makeRun("r1", "s1", { total_return_pct: 9, sharpe: 1.3 });
    run.strategy = null;

    renderSummary(strategies, [run]);

    expect(screen.getByTestId("summary-row-s1")).toBeInTheDocument();
    expect(screen.queryByText(/no completed evals yet/i)).toBeNull();
  });

  it("prompts to create a strategy when none are configured", () => {
    renderSummary([], []);

    expect(screen.getByText(/no strategies configured/i)).toBeInTheDocument();
    const link = screen.getByRole("link", { name: /create one/i });
    expect(link).toHaveAttribute("href", "/strategies");
    // Must NOT prompt an eval when there is nothing to evaluate yet.
    expect(screen.queryByText(/no completed evals yet/i)).toBeNull();
  });

  it("does not render a strategy in both the strongest and weakest sections", () => {
    const strategies = [makeStrategy("s1", "Alpha"), makeStrategy("s2", "Bravo")];
    const runs = [
      makeRun("r1", "s1", { total_return_pct: 12, sharpe: 1.5 }),
      makeRun("r2", "s2", { total_return_pct: 3, sharpe: 1.1 }),
    ];

    renderSummary(strategies, runs);

    // Each evaluated strategy appears exactly once even though section caps
    // (3 each) exceed the evaluated count.
    expect(screen.getAllByText("Alpha")).toHaveLength(1);
    expect(screen.getAllByText("Bravo")).toHaveLength(1);
  });
});
