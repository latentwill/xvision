import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { DecisionsTable } from "./DecisionsTable";
import type { TimelineDecision } from "./decision-view";

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

  test("non-chronological sort numbers every row (no blanking)", () => {
    const { container } = render(
      <DecisionsTable decisions={decisions} focusedIdx={null} onJump={() => {}} />,
    );
    fireEvent.change(screen.getByLabelText("Sort decisions"), {
      target: { value: "pnl-desc" },
    });
    // Every row carries its own step number when the rows may be scattered.
    expect(stepColumn(container).every((s) => s === "1" || s === "2")).toBe(true);
    expect(stepColumn(container)).not.toContain("");
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
});
