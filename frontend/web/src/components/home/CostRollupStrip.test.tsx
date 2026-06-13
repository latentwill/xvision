// frontend/web/src/components/home/CostRollupStrip.test.tsx
//
// Component spec for the cost rollup strip (bead-8wn). Verifies:
//   - "$spend / $cap" when a cap is set, "$spend / —" when UNSET (never a
//     faked cap — honesty §8.1/§8.9),
//   - null spend renders an em-dash / "no cost data" (never a faked $0),
//   - the since-last-visit + this-week windows both render,
//   - over/approaching-cap carries a NON-COLOR cue (glyph/word) for WCAG 1.4.1,
//   - the no-popup invariant (no role="dialog").
//
// The component is presentational: it takes already-fetched rollups + cap as
// props (the home route owns fetching), mirroring DeployReadinessStrip.

import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";

import { CostRollupStrip } from "./CostRollupStrip";
import type { CostRollupResponse } from "@/api/cost";

function rollup(over: Partial<CostRollupResponse>): CostRollupResponse {
  return {
    since: "2026-06-12T00:00:00Z",
    spend_usd: 0,
    eval_cost_usd: 0,
    optimizer_cost_usd: 0,
    daily_cap_usd: null,
    ...over,
  };
}

function renderStrip(
  props: Partial<React.ComponentProps<typeof CostRollupStrip>> = {},
) {
  return render(
    <CostRollupStrip
      sinceLastVisit={null}
      thisWeek={null}
      dailyCapUsd={null}
      {...props}
    />,
  );
}

describe("CostRollupStrip", () => {
  it("renders the strip container with the documented test id", () => {
    renderStrip({
      sinceLastVisit: rollup({ spend_usd: 1.5 }),
      thisWeek: rollup({ spend_usd: 4.25 }),
    });
    expect(screen.getByTestId("cost-rollup-strip")).toBeTruthy();
  });

  it("renders nothing while both windows are still loading (null)", () => {
    const { container } = renderStrip({
      sinceLastVisit: null,
      thisWeek: null,
    });
    expect(container.firstChild).toBeNull();
  });

  it("shows '$spend / $cap' when an operator cap is set (this week)", () => {
    renderStrip({
      sinceLastVisit: rollup({ spend_usd: 1.0 }),
      thisWeek: rollup({ spend_usd: 12.5, daily_cap_usd: 50 }),
      dailyCapUsd: 50,
    });
    const week = screen.getByTestId("cost-rollup-week");
    // Honest numerator over the operator cap.
    expect(week.textContent).toContain("$12.50");
    expect(week.textContent).toContain("$50.00");
  });

  it("renders an em-dash denominator (never a faked cap) when the cap is UNSET", () => {
    renderStrip({
      sinceLastVisit: rollup({ spend_usd: 1.0 }),
      thisWeek: rollup({ spend_usd: 12.5, daily_cap_usd: null }),
      dailyCapUsd: null,
    });
    const week = screen.getByTestId("cost-rollup-week");
    expect(week.textContent).toContain("$12.50");
    expect(week.textContent).toContain("—");
    // No fabricated cap number sneaks in.
    expect(week.textContent).not.toMatch(/\/\s*\$0/);
  });

  it("shows an em-dash / 'no cost data' when spend is null (honesty: never a faked $0)", () => {
    renderStrip({
      sinceLastVisit: rollup({ spend_usd: null }),
      thisWeek: rollup({ spend_usd: null }),
    });
    const since = screen.getByTestId("cost-rollup-since");
    expect(since.textContent).toMatch(/—|no cost data/i);
    // A null source must NOT be coerced to "$0.00".
    expect(since.textContent).not.toContain("$0.00");
  });

  it("renders both the since-last-visit and this-week windows", () => {
    renderStrip({
      sinceLastVisit: rollup({ spend_usd: 1.5 }),
      thisWeek: rollup({ spend_usd: 4.25 }),
    });
    const strip = screen.getByTestId("cost-rollup-strip");
    expect(within(strip).getByTestId("cost-rollup-since")).toBeTruthy();
    expect(within(strip).getByTestId("cost-rollup-week")).toBeTruthy();
    expect(within(strip).getByTestId("cost-rollup-since").textContent).toContain("$1.50");
    expect(within(strip).getByTestId("cost-rollup-week").textContent).toContain("$4.25");
  });

  it("labels the since-last-visit window as a first-visit when no boundary exists", () => {
    renderStrip({
      sinceLastVisit: null,
      thisWeek: rollup({ spend_usd: 4.25 }),
      firstVisit: true,
    });
    const since = screen.getByTestId("cost-rollup-since");
    // First visit has no prior boundary → an honest "first visit" label, not a
    // fabricated since-window number.
    expect(since.textContent).toMatch(/first visit/i);
  });

  // ─── WCAG 1.4.1: cap-state is conveyed by a glyph/word, not color alone ─────

  it("flags over-cap spend with a NON-COLOR cue (glyph/word), not color alone", () => {
    renderStrip({
      sinceLastVisit: rollup({ spend_usd: 1.0 }),
      thisWeek: rollup({ spend_usd: 60, daily_cap_usd: 50 }),
      dailyCapUsd: 50,
    });
    const week = screen.getByTestId("cost-rollup-week");
    // A textual / glyph cue must accompany the danger tint.
    expect(week.getAttribute("data-cap-state")).toBe("over");
    expect(week.textContent).toMatch(/over/i);
  });

  it("flags approaching-cap spend with a NON-COLOR cue", () => {
    renderStrip({
      sinceLastVisit: rollup({ spend_usd: 1.0 }),
      thisWeek: rollup({ spend_usd: 45, daily_cap_usd: 50 }),
      dailyCapUsd: 50,
    });
    const week = screen.getByTestId("cost-rollup-week");
    expect(week.getAttribute("data-cap-state")).toBe("near");
    expect(week.textContent).toMatch(/near|approaching/i);
  });

  it("marks an under-cap window as ok (no alarm)", () => {
    renderStrip({
      sinceLastVisit: rollup({ spend_usd: 1.0 }),
      thisWeek: rollup({ spend_usd: 5, daily_cap_usd: 50 }),
      dailyCapUsd: 50,
    });
    const week = screen.getByTestId("cost-rollup-week");
    expect(week.getAttribute("data-cap-state")).toBe("ok");
    expect(week.textContent).not.toMatch(/over/i);
  });

  it("never alarms when the cap is UNSET, even on large spend", () => {
    renderStrip({
      sinceLastVisit: rollup({ spend_usd: 1.0 }),
      thisWeek: rollup({ spend_usd: 9999, daily_cap_usd: null }),
      dailyCapUsd: null,
    });
    const week = screen.getByTestId("cost-rollup-week");
    expect(week.getAttribute("data-cap-state")).toBe("none");
    expect(week.textContent).not.toMatch(/over/i);
  });

  it("never renders a focus-stealing dialog/overlay (no popups rule)", () => {
    renderStrip({
      sinceLastVisit: rollup({ spend_usd: 1.0 }),
      thisWeek: rollup({ spend_usd: 60, daily_cap_usd: 50 }),
      dailyCapUsd: 50,
    });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("uses theme border tokens, never raw white/gray borders (dark-mode rule)", () => {
    renderStrip({
      sinceLastVisit: rollup({ spend_usd: 1.0 }),
      thisWeek: rollup({ spend_usd: 5 }),
    });
    const strip = screen.getByTestId("cost-rollup-strip");
    const cls = strip.className;
    expect(cls).not.toMatch(/border-white|border-gray-100|border-gray-200/);
  });
});
