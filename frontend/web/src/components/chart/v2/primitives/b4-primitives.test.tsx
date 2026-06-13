/**
 * Tests for B4 primitives + the GradientHeroDashboard helpers.
 * Canvas-heavy bits (HeroGradientEquity / UplotDrawdownPane) are not
 * exercised here; chart-lab/dashboards/hero is the visual review.
 */
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";

import { AuraBackground } from "./AuraBackground";
import { GrainOverlay } from "./GrainOverlay";
import { GlassCard } from "./GlassCard";
import { GradientHeadline } from "./GradientHeadline";
import {
  PerformanceRadar,
  polygonPoints,
  ringPoints,
  RADAR_DIMENSIONS,
  RADAR_AXIS_LABELS,
} from "./PerformanceRadar";
import { MarketContextCard } from "./MarketContextCard";
import { strategyToRadar } from "../surfaces/GradientHeroDashboard";
import type { MultiStrategyBundleEntry } from "../types";

describe("AuraBackground", () => {
  it("renders three positioned washes by default", () => {
    const { container } = render(<AuraBackground />);
    const root = container.querySelector('[data-testid="aura-background"]');
    expect(root).toBeInTheDocument();
    expect(root!.children).toHaveLength(3);
  });
  it("renders nothing when disabled", () => {
    const { container } = render(<AuraBackground disabled />);
    expect(container.querySelector('[data-testid="aura-background"]')).toBeNull();
  });
});

describe("GrainOverlay", () => {
  it("renders the full-bleed grain div", () => {
    render(<GrainOverlay />);
    expect(screen.getByTestId("grain-overlay")).toBeInTheDocument();
  });
});

describe("GlassCard", () => {
  it("wraps children inside the glass chrome", () => {
    render(
      <GlassCard>
        <span data-testid="inner">x</span>
      </GlassCard>,
    );
    expect(screen.getByTestId("glass-card")).toBeInTheDocument();
    expect(screen.getByTestId("inner")).toBeInTheDocument();
  });
});

describe("GradientHeadline", () => {
  it("renders prefix + bracketed + suffix + emphasis", () => {
    render(
      <GradientHeadline
        prefix="The"
        bracketed="Golden Cross"
        suffix="is up"
        emphasis="+82.41%"
      />,
    );
    expect(screen.getByText(/The/)).toBeInTheDocument();
    expect(screen.getByText("Golden Cross")).toBeInTheDocument();
    expect(screen.getByText("+82.41%")).toBeInTheDocument();
  });

  it("renders a space before the emphasis span so text reads naturally", () => {
    const { container } = render(
      <GradientHeadline
        prefix="The"
        bracketed="Golden Cross"
        suffix="is up"
        emphasis="1.02%"
      />,
    );
    const h1 = container.querySelector("h1");
    // The full text content should not have "up1.02%" with no space.
    expect(h1?.textContent).not.toMatch(/up1\.02%/);
    expect(h1?.textContent).toMatch(/is up/);
    expect(h1?.textContent).toMatch(/1\.02%/);
  });

  // W3: sign-aware suffix — "is up" for positive, "is down" for negative.
  // This tests that the GradientHeroDashboard wires the suffix correctly;
  // GradientHeadline itself is sign-agnostic and just renders what it receives.
  it("accepts 'is down' as suffix for negative returns", () => {
    render(
      <GradientHeadline
        prefix="The"
        bracketed="Golden Cross"
        suffix="is down"
        emphasis="1.02%"
      />,
    );
    expect(screen.getByText(/is down/)).toBeInTheDocument();
    expect(screen.getByText("1.02%")).toBeInTheDocument();
    // Emphasis must NOT repeat the sign — "is down" already carries direction.
    expect(screen.queryByText("-1.02%")).not.toBeInTheDocument();
  });
});

describe("PerformanceRadar geometry helpers", () => {
  it("polygonPoints returns N coordinate pairs for a length-N input", () => {
    const s = polygonPoints([0.5, 0.5, 0.5, 0.5, 0.5, 0.5], 100, 100, 50);
    const pairs = s.split(" ").filter(Boolean);
    expect(pairs).toHaveLength(RADAR_DIMENSIONS);
  });
  it("polygonPoints first vertex sits straight up (cy - r) for an all-1 vector", () => {
    const s = polygonPoints([1, 1, 1, 1, 1, 1], 100, 100, 50);
    const first = s.split(" ")[0].split(",").map(Number);
    expect(first[0]).toBeCloseTo(100, 1);
    expect(first[1]).toBeCloseTo(50, 1); // cy - r
  });
  it("ringPoints returns the unit polygon scaled by r", () => {
    expect(ringPoints(1, 100, 100, 50)).toBe(
      polygonPoints([1, 1, 1, 1, 1, 1], 100, 100, 50),
    );
  });

  it("renders the SVG with axis labels", () => {
    render(
      <PerformanceRadar
        strategies={[
          {
            id: "fib",
            label: "Fib · GC",
            color: "#D4A547",
            values: [0.8, 0.6, 0.5, 0.6, 0.9, 0.5],
          },
        ]}
      />,
    );
    expect(screen.getByTestId("performance-radar")).toBeInTheDocument();
    for (const label of RADAR_AXIS_LABELS) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
  });
});

describe("MarketContextCard", () => {
  it("renders 2×2 stats + regime chips", () => {
    render(
      <MarketContextCard
        data={{
          price: 65128.4,
          fundingPct: 0.012,
          openInterestUsd: 7_450_000_000,
          liq24hUsd: 84_000_000,
        }}
        regimes={[
          { label: "BULL", pct: 62 },
          { label: "SIDEWAYS", pct: 22 },
        ]}
      />,
    );
    expect(screen.getByText("Market Context · BTC")).toBeInTheDocument();
    expect(screen.getByText("Price")).toBeInTheDocument();
    expect(screen.getByText(/BULL · 62%/)).toBeInTheDocument();
    expect(screen.getByText(/SIDEWAYS · 22%/)).toBeInTheDocument();
  });

  it("renders the full 4-regime payload from the backend stub shape", () => {
    // Verifies the card works unchanged against the MarketContextPayload
    // shape returned by GET /api/v2/charts/market-context.
    const data = {
      price: 65_128.4,
      fundingPct: 0.012,
      openInterestUsd: 7_450_000_000,
      liq24hUsd: 84_000_000,
    };
    const regimes = [
      { label: "BULL", pct: 62 },
      { label: "SIDEWAYS", pct: 22 },
      { label: "BEAR", pct: 9 },
      { label: "HIGH VOL", pct: 7 },
    ];
    render(<MarketContextCard data={data} regimes={regimes} />);
    expect(screen.getByText(/BEAR · 9%/)).toBeInTheDocument();
    expect(screen.getByText(/HIGH VOL · 7%/)).toBeInTheDocument();
    // OI renders as $7.45B
    expect(screen.getByText("$7.45B")).toBeInTheDocument();
    // Liq renders as $84.0M
    expect(screen.getByText("$84.0M")).toBeInTheDocument();
  });
});

describe("strategyToRadar normalisation", () => {
  function entry(partial: Partial<MultiStrategyBundleEntry["metrics"]>): MultiStrategyBundleEntry {
    return {
      id: "x",
      name: "x",
      short: "x",
      color: "#000",
      kind: "Trend",
      equity: [],
      drawdown: [],
      monthly: [],
      metrics: { return: 0, sharpe: 0, mdd: 0, win: 0, pf: 0, ...partial },
    };
  }
  it("clamps to [0,1]", () => {
    const v = strategyToRadar(entry({ return: 1000, sharpe: 999, mdd: -100, win: 200, pf: 99 }));
    for (const x of v) {
      expect(x).toBeGreaterThanOrEqual(0);
      expect(x).toBeLessThanOrEqual(1);
    }
  });
  it("returns 6 values", () => {
    expect(strategyToRadar(entry({ return: 50 }))).toHaveLength(RADAR_DIMENSIONS);
  });
  it("zero-metrics map to mid-range return + zero everywhere else", () => {
    const v = strategyToRadar(entry({}));
    // Return: (0 + 50) / 200 = 0.25
    expect(v[0]).toBeCloseTo(0.25, 5);
    expect(v[1]).toBe(0); // sharpe
    expect(v[3]).toBe(0); // win
    expect(v[4]).toBe(0); // consistency
    // Stability + drawdown both = 1 (no drawdown).
    expect(v[2]).toBeCloseTo(1, 5);
    expect(v[5]).toBeCloseTo(1, 5);
  });
});
