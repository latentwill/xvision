import { describe, expect, it, vi, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
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
});
