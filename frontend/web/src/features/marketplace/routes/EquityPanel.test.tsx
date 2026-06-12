// src/features/marketplace/routes/EquityPanel.test.tsx
//
// EquityPanel is now a thin delegator over PerformanceSection (catalogue
// overhaul §3.2B). These tests assert the new structure: a ChartFrame-wrapped
// equity pane when there is data, an inline MarkerDock when trades exist, and
// the designed empty state when there is neither equity nor trades — never a
// fabricated curve.
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi, beforeAll, afterAll } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { EquityPanel } from "./EquityPanel";
import type { EquityCurve, TradeRecord } from "@/features/marketplace/data/types";

// The empty-state path renders a <Link>, so wrap every render in a router.
function withRouter(ui: React.ReactElement) {
  return <MemoryRouter>{ui}</MemoryRouter>;
}

// Mock uPlot so tests don't need a DOM canvas environment
vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    destroy() {}
  },
}));

// Mock ResizeObserver — jsdom doesn't provide it, but the equity pane uses it
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
beforeAll(() => {
  Object.defineProperty(globalThis, "ResizeObserver", {
    writable: true,
    configurable: true,
    value: ResizeObserverStub,
  });
});
afterAll(() => {
  delete (globalThis as { ResizeObserver?: unknown }).ResizeObserver;
});

const CURVE: EquityCurve = {
  base: 1000,
  points: [
    ...Array.from({ length: 60 }, (_, i) => ({ value: 1000 + i * 5, phase: "backtest" as const })),
    ...Array.from({ length: 30 }, (_, i) => ({ value: 1300 + i * 3, phase: "live" as const })),
  ],
};

const TRADES: TradeRecord[] = [
  {
    at: "2026-05-26T12:30:00Z",
    action: "close",
    symbol: "BTC",
    qty: "0.024",
    entry: 67420,
    exit: 68840,
    pnlUsd: 34.08,
    pnlPct: 2.1,
    runner: "0x7c2e…aa07",
    runnerKind: "human",
    tx: "0xa83ef12d",
    anchorTx: "0x2e1d44a9",
  },
];

const EMPTY_CURVE: EquityCurve = { base: 1000, points: [] };

describe("EquityPanel", () => {
  it("renders the performance section with a chart when equity data exists", () => {
    render(withRouter(<EquityPanel curve={CURVE} />));
    expect(screen.getByTestId("performance-section")).toBeInTheDocument();
    // ChartFrame title
    expect(screen.getByText(/equity/i)).toBeInTheDocument();
    // No empty state when there is data
    expect(screen.queryByTestId("performance-empty")).not.toBeInTheDocument();
  });

  it("renders an inline MarkerDock when on-chain trades are supplied", () => {
    render(withRouter(<EquityPanel curve={CURVE} trades={TRADES} />));
    expect(screen.getByTestId("marker-dock")).toBeInTheDocument();
    expect(screen.getByText(/on-chain actuations/i)).toBeInTheDocument();
  });

  it("renders the designed empty state (no fake curve) when there is no record", () => {
    render(withRouter(<EquityPanel curve={EMPTY_CURVE} trades={[]} />));
    expect(screen.getByTestId("performance-empty")).toBeInTheDocument();
    expect(screen.getByText(/no live performance record yet/i)).toBeInTheDocument();
    expect(
      screen.getByText(/hasn't completed a trading cycle on-chain/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/run a backtest/i)).toBeInTheDocument();
    // No chart / no marker dock in the empty state
    expect(screen.queryByTestId("marker-dock")).not.toBeInTheDocument();
  });
});
