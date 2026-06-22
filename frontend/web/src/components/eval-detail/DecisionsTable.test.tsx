import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";

import type { FilterSummary } from "@/api/types.gen/FilterSummary";

import { DecisionsTable } from "./DecisionsTable";
import type { TimelineDecision } from "./decision-view";

function makeSummary(overrides: Partial<FilterSummary> = {}): FilterSummary {
  return {
    filter_id: "f_test",
    bars_scanned: 0,
    wakeups: 0,
    suppressed_in_position: 0,
    suppressed_cooldown: 0,
    suppressed_daily_cap: 0,
    llm_calls_saved: 0,
    estimated_tokens_saved: 0,
    ...overrides,
  };
}

// Two decision steps, each fanned out into a BTC + ETH row at one shared
// timestamp — the real multi-asset shape (decision_index 0/1 then 2/3).
const TS_A = "2024-01-01T20:00:00+00:00";
const TS_B = "2024-01-07T13:00:00+00:00";
const decisions: TimelineDecision[] = [
  { i: 0, t: TS_A, phase: "engaged", action: "BUY", conv: 0.7, just: "btc entry", pnl: -3.75, asset: "BTC/USD" },
  { i: 1, t: TS_A, phase: "engaged", action: "BUY", conv: 0.7, just: "eth entry", pnl: -3.75, asset: "ETH/USD" },
  { i: 2, t: TS_B, phase: "engaged", action: "HOLD", conv: 0.6, just: "btc hold", pnl: null, asset: "BTC/USD" },
  { i: 3, t: TS_B, phase: "engaged", action: "HOLD", conv: 0.5, just: "eth hold", pnl: null, asset: "ETH/USD" },
];

function stepColumn(container: HTMLElement): string[] {
  return [...container.querySelectorAll("tbody tr")].map(
    (tr) => tr.querySelector("td")?.textContent?.trim() ?? "",
  );
}

describe("DecisionsTable step + asset columns", () => {
  test("renders STEP and ASSET headers and both assets", () => {
    render(<DecisionsTable decisions={decisions} focusedIdx={null} onJump={() => {}} />);
    expect(screen.getByText("STEP")).toBeInTheDocument();
    expect(screen.getByText("ASSET")).toBeInTheDocument();
    // Short symbols, one per row.
    expect(screen.getAllByText("BTC")).toHaveLength(2);
    expect(screen.getAllByText("ETH")).toHaveLength(2);
  });

  test("summary chip is step-centric, not per-asset row count", () => {
    // Regression guard: the chip used to read
    //   "4 of 4 decisions · 2 steps · 4 engaged"
    // triple-counting the multi-asset fanout. 4 rows / 2 assets across 2
    // steps should surface as 2 steps (primary), 2 engaged, 4 trader calls.
    const { container } = render(
      <DecisionsTable decisions={decisions} focusedIdx={null} onJump={() => {}} />,
    );
    expect(container.textContent).toContain("2 of 2 steps");
    expect(container.textContent).toContain("2 engaged");
    expect(container.textContent).toContain("4 trader calls");
    expect(container.textContent).not.toContain("decisions");
  });

  test("density-strip header reports step count, not per-asset row count", () => {
    const { container } = render(
      <DecisionsTable decisions={decisions} focusedIdx={null} onJump={() => {}} />,
    );
    const strip = container.querySelector('[data-testid="decision-density-strip"]');
    expect(strip).not.toBeNull();
    // The strip used to read "4 steps · …" (using sorted.length). Now it
    // reports the distinct step count and the per-asset rows separately.
    expect(strip?.textContent).toContain("2 steps");
    expect(strip?.textContent).toContain("4 trader calls");
  });

  test("chronological sort shows the step on the first row of each step and blanks the rest", () => {
    const { container } = render(
      <DecisionsTable decisions={decisions} focusedIdx={null} onJump={() => {}} />,
    );
    // Default sort is time-asc → step 1 (BTC, ETH) then step 2 (BTC, ETH).
    expect(stepColumn(container)).toEqual(["1", "", "2", ""]);
  });

  test("non-chronological sort numbers every row (no blanking)", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <DecisionsTable decisions={decisions} focusedIdx={null} onJump={() => {}} />,
    );
    await user.click(screen.getByRole("button", { name: /sort/i }));
    await user.click(await screen.findByRole("option", { name: /PnL high/i }));
    // Every row carries its own step number when the rows may be scattered.
    expect(stepColumn(container).every((s) => s === "1" || s === "2")).toBe(true);
    expect(stepColumn(container)).not.toContain("");
  });

  test("PHASE chip renders once per step in chronological sort, not per asset row", () => {
    // Regression guard: a single multi-asset step used to render ENGAGED on
    // every per-asset row, reading as "engaged every row" instead of "engaged
    // every step."
    render(<DecisionsTable decisions={decisions} focusedIdx={null} onJump={() => {}} />);
    // PhaseChip uppercases its label. Two steps ⇒ two chips in chrono sort,
    // not four (one per row).
    expect(screen.getAllByText("ENGAGED")).toHaveLength(2);
  });

  test("PHASE chip renders on every row in non-chronological sort", async () => {
    // When same-step rows can be scattered by sort key, every row needs its
    // own chip — mirrors the STEP-number behaviour.
    const user = userEvent.setup();
    render(<DecisionsTable decisions={decisions} focusedIdx={null} onJump={() => {}} />);
    await user.click(screen.getByRole("button", { name: /sort/i }));
    await user.click(await screen.findByRole("option", { name: /PnL high/i }));
    expect(screen.getAllByText("ENGAGED")).toHaveLength(4);
  });

  test("timestamp column shows UTC date + HH:MM:SS and exposes full ISO on hover", () => {
    // Regression guard: the column used to render `HH:MM:SS.mmm` only, so a
    // multi-day run looked like every step happened at the same time.
    const { container } = render(
      <DecisionsTable decisions={decisions} focusedIdx={null} onJump={() => {}} />,
    );
    const rows = [...container.querySelectorAll("tbody tr")];
    // 3rd <td> is TIMESTAMP (after STEP, ASSET).
    const stamps = rows.map((tr) => tr.querySelectorAll("td")[2]);
    expect(stamps[0]?.textContent).toBe("2024-01-01 20:00:00");
    expect(stamps[2]?.textContent).toBe("2024-01-07 13:00:00");
    // The raw ISO stays accessible via the title attr for copy-paste into the
    // CLI / log search.
    expect(stamps[0]?.getAttribute("title")).toBe(TS_A);
  });

  test("action-filter pill row has All/Buy/Sell/Short/Hold pills and no No-op", () => {
    // No-op/Filtered pill removed: filtered rows aren't real decisions and
    // the pill added clutter. SHORT pill added for short-entry actions.
    render(<DecisionsTable decisions={decisions} focusedIdx={null} onJump={() => {}} />);
    expect(screen.getByRole("button", { name: /All/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Buy/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Sell/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Short/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Hold/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /No-op/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Filtered/i })).not.toBeInTheDocument();
  });

  test("engine-filter activity line is omitted when no filter summaries are present", () => {
    // EveryBar runs (and any run with empty filter_summaries) must not
    // render the activity line at all — the layout shouldn't shift for them.
    render(<DecisionsTable decisions={decisions} focusedIdx={null} onJump={() => {}} />);
    expect(
      screen.queryByTestId("decisions-filter-activity"),
    ).not.toBeInTheDocument();
  });

  test("engine-filter activity line surfaces bars scanned / fired / suppressed", () => {
    // Intake §5. The 1399 suppressed bars exist in filter_summaries but
    // never become decision rows; without this line the operator reading
    // the Decisions card in isolation concludes "every step was engaged."
    render(
      <DecisionsTable
        decisions={decisions}
        focusedIdx={null}
        onJump={() => {}}
        filterSummaries={[
          makeSummary({
            bars_scanned: 1404,
            wakeups: 5,
            llm_calls_saved: 1399,
          }),
        ]}
      />,
    );
    const activity = screen.getByTestId("decisions-filter-activity");
    expect(activity.textContent).toContain("1,404 bars scanned");
    expect(activity.textContent).toContain("5 fired");
    expect(activity.textContent).toContain("1,399 suppressed");
  });

  test("engine-filter activity sums across multiple filter summaries", () => {
    // Multi-filter runs (a strategy with more than one filter artifact)
    // aggregate their counters into one operator-readable activity line.
    render(
      <DecisionsTable
        decisions={decisions}
        focusedIdx={null}
        onJump={() => {}}
        filterSummaries={[
          makeSummary({ bars_scanned: 1000, wakeups: 3, llm_calls_saved: 997 }),
          makeSummary({
            filter_id: "f_other",
            bars_scanned: 404,
            wakeups: 2,
            llm_calls_saved: 402,
          }),
        ]}
      />,
    );
    const activity = screen.getByTestId("decisions-filter-activity");
    expect(activity.textContent).toContain("1,404 bars scanned");
    expect(activity.textContent).toContain("5 fired");
    expect(activity.textContent).toContain("1,399 suppressed");
  });

  test("engine-filter activity line is omitted when all counters are zero", () => {
    // Defensive: a summary present but with no activity (e.g. run errored
    // before scanning any bars) shouldn't render a meaningless "0 bars
    // scanned · 0 fired · 0 suppressed" line.
    render(
      <DecisionsTable
        decisions={decisions}
        focusedIdx={null}
        onJump={() => {}}
        filterSummaries={[makeSummary()]}
      />,
    );
    expect(
      screen.queryByTestId("decisions-filter-activity"),
    ).not.toBeInTheDocument();
  });
});
