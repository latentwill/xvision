import { describe, expect, it, vi, afterEach } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { LineageTreePanel } from "./LineageTreePanel";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("LineageTreePanel", () => {
  it("renders a parent and its child indented, each linking to the experiment screen", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      { bundle_hash: "parent00", parent_hash: null, gate_verdict: "Pass", status: "active", cycle_id: "cyc-1", created_at: "2026-06-01T00:00:00Z" },
      { bundle_hash: "child0001", parent_hash: "parent00", gate_verdict: "Fail", status: "rejected", cycle_id: "cyc-1", created_at: "2026-06-01T00:05:00Z" },
    ]);
    renderWithProviders(<LineageTreePanel cycleId="cyc-1" />);
    expect(await screen.findByText("Lineage tree")).toBeInTheDocument();
    const parentLink = await screen.findByRole("link", { name: /parent00/ });
    expect(parentLink).toHaveAttribute("href", "/optimizer/experiment/parent00");
    const childLink = await screen.findByRole("link", { name: /child0001/ });
    expect(childLink).toHaveAttribute("href", "/optimizer/experiment/child0001");
  });
});
