// src/features/marketplace/routes/PerformanceSection.provenance.test.tsx
//
// Asserts the provenance labeling added in plan §7 / Phase 8:
//  - "Indicative · backtest" label always renders (the equity curve is not a
//    real-money track record).
//  - "Live · Degen Arena (on-chain)" row renders ONLY when liveDegenPnlUsd is
//    non-null, and is hidden when null/undefined.
//  - The live row shows the formatted USD value with sign.
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi, beforeAll, afterAll } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { PerformanceSection } from "./PerformanceSection";
import type { EquityCurve, TradeRecord } from "@/features/marketplace/data/types";

// Wrap in router so <Link> inside empty-state renders.
function withRouter(ui: React.ReactElement) {
  return <MemoryRouter>{ui}</MemoryRouter>;
}

// Mock uPlot — tests don't need a canvas environment.
vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    destroy() {}
  },
}));

// Stub ResizeObserver — jsdom doesn't provide it.
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

const EMPTY_CURVE: EquityCurve = { base: 1000, points: [] };
const EMPTY_TRADES: TradeRecord[] = [];

const CURVE_WITH_DATA: EquityCurve = {
  base: 1000,
  points: Array.from({ length: 30 }, (_, i) => ({
    value: 1000 + i * 8,
    phase: "backtest" as const,
  })),
};

describe("PerformanceSection — provenance labeling", () => {
  it("always renders the 'Indicative · backtest' label on the provenance banner", () => {
    render(
      withRouter(
        <PerformanceSection
          curve={EMPTY_CURVE}
          trades={EMPTY_TRADES}
          liveDegenPnlUsd={null}
        />,
      ),
    );
    expect(screen.getByTestId("provenance-banner")).toBeInTheDocument();
    expect(screen.getByTestId("provenance-indicative")).toBeInTheDocument();
    expect(screen.getByText(/indicative\s*·\s*backtest/i)).toBeInTheDocument();
  });

  it("hides the live Degen Arena row when liveDegenPnlUsd is null", () => {
    render(
      withRouter(
        <PerformanceSection
          curve={EMPTY_CURVE}
          trades={EMPTY_TRADES}
          liveDegenPnlUsd={null}
        />,
      ),
    );
    expect(screen.queryByTestId("provenance-live")).not.toBeInTheDocument();
    expect(screen.queryByText(/live\s*·\s*degen arena/i)).not.toBeInTheDocument();
  });

  it("hides the live Degen Arena row when liveDegenPnlUsd is undefined (default)", () => {
    render(
      withRouter(
        <PerformanceSection curve={EMPTY_CURVE} trades={EMPTY_TRADES} />,
      ),
    );
    expect(screen.queryByTestId("provenance-live")).not.toBeInTheDocument();
  });

  it("renders the live 'Live · Degen Arena (on-chain)' row when liveDegenPnlUsd is provided", () => {
    render(
      withRouter(
        <PerformanceSection
          curve={EMPTY_CURVE}
          trades={EMPTY_TRADES}
          liveDegenPnlUsd={1234.56}
        />,
      ),
    );
    expect(screen.getByTestId("provenance-live")).toBeInTheDocument();
    expect(screen.getByText(/live\s*·\s*degen arena \(on-chain\)/i)).toBeInTheDocument();
    // Both labels co-exist in the banner
    expect(screen.getByTestId("provenance-indicative")).toBeInTheDocument();
  });

  it("renders the live Degen Arena PnL value with authoritative label", () => {
    render(
      withRouter(
        <PerformanceSection
          curve={EMPTY_CURVE}
          trades={EMPTY_TRADES}
          liveDegenPnlUsd={1234.56}
        />,
      ),
    );
    // The formatted amount includes "$1,234.56"
    expect(screen.getByText(/\$1,234\.56/)).toBeInTheDocument();
    expect(screen.getByText(/authoritative/i)).toBeInTheDocument();
  });

  it("renders a negative live PnL without '+' prefix", () => {
    render(
      withRouter(
        <PerformanceSection
          curve={EMPTY_CURVE}
          trades={EMPTY_TRADES}
          liveDegenPnlUsd={-88.5}
        />,
      ),
    );
    expect(screen.getByText(/-\$88\.50/)).toBeInTheDocument();
  });

  it("renders provenance banner in the chart/data path (not just empty state)", () => {
    render(
      withRouter(
        <PerformanceSection
          curve={CURVE_WITH_DATA}
          trades={EMPTY_TRADES}
          liveDegenPnlUsd={null}
        />,
      ),
    );
    // Still shows the banner even when there is chart data
    expect(screen.getByTestId("provenance-banner")).toBeInTheDocument();
    expect(screen.getByTestId("provenance-indicative")).toBeInTheDocument();
    // Still not showing live row
    expect(screen.queryByTestId("provenance-live")).not.toBeInTheDocument();
  });

  it("renders both labels when equity data exists AND liveDegenPnlUsd is provided", () => {
    render(
      withRouter(
        <PerformanceSection
          curve={CURVE_WITH_DATA}
          trades={EMPTY_TRADES}
          liveDegenPnlUsd={500}
        />,
      ),
    );
    expect(screen.getByTestId("provenance-live")).toBeInTheDocument();
    expect(screen.getByTestId("provenance-indicative")).toBeInTheDocument();
    expect(screen.getByText(/live\s*·\s*degen arena/i)).toBeInTheDocument();
    expect(screen.getByText(/indicative\s*·\s*backtest/i)).toBeInTheDocument();
  });
});
