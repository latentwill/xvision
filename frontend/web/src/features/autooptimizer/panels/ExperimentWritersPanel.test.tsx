import { describe, expect, it, vi, afterEach, beforeAll, afterAll } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../test-utils";
import { ExperimentWritersPanel } from "./ExperimentWritersPanel";
import * as client from "@/api/client";

// Mock uPlot — WriterLadderChart uses uPlot; jsdom has no canvas
vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    destroy() {}
  },
}));

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
beforeAll(() => {
  Object.defineProperty(globalThis, "ResizeObserver", {
    writable: true,
    configurable: true,
    value: ResizeObserverStub,
  });
});
afterAll(() => {
  delete (globalThis as { ResizeObserver?: unknown }).ResizeObserver;
});

afterEach(() => vi.restoreAllMocks());

describe("ExperimentWritersPanel", () => {
  it("renders writer rows ranked, with operator labels", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      { provider: "anthropic", model: "claude-haiku-4-5", prompt_version: "v1",
        proposals: 10, accepted: 6, rejected_overfit: 4, avg_delta_sharpe: 0.18 },
    ]);
    renderWithProviders(<ExperimentWritersPanel />);
    expect(await screen.findByText("Experiment writers")).toBeInTheDocument();
    // Model name appears in both the table row and the WriterLadderChart legend
    await waitFor(() => expect(screen.getAllByText("claude-haiku-4-5").length).toBeGreaterThan(0));
    expect(screen.getByText("60%")).toBeInTheDocument(); // accept rate
  });

  it("expands a writer row to reveal real stats, not fabricated experiment links", async () => {
    vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/ladder")) return [
        { provider: "anthropic", model: "claude-haiku-4-5", prompt_version: "v1",
          proposals: 4, accepted: 2, rejected_overfit: 2, avg_delta_sharpe: 0.1 },
      ];
      return [];
    });
    const user = userEvent.setup();
    renderWithProviders(<ExperimentWritersPanel />);
    const row = await screen.findByRole("button", { name: /claude-haiku-4-5/ });
    await user.click(row);
    expect(await screen.findByText(/Phase 2/)).toBeInTheDocument();
    expect(screen.getByText("v1")).toBeInTheDocument();
    // Confirm the dl label is rendered — the value "2" appears multiple times (accepted + rejected_overfit)
    expect(screen.getByText("Rejected (overfit)")).toBeInTheDocument();
  });
});
