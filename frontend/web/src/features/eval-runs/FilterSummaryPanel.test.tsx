// Tests for the Filter v1 read-only summary panel.
//
// The panel reads a list of `FilterSummary` rows (one per filter that ran
// during a backtest) and renders the bars-scanned / wake-ups / suppression
// breakdown / LLM-calls-saved metrics inline on the run-detail surface.
// EveryBar runs and runs that produced no
// FilterEventV1 rows pass `[]` and the panel renders nothing.

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";

import type { FilterSummary } from "@/api/types.gen/FilterSummary";

import { FilterSummaryPanel } from "./FilterSummaryPanel";

afterEach(() => {
  cleanup();
});

function makeSummary(overrides: Partial<FilterSummary> = {}): FilterSummary {
  return {
    filter_id: "f_01JX9TEST",
    bars_scanned: 100,
    wakeups: 5,
    suppressed_in_position: 1,
    suppressed_daily_cap: 0,
    suppressed_cooldown: 2,
    llm_calls_saved: 95,
    estimated_tokens_saved: 4_750_000,
    ...overrides,
  };
}

describe("FilterSummaryPanel", () => {
  it("renders nothing when given an empty summaries array", () => {
    const { container } = render(<FilterSummaryPanel summaries={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders the panel header with a count badge when summaries are present", () => {
    render(<FilterSummaryPanel summaries={[makeSummary()]} />);
    const panel = screen.getByTestId("filter-summary-panel");
    expect(within(panel).getByText(/^Filters/)).toBeInTheDocument();
    expect(within(panel).getByText("(1)")).toBeInTheDocument();
  });

  it("displays the per-metric counts for a single summary", () => {
    render(<FilterSummaryPanel summaries={[makeSummary()]} />);
    const row = screen.getByTestId("filter-summary-row");
    expect(row).toHaveAttribute("data-filter-id", "f_01JX9TEST");

    // bars scanned + total suppressed (1 + 0 + 2 = 3) + LLM calls saved
    expect(within(row).getByText("100")).toBeInTheDocument();
    expect(within(row).getByText("95")).toBeInTheDocument();
    expect(within(row).getByText("3")).toBeInTheDocument();
  });

  it("does not render the estimated token-savings metric", () => {
    render(<FilterSummaryPanel summaries={[makeSummary()]} />);
    const row = screen.getByTestId("filter-summary-row");
    expect(within(row).queryByText("4,750,000")).not.toBeInTheDocument();
    expect(within(row).queryByText(/est\. tokens saved/i)).not.toBeInTheDocument();
  });

  it("renders one row per summary when multiple filters are present", () => {
    const summaries = [
      makeSummary({ filter_id: "f_one", wakeups: 3 }),
      makeSummary({ filter_id: "f_two", wakeups: 7 }),
      makeSummary({ filter_id: "f_three", wakeups: 0 }),
    ];
    render(<FilterSummaryPanel summaries={summaries} />);
    const rows = screen.getAllByTestId("filter-summary-row");
    expect(rows).toHaveLength(3);
    expect(rows.map((r) => r.getAttribute("data-filter-id"))).toEqual([
      "f_one",
      "f_two",
      "f_three",
    ]);
    expect(screen.getByText("(3)")).toBeInTheDocument();
  });

  it("formats the wake-rate header to one decimal", () => {
    // 5 / 100 = 5.0 %
    render(<FilterSummaryPanel summaries={[makeSummary()]} />);
    const row = screen.getByTestId("filter-summary-row");
    expect(within(row).getByText(/5\.0%/)).toBeInTheDocument();
  });

  it("treats zero bars_scanned as a 0% wake rate (no NaN)", () => {
    render(
      <FilterSummaryPanel
        summaries={[
          makeSummary({
            bars_scanned: 0,
            wakeups: 0,
            suppressed_in_position: 0,
            suppressed_daily_cap: 0,
            suppressed_cooldown: 0,
            llm_calls_saved: 0,
            estimated_tokens_saved: 0,
          }),
        ]}
      />,
    );
    const row = screen.getByTestId("filter-summary-row");
    expect(within(row).getByText(/0\.0%/)).toBeInTheDocument();
  });
});
