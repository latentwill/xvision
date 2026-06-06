import { describe, expect, it } from "vitest";
import { renderWithProviders } from "../test-utils";
import { screen } from "@testing-library/react";
import { EvalMatrix } from "./EvalMatrix";

const node = (hash: string, regimes: Array<{label: string; delta: number}>) => ({
  bundle_hash: hash, parent_hash: null, gate_verdict: "Pass", status: "active",
  cycle_id: "cyc-1", created_at: "2026-06-01T00:00:00Z",
  regime_results: regimes.map((r) => ({
    regime_label: r.label, side: "bull", delta_sharpe: r.delta, verdict: "passed",
    metrics_day: { total_return_pct: 1, sharpe: 1.2, max_drawdown_pct: 2, win_rate: 0.5, n_trades: 10 },
    metrics_untouched: { total_return_pct: 1, sharpe: 1.1, max_drawdown_pct: 2, win_rate: 0.5, n_trades: 9 },
  })),
});

describe("EvalMatrix", () => {
  it("renders experiments as rows and regimes as columns with delta cells", () => {
    renderWithProviders(<EvalMatrix nodes={[node("aaaa1111", [{label:"bull", delta:0.22}, {label:"bear", delta:-0.1}]) as any]} />);
    expect(screen.getByText("Eval matrix")).toBeInTheDocument();
    expect(screen.getByText("bull")).toBeInTheDocument();
    expect(screen.getByText("bear")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /aaaa1111/ })).toHaveAttribute("href", "/optimizer/experiment/aaaa1111");
    expect(screen.getByText("+0.22")).toBeInTheDocument();
  });
  it("shows an empty state when no regime results exist", () => {
    renderWithProviders(<EvalMatrix nodes={[node("bbbb2222", []) as any]} />);
    expect(screen.getByText(/Lights up when/i)).toBeInTheDocument();
  });
});
