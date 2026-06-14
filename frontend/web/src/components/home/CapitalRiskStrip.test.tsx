// frontend/web/src/components/home/CapitalRiskStrip.test.tsx
//
// Component spec for the home capital-risk strip (bead 8s4, CT5 §9.3). Verifies
// the three live-money metrics render, the HONESTY invariants (null → "—", the
// below-floor "insufficient data" state, the deferred risk-veto chip rendering
// "—" not 0), the buffer color-coding toward danger, the route into /live, and
// the no-popup rule.

import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

import { CapitalRiskStrip } from "./CapitalRiskStrip";
import {
  aggregateCapitalRisk,
  type CapitalRiskAggregate,
} from "@/features/home/capital-risk";
import type { LiveDeploymentSummary } from "@/api/types.gen";

function dep(over: Partial<LiveDeploymentSummary>): LiveDeploymentSummary {
  return {
    deployment_id: "dep-1",
    strategy_id: "strat-1",
    strategy_name: "S1",
    mode: "paper",
    status: "running",
    started_at: "2026-06-13T00:00:00Z",
    last_decision_at: null,
    venue: "alpaca-paper",
    venue_connected: true,
    deployed_capital_usd: null,
    realized_pnl_usd: null,
    unrealized_pnl_usd: null,
    drawdown_pct: null,
    daily_loss_limit_remaining_usd: null,
    daily_loss_budget_usd: null,
    stop_at: null,
    risk_veto_count_since_last_visit: null,
    paused: false,
    flatten_requested: false,
    global_safety_paused: false,
    source: "human",
    unavailable_reason: null,
    ...over,
  };
}

function renderStrip(agg: CapitalRiskAggregate) {
  return render(
    <MemoryRouter>
      <CapitalRiskStrip agg={agg} />
    </MemoryRouter>,
  );
}

// A populated, healthy aggregate.
const HEALTHY = aggregateCapitalRisk([
  dep({
    deployment_id: "a",
    deployed_capital_usd: 1000,
    drawdown_pct: 2.5,
    daily_loss_limit_remaining_usd: 800,
    daily_loss_budget_usd: 1000,
  }),
]);

describe("CapitalRiskStrip", () => {
  it("renders the three live-money metrics when there is data", () => {
    renderStrip(HEALTHY);
    const strip = screen.getByTestId("capital-risk-strip");

    const cap = within(strip).getByTestId("capital-risk-deployed");
    const dd = within(strip).getByTestId("capital-risk-drawdown");
    const buf = within(strip).getByTestId("capital-risk-buffer");

    expect(cap.textContent).toMatch(/\$1,000/);
    expect(dd.textContent).toMatch(/2\.5/);
    expect(buf.textContent).toMatch(/\$800/);
  });

  it("renders an explicit 'insufficient data' state below the data floor (never a green zero)", () => {
    const floor = aggregateCapitalRisk([]); // hasData=false
    renderStrip(floor);
    const strip = screen.getByTestId("capital-risk-strip");

    expect(within(strip).getByTestId("capital-risk-empty")).toBeInTheDocument();
    expect(strip.textContent).toMatch(/insufficient data/i);
    expect(strip.textContent).toMatch(/no live capital deployed/i);
    // It must NOT fabricate a calm $0 metric grid in the floor state.
    expect(within(strip).queryByTestId("capital-risk-deployed")).toBeNull();
    expect(strip.textContent).not.toMatch(/\$0/);
  });

  it("also hits the floor state when deployments exist but every field is null", () => {
    const allNull = aggregateCapitalRisk([dep({}), dep({ deployment_id: "b" })]);
    renderStrip(allNull);
    const strip = screen.getByTestId("capital-risk-strip");
    expect(within(strip).getByTestId("capital-risk-empty")).toBeInTheDocument();
    expect(strip.textContent).toMatch(/insufficient data/i);
  });

  it("renders '—' for an individual null field, never a fabricated $0", () => {
    // Has SOME data (drawdown) so it is above the floor, but deployed capital
    // and buffer are null → each renders the em-dash.
    const partial = aggregateCapitalRisk([dep({ drawdown_pct: 4 })]);
    renderStrip(partial);
    const strip = screen.getByTestId("capital-risk-strip");

    expect(within(strip).getByTestId("capital-risk-deployed").textContent).toBe("—");
    expect(within(strip).getByTestId("capital-risk-buffer").textContent).toBe("—");
    expect(within(strip).getByTestId("capital-risk-drawdown").textContent).toMatch(/4/);
    // No fabricated $0 anywhere.
    expect(strip.textContent).not.toMatch(/\$0\b/);
  });

  it("color-codes the buffer healthy when it is comfortably large", () => {
    renderStrip(HEALTHY); // buffer 800 of 1000 → healthy
    const buf = screen.getByTestId("capital-risk-buffer");
    expect(buf.getAttribute("data-tone")).toBe("healthy");
  });

  it("color-codes the buffer danger as it approaches 0", () => {
    const danger = aggregateCapitalRisk([
      dep({ daily_loss_limit_remaining_usd: 0, daily_loss_budget_usd: 1000 }),
    ]);
    renderStrip(danger);
    const buf = screen.getByTestId("capital-risk-buffer");
    expect(buf.getAttribute("data-tone")).toBe("danger");
  });

  it("color-codes the buffer warn in the middle band", () => {
    const warn = aggregateCapitalRisk([
      dep({ daily_loss_limit_remaining_usd: 70, daily_loss_budget_usd: 1000 }),
    ]);
    renderStrip(warn);
    const buf = screen.getByTestId("capital-risk-buffer");
    expect(buf.getAttribute("data-tone")).toBe("warn");
  });

  it("keeps the buffer neutral when its budget is not sourced", () => {
    const neutral = aggregateCapitalRisk([
      dep({ daily_loss_limit_remaining_usd: 500, daily_loss_budget_usd: null }),
    ]);
    renderStrip(neutral);
    const buf = screen.getByTestId("capital-risk-buffer");
    expect(buf.getAttribute("data-tone")).toBeNull();
  });

  // bead s78.2: the risk-veto chip now renders the REAL aggregate count.
  it("renders '—' for the risk-veto chip when the count is null (no boundary)", () => {
    // HEALTHY's deployment has risk_veto_count_since_last_visit: null (no
    // ?since boundary) ⇒ the aggregate count is null ⇒ chip shows "—".
    renderStrip(HEALTHY);
    const chip = screen.getByTestId("capital-risk-veto");
    expect(chip.textContent).toBe("—");
  });

  it("renders the real summed risk-veto count when known", () => {
    const withVetoes = aggregateCapitalRisk([
      dep({
        deployment_id: "a",
        deployed_capital_usd: 1000,
        daily_loss_limit_remaining_usd: 800,
        risk_veto_count_since_last_visit: 3,
      }),
      dep({
        deployment_id: "b",
        deployed_capital_usd: 500,
        daily_loss_limit_remaining_usd: 400,
        risk_veto_count_since_last_visit: 5,
      }),
    ]);
    renderStrip(withVetoes);
    const chip = screen.getByTestId("capital-risk-veto");
    expect(chip.textContent).toBe("8");
  });

  it("renders a real 0 risk-veto count as '0' (honest), NOT '—'", () => {
    // A boundary WAS supplied and the real count is 0 ("0 vetoes since you
    // were last here") — an honest fact, distinct from null.
    const zeroVetoes = aggregateCapitalRisk([
      dep({
        deployment_id: "a",
        deployed_capital_usd: 1000,
        daily_loss_limit_remaining_usd: 800,
        risk_veto_count_since_last_visit: 0,
      }),
    ]);
    renderStrip(zeroVetoes);
    const chip = screen.getByTestId("capital-risk-veto");
    expect(chip.textContent).toBe("0");
    expect(chip.textContent).not.toBe("—");
  });

  it("routes to /live for per-deployment detail", () => {
    renderStrip(HEALTHY);
    const strip = screen.getByTestId("capital-risk-strip");
    const link = within(strip).getByRole("link");
    expect(link).toHaveAttribute("href", "/live");
  });

  it("never renders a focus-stealing dialog/overlay (no popups rule)", () => {
    renderStrip(HEALTHY);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});
