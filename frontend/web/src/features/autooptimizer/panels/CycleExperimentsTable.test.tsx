import { describe, expect, it, vi, afterEach } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { CycleExperimentsTable } from "./CycleExperimentsTable";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("CycleExperimentsTable", () => {
  it("lists experiments for the cycle with a link + Kept badge", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      { bundle_hash: "deadbeefcafe", parent_hash: "0000", gate_verdict: "Pass",
        status: "active", cycle_id: "cyc-1", created_at: "2026-06-01T00:00:00Z",
        diversity_score: 0.24 },
    ]);
    renderWithProviders(<CycleExperimentsTable cycleId="cyc-1" />);
    const link = await screen.findByRole("link", { name: /deadbeef/ });
    expect(link).toHaveAttribute("href", "/optimizer/experiment/deadbeefcafe");
    expect(screen.getByText("Kept")).toBeInTheDocument();
  });
});
