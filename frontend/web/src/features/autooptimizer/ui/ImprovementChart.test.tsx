import { describe, expect, it, vi, beforeAll, afterAll } from "vitest";
import { render, screen } from "@testing-library/react";
import type { StatsRow } from "../api";

// Mock uPlot — tests run in jsdom which has no canvas
vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    destroy() {}
  },
}));

// Stub ResizeObserver
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

// Import after mocks are installed
import { ImprovementChart } from "./ImprovementChart";

const FIXTURE: StatsRow[] = [
  {
    cycle_id: "cyc-1",
    session_id: "sess-1",
    ts: "2026-06-01T10:00:00Z",
    kept: 1,
    suspect: 0,
    dropped: 2,
    best_delta_holdout: 0.05,
    cost_usd: 0.12,
    cum_cost_usd: 0.12,
  },
  {
    cycle_id: "cyc-2",
    session_id: "sess-1",
    ts: "2026-06-01T10:05:00Z",
    kept: 2,
    suspect: 1,
    dropped: 1,
    best_delta_holdout: 0.09,
    cost_usd: 0.08,
    cum_cost_usd: 0.20,
  },
];

describe("ImprovementChart", () => {
  it("renders the chart wrapper when rows have data", () => {
    const { container } = render(<ImprovementChart rows={FIXTURE} />);
    // The chart host div should be present
    expect(container.querySelector("[data-chart='improvement']")).toBeInTheDocument();
  });

  it("shows the empty-state message when rows is empty", () => {
    render(<ImprovementChart rows={[]} />);
    expect(
      screen.getByText(/start an optimizer run to see improvement over time/i),
    ).toBeInTheDocument();
  });

  it("shows the empty-state message when all best_delta_holdout values are null", () => {
    const nullRows: StatsRow[] = [
      { ...FIXTURE[0], best_delta_holdout: null },
      { ...FIXTURE[1], best_delta_holdout: null },
    ];
    render(<ImprovementChart rows={nullRows} />);
    expect(
      screen.getByText(/start an optimizer run to see improvement over time/i),
    ).toBeInTheDocument();
  });

  it("accepts an optional sessionId prop without crashing", () => {
    const { container } = render(
      <ImprovementChart rows={FIXTURE} sessionId="sess-1" />,
    );
    expect(container.querySelector("[data-chart='improvement']")).toBeInTheDocument();
  });
});
