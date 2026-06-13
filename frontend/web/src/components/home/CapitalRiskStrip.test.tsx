// frontend/web/src/components/home/CapitalRiskStrip.test.tsx
//
// Component spec for the 8s4 capital-risk strip (CT5 S1). Verifies:
//   - empty deployments → renders nothing
//   - one paper deployment → row with VenueBadge + all four metric cells
//   - drawdown_pct: 20 → danger tone on drawdown cell
//   - negative running P&L → danger + ▼ glyph
//   - breached buffer (daily_loss_limit_remaining_usd: 0) → danger
//   - no popup / no dialog invariant
//   - simulated label present (honesty mandate)

import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";

import type { LiveDeploymentSummary } from "@/api/types.gen/LiveDeploymentSummary";
import { CapitalRiskStrip } from "./CapitalRiskStrip";

// ─── Fixture helper ───────────────────────────────────────────────────────────

function dep(overrides: Partial<LiveDeploymentSummary> = {}): LiveDeploymentSummary {
  return {
    deployment_id: "dep-001",
    strategy_id: "strat-001",
    strategy_name: "Alpha Strategy",
    venue_label: "paper",
    status: "running",
    paused: false,
    started_at: "2026-06-01T00:00:00Z",
    last_decision_at: "2026-06-13T10:00:00Z",
    deployed_capital_usd: 10000,
    equity_usd: 10500,
    realized_pnl_usd: 200,
    unrealized_pnl_usd: 300,
    realized_today_usd: 150,
    drawdown_pct: 2,
    daily_loss_limit_remaining_usd: 500,
    risk_veto_count: 0,
    daily_loss_budget_usd: null,
    stop_at: null,
    ...overrides,
  };
}

function renderStrip(deployments: LiveDeploymentSummary[]) {
  return render(
    <MemoryRouter>
      <CapitalRiskStrip deployments={deployments} />
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
});

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("CapitalRiskStrip", () => {
  it("renders nothing when deployments is empty", () => {
    const { container } = renderStrip([]);
    expect(container.firstChild).toBeNull();
  });

  it("renders a row for one paper deployment with a VenueBadge", () => {
    renderStrip([dep()]);
    const strip = screen.getByTestId("capital-risk-strip");
    expect(strip).toBeInTheDocument();
    // Per row data-testid
    expect(within(strip).getByTestId("capital-risk-row-dep-001")).toBeInTheDocument();
    // VenueBadge for paper
    expect(within(strip).getByTestId("venue-badge-paper")).toBeInTheDocument();
  });

  it("renders the strategy name or fallback dash", () => {
    renderStrip([dep({ strategy_name: "My Strategy" })]);
    expect(screen.getByTestId("capital-risk-strip").textContent).toContain("My Strategy");
  });

  it("renders '—' when strategy_name is null", () => {
    renderStrip([dep({ strategy_name: null })]);
    // The first — for strategy name should appear
    expect(screen.getByTestId("capital-risk-strip").textContent).toContain("—");
  });

  it("renders the four metric cells: deployed, drawdown, P&L, daily-loss buffer", () => {
    renderStrip([dep({
      deployed_capital_usd: 10000,
      drawdown_pct: 2,
      unrealized_pnl_usd: 300,
      realized_today_usd: 150,
      daily_loss_limit_remaining_usd: 500,
    })]);
    const strip = screen.getByTestId("capital-risk-strip");
    // deployed capital
    expect(strip.textContent).toContain("$10,000");
    // drawdown: 2% formatted as "2%"
    expect(strip.textContent).toMatch(/2%/);
    // P&L: 300 + 150 = 450
    expect(strip.textContent).toContain("$450");
    // daily-loss buffer
    expect(strip.textContent).toContain("$500");
  });

  it("shows a simulated label (honesty: paper/testnet is not real money)", () => {
    renderStrip([dep()]);
    const strip = screen.getByTestId("capital-risk-strip");
    expect(strip.textContent?.toLowerCase()).toMatch(/simulated|capital at risk/);
  });

  it("does NOT render a focus-stealing dialog/overlay (no popups rule)", () => {
    renderStrip([dep()]);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  // ─── Tone / danger signaling ──────────────────────────────────────────────

  it("marks drawdown cell danger when drawdown_pct is 20 (≥15 threshold)", () => {
    renderStrip([dep({ drawdown_pct: 20 })]);
    const row = screen.getByTestId("capital-risk-row-dep-001");
    const drawdownCell = within(row).getByTestId("drawdown-cell-dep-001");
    expect(drawdownCell.getAttribute("data-tone")).toBe("danger");
    // Must also show the ✗ glyph alongside the value (not color-only)
    expect(drawdownCell.textContent).toContain("✗");
  });

  it("marks drawdown cell gold when drawdown_pct is healthy (<5)", () => {
    renderStrip([dep({ drawdown_pct: 2 })]);
    const row = screen.getByTestId("capital-risk-row-dep-001");
    const drawdownCell = within(row).getByTestId("drawdown-cell-dep-001");
    expect(drawdownCell.getAttribute("data-tone")).toBe("gold");
    expect(drawdownCell.textContent).toContain("✓");
  });

  it("marks drawdown cell warn for 5–15% drawdown", () => {
    renderStrip([dep({ drawdown_pct: 10 })]);
    const row = screen.getByTestId("capital-risk-row-dep-001");
    const drawdownCell = within(row).getByTestId("drawdown-cell-dep-001");
    expect(drawdownCell.getAttribute("data-tone")).toBe("warn");
    expect(drawdownCell.textContent).toContain("⚠");
  });

  it("marks P&L cell danger and shows ▼ glyph when running P&L is negative", () => {
    renderStrip([dep({
      unrealized_pnl_usd: -800,
      realized_today_usd: -200,
    })]);
    const row = screen.getByTestId("capital-risk-row-dep-001");
    const pnlCell = within(row).getByTestId("pnl-cell-dep-001");
    expect(pnlCell.getAttribute("data-tone")).toBe("danger");
    expect(pnlCell.textContent).toContain("▼");
    // Negative sign in value
    expect(pnlCell.textContent).toContain("-$1,000");
  });

  it("marks P&L cell gold and shows ▲ glyph when running P&L is positive", () => {
    renderStrip([dep({
      unrealized_pnl_usd: 300,
      realized_today_usd: 150,
    })]);
    const row = screen.getByTestId("capital-risk-row-dep-001");
    const pnlCell = within(row).getByTestId("pnl-cell-dep-001");
    expect(pnlCell.getAttribute("data-tone")).toBe("gold");
    expect(pnlCell.textContent).toContain("▲");
  });

  it("marks buffer cell danger when daily_loss_limit_remaining_usd is 0 (breach)", () => {
    renderStrip([dep({ daily_loss_limit_remaining_usd: 0 })]);
    const row = screen.getByTestId("capital-risk-row-dep-001");
    const bufferCell = within(row).getByTestId("buffer-cell-dep-001");
    expect(bufferCell.getAttribute("data-tone")).toBe("danger");
    expect(bufferCell.textContent).toContain("✗");
  });

  it("marks buffer cell gold when daily_loss_limit_remaining_usd > 0", () => {
    renderStrip([dep({ daily_loss_limit_remaining_usd: 500 })]);
    const row = screen.getByTestId("capital-risk-row-dep-001");
    const bufferCell = within(row).getByTestId("buffer-cell-dep-001");
    expect(bufferCell.getAttribute("data-tone")).toBe("gold");
    expect(bufferCell.textContent).toContain("✓");
  });

  it("renders multiple deployments each with their own row", () => {
    const d1 = dep({ deployment_id: "dep-001", strategy_name: "Alpha" });
    const d2 = dep({ deployment_id: "dep-002", strategy_name: "Bravo", venue_label: "testnet" });
    renderStrip([d1, d2]);
    const strip = screen.getByTestId("capital-risk-strip");
    expect(within(strip).getByTestId("capital-risk-row-dep-001")).toBeInTheDocument();
    expect(within(strip).getByTestId("capital-risk-row-dep-002")).toBeInTheDocument();
    expect(strip.textContent).toContain("Alpha");
    expect(strip.textContent).toContain("Bravo");
  });

  it("renders neutral cells (—) when metric values are null", () => {
    renderStrip([dep({
      deployed_capital_usd: null,
      drawdown_pct: null,
      unrealized_pnl_usd: null,
      realized_today_usd: null,
      daily_loss_limit_remaining_usd: null,
    })]);
    // deployed shows —
    expect(screen.getByTestId("deployed-cell-dep-001").textContent).toContain("—");
    // drawdown shows —
    const drawdownCell = screen.getByTestId("drawdown-cell-dep-001");
    expect(drawdownCell.getAttribute("data-tone")).toBe("neutral");
    // pnl shows — (both null → neutral)
    expect(screen.getByTestId("pnl-cell-dep-001").getAttribute("data-tone")).toBe("neutral");
  });
});
