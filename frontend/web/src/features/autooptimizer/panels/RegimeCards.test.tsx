import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { RegimeCards } from "./RegimeCards";

const r = (label: string, delta: number) => ({
  regime_label: label, side: "bull" as const, delta_sharpe: delta, verdict: "passed",
  metrics_day: { total_return_pct: 12.3, sharpe: 1.4, max_drawdown_pct: 4.5, win_rate: 0.62, n_trades: 18 },
  metrics_untouched: { total_return_pct: 10, sharpe: 1.2, max_drawdown_pct: 5, win_rate: 0.6, n_trades: 16 },
});

describe("RegimeCards", () => {
  it("renders a card per regime with delta and micro-metrics", () => {
    render(<RegimeCards results={[r("bull", 0.22), r("bear", -0.1)]} />);
    expect(screen.getByText("Per-regime evaluation")).toBeInTheDocument();
    expect(screen.getByText("bull")).toBeInTheDocument();
    expect(screen.getByText("bear")).toBeInTheDocument();
    expect(screen.getByText("+0.22")).toBeInTheDocument();
    expect(screen.getByText("-0.10")).toBeInTheDocument();
    expect(screen.getByText("62%")).toBeInTheDocument(); // win_rate 0.62 -> 62%
  });
  it("shows empty state when no results", () => {
    render(<RegimeCards results={[]} />);
    expect(screen.getByText(/Lights up when/i)).toBeInTheDocument();
  });
});
