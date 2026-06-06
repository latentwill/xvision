import { describe, expect, it, vi, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { Routes, Route } from "react-router-dom";
import { renderWithProviders } from "../test-utils";
import { CycleDetail } from "./CycleDetail";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("CycleDetail", () => {
  it("renders the cycle hero, experiments table, and phase stubs", async () => {
    vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/cycles/")) {
        return { cycle_id: "cyc-1", node_count: 3, active_count: 1, rejected_count: 2,
                 first_created_at: "2026-06-01T00:00:00Z", last_created_at: "2026-06-01T01:00:00Z",
                 cost_usd: 4.2, input_tokens: 1000, output_tokens: 500, unpriced_calls: 0, nodes: [] };
      }
      if (url.includes("/lineage")) return [];
      if (url.includes("/health")) return { status: "ok", probes: [] };
      return {};
    });
    renderWithProviders(
      <Routes>
        <Route path="/optimizer/cycle/:cycleId" element={<CycleDetail />} />
      </Routes>,
      { route: "/optimizer/cycle/cyc-1" },
    );
    await waitFor(() =>
      expect(screen.getAllByRole("heading", { level: 1 }).some((h) => h.textContent === "cyc-1")).toBe(true)
    );
    expect(screen.getByText("Experiments this cycle")).toBeInTheDocument();
    expect(screen.getByText("Eval matrix")).toBeInTheDocument();         // EvalMatrix h2
    expect(screen.getByText("Anti-overfit gate")).toBeInTheDocument();   // GateBuckets h2
    // ProgressDial: kept rate = active_count/node_count = 1/3 ≈ 33%
    expect(await screen.findByText("33%")).toBeInTheDocument();
    expect(screen.getByText("KEPT")).toBeInTheDocument();
    // GateBuckets bucket labels (multiple "Kept"/"Dropped" instances across hero stats + GateBuckets)
    expect(screen.getAllByText("Kept").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Suspect").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Dropped").length).toBeGreaterThan(0);
  });
});
