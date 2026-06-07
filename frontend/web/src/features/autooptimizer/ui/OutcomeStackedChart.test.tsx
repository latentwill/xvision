import { describe, expect, it, vi, beforeAll, afterAll } from "vitest";
import { render, screen } from "@testing-library/react";
import type { StatsRow } from "../api";

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

import { OutcomeStackedChart } from "./OutcomeStackedChart";

const FIXTURE: StatsRow[] = [
  {
    cycle_id: "cyc-1",
    session_id: "sess-1",
    ts: "2026-06-01T10:00:00Z",
    kept: 2,
    suspect: 1,
    dropped: 3,
    best_delta_holdout: 0.05,
    cost_usd: 0.10,
    cum_cost_usd: 0.10,
  },
  {
    cycle_id: "cyc-2",
    session_id: "sess-1",
    ts: "2026-06-01T10:05:00Z",
    kept: 3,
    suspect: 0,
    dropped: 2,
    best_delta_holdout: 0.08,
    cost_usd: 0.07,
    cum_cost_usd: 0.17,
  },
];

describe("OutcomeStackedChart", () => {
  it("renders the chart wrapper when rows have data", () => {
    const { container } = render(<OutcomeStackedChart rows={FIXTURE} />);
    expect(container.querySelector("[data-chart='outcome-stacked']")).toBeInTheDocument();
  });

  it("shows the empty-state message when rows is empty", () => {
    render(<OutcomeStackedChart rows={[]} />);
    expect(screen.getByText(/no cycles yet/i)).toBeInTheDocument();
  });

  it("renders a legend with kept, suspect, and dropped labels", () => {
    render(<OutcomeStackedChart rows={FIXTURE} />);
    expect(screen.getByText(/kept/i)).toBeInTheDocument();
    expect(screen.getByText(/suspect/i)).toBeInTheDocument();
    expect(screen.getByText(/dropped/i)).toBeInTheDocument();
  });
});
