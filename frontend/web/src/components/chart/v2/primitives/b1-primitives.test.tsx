/**
 * Unit tests for the B1 dashboard primitives that have testable pure
 * logic or simple DOM render contracts. uPlot canvas snapshotting is
 * intentionally skipped (brittle across browsers); the
 * MultiStrategyEquityPane helpers (`buildAlignedData`,
 * `resolveLeadIndex`) and the MonthlyReturnsHeatmap `cellAlpha`
 * computation carry the load instead.
 */
import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";

// uPlot calls `matchMedia` at module-load time (sets up the DPR change
// listener). jsdom does not ship `matchMedia`, so importing
// MultiStrategyEquityPane below would fail outright. Polyfill before
// any uPlot import resolves — `vi.hoisted` runs before module ESM
// initialization, which is critical here.
// UplotDrawdownPane (used by DrawdownCard) instantiates uPlot, which
// hits jsdom's incomplete canvas implementation. We're testing the
// card's chrome + footer formatting, not the canvas pane, so stub it.
vi.mock("./UplotDrawdownPane", () => ({
  UplotDrawdownPane: (_props: unknown) => null,
}));

import { cellAlpha, MonthlyReturnsHeatmap } from "./MonthlyReturnsHeatmap";
import { KpiCard, KpiRow } from "./KpiCard";
import { ChartsTopbar } from "./Topbar";
import { DrawdownCard, formatPct } from "./DrawdownCard";
import {
  buildAlignedData,
  resolveLeadIndex,
  type MultiStrategyEquitySeries,
} from "./MultiStrategyEquityPane";

describe("cellAlpha — heatmap intensity scaling", () => {
  it("returns minAlpha at zero magnitude", () => {
    expect(cellAlpha(0, 0.15, 0.10, 0.65)).toBeCloseTo(0.10, 6);
  });
  it("returns maxAlpha at the ceiling", () => {
    expect(cellAlpha(0.15, 0.15, 0.10, 0.65)).toBeCloseTo(0.65, 6);
    expect(cellAlpha(-0.15, 0.15, 0.10, 0.65)).toBeCloseTo(0.65, 6);
  });
  it("clamps above the ceiling", () => {
    expect(cellAlpha(0.20, 0.15, 0.10, 0.65)).toBeCloseTo(0.65, 6);
    expect(cellAlpha(-100, 0.15, 0.10, 0.65)).toBeCloseTo(0.65, 6);
  });
  it("interpolates linearly mid-range", () => {
    // value=0.075 is exactly half of 0.15 → halfway between min and max
    const mid = cellAlpha(0.075, 0.15, 0.10, 0.65);
    expect(mid).toBeCloseTo((0.10 + 0.65) / 2, 6);
  });
  it("returns minAlpha for non-finite input", () => {
    expect(cellAlpha(NaN, 0.15, 0.10, 0.65)).toBeCloseTo(0.10, 6);
    expect(cellAlpha(Infinity, 0.15, 0.10, 0.65)).toBeCloseTo(0.10, 6);
  });
});

describe("MonthlyReturnsHeatmap", () => {
  const rows = [
    {
      id: "fib",
      label: "Fibonacci GC",
      cells: [
        { year: 2024, month: 1, value: 0.05 },
        { year: 2024, month: 2, value: -0.03 },
      ],
    },
    {
      id: "ema",
      label: "EMA Pullback",
      cells: [
        { year: 2024, month: 1, value: 0.02 },
        { year: 2024, month: 2, value: 0.0 },
      ],
    },
  ];

  it("renders title + N strategy labels + month headers", () => {
    render(<MonthlyReturnsHeatmap rows={rows} />);
    expect(screen.getByText("Monthly Returns")).toBeInTheDocument();
    expect(screen.getByText("Fibonacci GC")).toBeInTheDocument();
    expect(screen.getByText("EMA Pullback")).toBeInTheDocument();
    expect(screen.getByText("Jan '24")).toBeInTheDocument();
    expect(screen.getByText("Feb '24")).toBeInTheDocument();
  });

  it("renders cells with semantic role=cell", () => {
    render(<MonthlyReturnsHeatmap rows={rows} />);
    expect(screen.getAllByRole("cell")).toHaveLength(4);
  });
});

describe("KpiCard", () => {
  it("renders label, value, and foot", () => {
    render(
      <KpiCard label="Total Return" value="+82.41%" foot="vs +12.4% benchmark" />,
    );
    expect(screen.getByText("Total Return")).toBeInTheDocument();
    expect(screen.getByText("+82.41%")).toBeInTheDocument();
    expect(screen.getByText("vs +12.4% benchmark")).toBeInTheDocument();
  });
  it("applies danger intent for negative-rendering KPIs", () => {
    render(<KpiCard label="Max DD" value="-18.72%" intent="danger" />);
    const v = screen.getByText("-18.72%");
    expect(v.className).toMatch(/text-danger/);
  });
  it("draws cornerGlow=gold when set", () => {
    const { container } = render(
      <KpiCard label="Total Return" value="+82.41%" cornerGlow="gold" />,
    );
    const glow = container.querySelector('[aria-hidden="true"]') as HTMLElement;
    expect(glow).toBeInTheDocument();
    expect(glow.style.background).toMatch(/radial-gradient/);
  });

  it("KpiRow renders all children in a single grid wrapper", () => {
    const { container } = render(
      <KpiRow>
        <KpiCard label="A" value="1" />
        <KpiCard label="B" value="2" />
      </KpiRow>,
    );
    expect(container.querySelectorAll(".caps")).toHaveLength(2);
  });
});

describe("ChartsTopbar", () => {
  it("renders eyebrow + headline + tagline + actions", () => {
    render(
      <ChartsTopbar
        eyebrow="CRYPTO · STRATEGIES"
        headline="Strategy Comparison"
        tagline="Five strategies, one frame."
        actions={<span data-testid="action">act</span>}
      />,
    );
    expect(screen.getByText("CRYPTO · STRATEGIES")).toBeInTheDocument();
    expect(screen.getByText("Strategy Comparison")).toBeInTheDocument();
    expect(screen.getByText("Five strategies, one frame.")).toBeInTheDocument();
    expect(screen.getByTestId("action")).toBeInTheDocument();
  });
  it("omits eyebrow + tagline when not provided", () => {
    render(<ChartsTopbar headline="Just a title" />);
    expect(screen.getByText("Just a title")).toBeInTheDocument();
    expect(screen.queryByText("CRYPTO · STRATEGIES")).toBeNull();
  });
});

describe("DrawdownCard helpers", () => {
  it("formats positive/zero/negative percentages with explicit sign", () => {
    expect(formatPct(0)).toBe("0.00%");
    expect(formatPct(1.234)).toBe("+1.23%");
    expect(formatPct(-18.7)).toBe("-18.70%");
  });

  it("renders the title and 4 footer stats", () => {
    render(
      <DrawdownCard
        title="Drawdown · Fib GC"
        points={[
          { time: 1, value: 0 },
          { time: 2, value: -1.5 },
          { time: 3, value: -2.3 },
        ]}
        stats={{
          maxDrawdownPct: -18.72,
          avgDrawdownPct: -4.4,
          durationDays: 42,
          recoveryDays: 18,
        }}
      />,
    );
    expect(screen.getByText("Drawdown · Fib GC")).toBeInTheDocument();
    expect(screen.getByText("-18.72%")).toBeInTheDocument();
    expect(screen.getByText("-4.40%")).toBeInTheDocument();
    expect(screen.getByText("42d")).toBeInTheDocument();
    expect(screen.getByText("18d")).toBeInTheDocument();
  });

  it("renders Recovery as em-dash when null", () => {
    render(
      <DrawdownCard
        points={[]}
        stats={{
          maxDrawdownPct: -10,
          avgDrawdownPct: -3,
          durationDays: 5,
          recoveryDays: null,
        }}
      />,
    );
    expect(screen.getByText("—")).toBeInTheDocument();
  });
});

describe("MultiStrategyEquityPane helpers", () => {
  const series: MultiStrategyEquitySeries[] = [
    { id: "fib", label: "Fib · GC", values: [0, 5, 10], color: "#D4A547" },
    { id: "ema", label: "EMA", values: [0, 2, 4], color: "#E8DCB0" },
    { id: "btc", label: "BTC HOLD", values: [0, -1, -2], color: "#6B6553", dashed: true },
  ];

  it("buildAlignedData prepends the time array, then each series.values", () => {
    const data = buildAlignedData([100, 200, 300], series);
    expect(data[0]).toEqual([100, 200, 300]);
    expect(data[1]).toEqual([0, 5, 10]);
    expect(data[3]).toEqual([0, -1, -2]);
  });

  it("resolveLeadIndex defaults to 0 when leadId is absent or unknown", () => {
    expect(resolveLeadIndex(series)).toBe(0);
    expect(resolveLeadIndex(series, "missing")).toBe(0);
  });

  it("resolveLeadIndex returns the index matching leadId", () => {
    expect(resolveLeadIndex(series, "ema")).toBe(1);
    expect(resolveLeadIndex(series, "btc")).toBe(2);
  });

  it("resolveLeadIndex safely returns 0 on empty series", () => {
    expect(resolveLeadIndex([], "anything")).toBe(0);
  });
});
