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
    expect(screen.getByText("Per-regime evaluation")).toBeInTheDocument(); // EmptyPanel stub
    expect(screen.getByText("Flight recorder")).toBeInTheDocument();        // EmptyPanel stub
  });
});
