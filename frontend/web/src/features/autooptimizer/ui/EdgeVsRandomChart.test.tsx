import { describe, expect, it, vi, beforeAll, afterAll } from "vitest";
import { render, screen } from "@testing-library/react";
import type { StatsRow } from "../api";

// uPlot draws to canvas — mock it so jsdom doesn't throw.
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

import { EdgeVsRandomChart } from "./EdgeVsRandomChart";

function row(over: Partial<StatsRow> & { cycle_id: string }): StatsRow {
  return {
    session_id: "sess-1",
    ts: "2026-06-01T00:00:00Z",
    kept: 1,
    suspect: 0,
    dropped: 0,
    best_delta_holdout: null,
    cost_usd: 0,
    cum_cost_usd: 0,
    ...over,
  };
}

const FIXTURE: StatsRow[] = [
  row({ cycle_id: "c1", best_edge_over_random: 0.12, best_parent_edge: 0.08 }),
  row({ cycle_id: "c2", best_edge_over_random: 0.21, best_parent_edge: 0.15 }),
  row({ cycle_id: "c3", best_edge_over_random: -0.03, best_parent_edge: 0.11 }),
];

const NULL_ROWS: StatsRow[] = [
  row({ cycle_id: "c1", best_edge_over_random: null, best_parent_edge: null }),
];

describe("EdgeVsRandomChart", () => {
  it("renders the chart host div when there are data rows", () => {
    const { container } = render(<EdgeVsRandomChart rows={FIXTURE} />);
    expect(container.querySelector("[data-chart='edge-vs-random']")).toBeInTheDocument();
  });

  it("renders the plain-language explainer when data is present", () => {
    render(<EdgeVsRandomChart rows={FIXTURE} />);
    expect(screen.getByTestId("edge-vs-random-explainer")).toBeInTheDocument();
    expect(screen.getByTestId("edge-vs-random-explainer").textContent).toMatch(
      /above 0 means the optimizer beats chance/i,
    );
  });

  it("shows the explainer + honest empty state copy when there are no data rows", () => {
    render(<EdgeVsRandomChart rows={[]} />);
    // Explainer is still shown
    expect(screen.getByTestId("edge-vs-random-explainer")).toBeInTheDocument();
    // Honest empty state copy
    expect(screen.getByText(/Appears once cycles run with a baseline/i)).toBeInTheDocument();
    // No chart host rendered in the empty state
    expect(document.querySelector("[data-chart='edge-vs-random']")).toBeNull();
  });

  it("treats rows with all-null edge fields as no-data (shows empty state)", () => {
    render(<EdgeVsRandomChart rows={NULL_ROWS} />);
    expect(screen.getByText(/Appears once cycles run with a baseline/i)).toBeInTheDocument();
    expect(document.querySelector("[data-chart='edge-vs-random']")).toBeNull();
  });

  it("does not render the empty-state copy when data rows are present", () => {
    render(<EdgeVsRandomChart rows={FIXTURE} />);
    expect(screen.queryByText(/Appears once cycles run with a baseline/i)).toBeNull();
  });
});
