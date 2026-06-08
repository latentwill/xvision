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

import { SpendChart } from "./SpendChart";

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

describe("SpendChart", () => {
  it("renders the chart wrapper when rows have cost data", () => {
    const { container } = render(<SpendChart rows={FIXTURE} />);
    expect(container.querySelector("[data-chart='spend']")).toBeInTheDocument();
  });

  it("shows the empty-state message when rows is empty", () => {
    render(<SpendChart rows={[]} />);
    expect(screen.getByText(/no cost data yet/i)).toBeInTheDocument();
  });

  it("renders a budget cap label when budgetCap is provided", () => {
    render(<SpendChart rows={FIXTURE} budgetCap={1.0} />);
    expect(screen.getByText(/budget/i)).toBeInTheDocument();
  });

  it("does not render a budget cap label when budgetCap is not provided", () => {
    render(<SpendChart rows={FIXTURE} />);
    expect(screen.queryByText(/budget/i)).toBeNull();
  });
});
