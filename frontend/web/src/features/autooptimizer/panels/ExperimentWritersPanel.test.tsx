import { describe, expect, it, vi, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../test-utils";
import { ExperimentWritersPanel } from "./ExperimentWritersPanel";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("ExperimentWritersPanel", () => {
  it("renders writer rows ranked, with operator labels", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      { provider: "anthropic", model: "claude-haiku-4-5", prompt_version: "v1",
        proposals: 10, accepted: 6, rejected_overfit: 4, avg_delta_sharpe: 0.18 },
    ]);
    renderWithProviders(<ExperimentWritersPanel />);
    expect(await screen.findByText("Experiment writers")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("claude-haiku-4-5")).toBeInTheDocument());
    expect(screen.getByText("60%")).toBeInTheDocument(); // accept rate
  });

  it("expands a writer row to reveal its recent experiments", async () => {
    vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/ladder")) return [
        { provider: "anthropic", model: "claude-haiku-4-5", prompt_version: "v1", proposals: 4, accepted: 2, rejected_overfit: 2, avg_delta_sharpe: 0.1 },
      ];
      if (url.includes("/lineage")) return [
        { bundle_hash: "exp0000001", parent_hash: null, gate_verdict: "Pass", status: "active", cycle_id: "cyc-1", created_at: "2026-06-01T00:00:00Z" },
      ];
      return [];
    });
    const user = userEvent.setup();
    renderWithProviders(<ExperimentWritersPanel />);
    const row = await screen.findByRole("button", { name: /claude-haiku-4-5/ });
    await user.click(row);
    expect(await screen.findByRole("link", { name: /exp0000001/ })).toHaveAttribute("href", "/optimizer/experiment/exp0000001");
  });
});
