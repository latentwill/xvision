import { describe, expect, it, vi, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { Routes, Route } from "react-router-dom";
import { renderWithProviders } from "../test-utils";
import { ExperimentDetail } from "./ExperimentDetail";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("ExperimentDetail", () => {
  it("renders the experiment hero, diff panel, and phase stubs", async () => {
    vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/lineage/")) {
        return { bundle_hash: "deadbeefcafe", parent_hash: "0000", gate_verdict: "Pass",
                 status: "active", cycle_id: "cyc-1", created_at: "2026-06-01T00:00:00Z" };
      }
      if (url.includes("/cycles/")) {
        return {
          cycle_id: "cyc-1", node_count: 1, active_count: 1, suspect_count: 0, rejected_count: 0,
          first_created_at: "2026-06-01T00:00:00Z", last_created_at: "2026-06-01T01:00:00Z",
          cost_usd: 1, input_tokens: 100, output_tokens: 50, unpriced_calls: 0,
          nodes: [{ bundle_hash: "deadbeefcafe", parent_hash: null, gate_verdict: "Pass",
                    status: "active", cycle_id: "cyc-1", created_at: "2026-06-01T00:00:00Z",
                    regime_results: [{
                      regime_label: "bull", side: "bull", delta_sharpe: 0.22, verdict: "passed",
                      metrics_day: { total_return_pct: 10, sharpe: 1.3, max_drawdown_pct: 3, win_rate: 0.6, n_trades: 12 },
                      metrics_untouched: { total_return_pct: 9, sharpe: 1.2, max_drawdown_pct: 3.5, win_rate: 0.58, n_trades: 11 },
                    }] }],
        };
      }
      if (url.includes("/health")) return { status: "ok", probes: [] };
      return {}; // blobs
    });
    renderWithProviders(
      <Routes>
        <Route path="/optimizer/experiment/:hash" element={<ExperimentDetail />} />
      </Routes>,
      { route: "/optimizer/experiment/deadbeefcafe" },
    );
    await waitFor(() => {
      const headings = screen.getAllByRole("heading", { level: 1 });
      expect(headings.some((h) => /deadbeef/.test(h.textContent ?? ""))).toBe(true);
    });
    expect(screen.getByText("What this experiment changed")).toBeInTheDocument();
    expect(screen.getByText("Per-regime evaluation")).toBeInTheDocument(); // RegimeCards h2
    expect(screen.getByText("Flight recorder")).toBeInTheDocument();        // EmptyPanel stub
    // RegimeCards: regime label and Δ-Sharpe
    expect(await screen.findByText("bull")).toBeInTheDocument();
    expect(await screen.findByText("+0.22")).toBeInTheDocument();
  });
});
