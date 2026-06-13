import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";

import { HomeDeltaSubtitle } from "./HomeDeltaSubtitle";
import type { SinceDelta } from "@/features/home/last-visit";

function delta(over: Partial<SinceDelta>): SinceDelta {
  return {
    runsSince: 0,
    findingsSince: 0,
    hoursAgo: 0,
    firstVisit: false,
    ...over,
  };
}

describe("HomeDeltaSubtitle", () => {
  it("renders runs, findings, and the hours-ago stamp (populated state)", () => {
    render(
      <HomeDeltaSubtitle
        delta={delta({ runsSince: 3, findingsSince: 2, hoursAgo: 5 })}
      />,
    );
    const el = screen.getByTestId("home-delta-subtitle");
    expect(el.textContent).toContain("3 runs");
    expect(el.textContent).toContain("2 findings");
    expect(el.textContent).toContain("since you were last here");
    expect(el.textContent).toContain("5h ago");
  });

  it("renders a neutral first-visit line with no counts", () => {
    render(<HomeDeltaSubtitle delta={delta({ firstVisit: true, hoursAgo: null })} />);
    const el = screen.getByTestId("home-delta-subtitle");
    // Neutral welcome copy, no "since you were last here" delta phrasing.
    expect(el.textContent).not.toContain("since you were last here");
    expect(el.textContent).not.toMatch(/\d+ runs/);
    expect(el.textContent).toMatch(/welcome|at a glance|here'?s/i);
  });

  it("omits the hours-ago stamp when hoursAgo is null but not a first visit", () => {
    render(
      <HomeDeltaSubtitle
        delta={delta({ runsSince: 1, findingsSince: 0, hoursAgo: null })}
      />,
    );
    const el = screen.getByTestId("home-delta-subtitle");
    expect(el.textContent).toContain("1 run");
    expect(el.textContent).not.toContain("ago");
  });

  it("uses singular nouns for a count of one", () => {
    render(
      <HomeDeltaSubtitle
        delta={delta({ runsSince: 1, findingsSince: 1, hoursAgo: 2 })}
      />,
    );
    const el = screen.getByTestId("home-delta-subtitle");
    expect(el.textContent).toContain("1 run ");
    expect(el.textContent).toContain("1 finding ");
    expect(el.textContent).not.toContain("1 runs");
    expect(el.textContent).not.toContain("1 findings");
  });

  it("NEVER renders live-money / P&L phrasing (honesty mandate)", () => {
    render(
      <HomeDeltaSubtitle
        delta={delta({ runsSince: 9, findingsSince: 4, hoursAgo: 12 })}
      />,
    );
    const el = screen.getByTestId("home-delta-subtitle");
    expect(el.textContent).not.toMatch(/real money/i);
    expect(el.textContent).not.toMatch(/live strateg/i);
    expect(el.textContent).not.toMatch(/P&L/i);
    expect(el.textContent).not.toMatch(/\$/);
    expect(el.textContent).not.toMatch(/capital/i);
    expect(el.textContent).not.toMatch(/budget/i);
  });

  it("renders counts in a tabular-nums mono span", () => {
    render(
      <HomeDeltaSubtitle
        delta={delta({ runsSince: 7, findingsSince: 3, hoursAgo: 1 })}
      />,
    );
    const el = screen.getByTestId("home-delta-subtitle");
    const mono = el.querySelectorAll(".font-mono.tabular-nums");
    expect(mono.length).toBeGreaterThanOrEqual(1);
  });

  it("does not introduce a popup overlay (no role=dialog)", () => {
    render(
      <HomeDeltaSubtitle
        delta={delta({ runsSince: 2, findingsSince: 1, hoursAgo: 3 })}
      />,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
