import { describe, expect, it, vi, afterEach } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { RecentCyclesTable } from "./RecentCyclesTable";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("RecentCyclesTable", () => {
  it("links each cycle row to its detail screen", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      { cycle_id: "cyc-1", node_count: 5, active_count: 2, rejected_count: 3,
        first_created_at: "2026-06-01T00:00:00Z", last_created_at: "2026-06-01T01:00:00Z",
        cost_usd: 4.2, input_tokens: 1000, output_tokens: 500, unpriced_calls: 0 },
    ]);
    renderWithProviders(<RecentCyclesTable />);
    const link = await screen.findByRole("link", { name: /cyc-1/ });
    expect(link).toHaveAttribute("href", "/optimizer/cycle/cyc-1");
  });
});
