import { describe, expect, it, vi, beforeAll, afterAll } from "vitest";
import { render, screen } from "@testing-library/react";
import type { MutatorScore } from "../api";

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

import { WriterLadderChart } from "./WriterLadderChart";

const FIXTURE: MutatorScore[] = [
  {
    provider: "anthropic",
    model: "claude-haiku-4-5",
    prompt_version: "v1",
    proposals: 10,
    accepted: 6,
    rejected_overfit: 4,
    avg_delta_sharpe: 0.18,
  },
  {
    provider: "openai",
    model: "gpt-4o-mini",
    prompt_version: "v1",
    proposals: 8,
    accepted: 3,
    rejected_overfit: 5,
    avg_delta_sharpe: 0.07,
  },
];

describe("WriterLadderChart", () => {
  it("renders the chart wrapper when rows have data", () => {
    const { container } = render(<WriterLadderChart rows={FIXTURE} />);
    expect(container.querySelector("[data-chart='writer-ladder']")).toBeInTheDocument();
  });

  it("shows the empty-state message when rows is empty", () => {
    render(<WriterLadderChart rows={[]} />);
    expect(screen.getByText(/no writer data yet/i)).toBeInTheDocument();
  });

  it("renders a legend entry for each writer", () => {
    render(<WriterLadderChart rows={FIXTURE} />);
    expect(screen.getByText(/claude-haiku-4-5/i)).toBeInTheDocument();
    expect(screen.getByText(/gpt-4o-mini/i)).toBeInTheDocument();
  });
});
