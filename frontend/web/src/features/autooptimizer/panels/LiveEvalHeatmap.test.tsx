import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { LiveEvalHeatmap } from "./LiveEvalHeatmap";
import { HEATMAP_RUNNING, HEATMAP_IDLE, HEATMAP_EMPTY } from "./LiveEvalHeatmap.fixtures";

function cellStates(): string[] {
  return Array.from(document.querySelectorAll("[data-cell-state]")).map(
    (el) => el.getAttribute("data-cell-state") ?? "",
  );
}

describe("LiveEvalHeatmap", () => {
  it("renders experiments as rows and the regime union as columns", () => {
    renderWithProviders(<LiveEvalHeatmap nodes={HEATMAP_RUNNING} isRunning />);
    // 4 experiments × 5 regimes = 20 cells.
    expect(cellStates()).toHaveLength(20);
    // Each experiment row links to its detail page.
    expect(
      screen.getByRole("link", { name: /0xaaaa1111/ }),
    ).toHaveAttribute("href", "/optimizer/experiment/0xaaaa1111");
  });

  it("derives done cells where a regime result exists", () => {
    renderWithProviders(<LiveEvalHeatmap nodes={HEATMAP_RUNNING} isRunning />);
    // node aaaa is fully done (5), bbbb has 3 → 8 done minimum.
    const done = cellStates().filter((s) => s === "done").length;
    expect(done).toBe(5 + 3 + 1 + 0);
  });

  it("renders missing cells as testing while running (shimmer)", () => {
    renderWithProviders(<LiveEvalHeatmap nodes={HEATMAP_RUNNING} isRunning />);
    const states = cellStates();
    expect(states).toContain("testing");
    expect(states).not.toContain("queued");
  });

  it("renders missing cells as queued when idle", () => {
    renderWithProviders(<LiveEvalHeatmap nodes={HEATMAP_IDLE} isRunning={false} />);
    const states = cellStates();
    expect(states).toContain("queued");
    expect(states).not.toContain("testing");
  });

  it("shows the evals progress count", () => {
    renderWithProviders(<LiveEvalHeatmap nodes={HEATMAP_IDLE} isRunning={false} />);
    // 3 nodes × 5 regimes = 15 total; done = 5+4+2 = 11.
    expect(screen.getByText(/11 \/ 15 evals/)).toBeInTheDocument();
  });

  it("shows an empty state when there is no regime data", () => {
    renderWithProviders(<LiveEvalHeatmap nodes={HEATMAP_EMPTY} isRunning={false} />);
    expect(screen.getByText(/lights up when the optimizer runs/i)).toBeInTheDocument();
    expect(cellStates()).toHaveLength(0);
  });
});
