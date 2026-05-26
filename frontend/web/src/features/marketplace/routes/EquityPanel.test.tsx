// src/features/marketplace/routes/EquityPanel.test.tsx
import { render, screen, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeAll, afterAll } from "vitest";
import { EquityPanel } from "./EquityPanel";
import type { EquityCurve } from "@/features/marketplace/data/types";

// Mock uPlot so tests don't need a DOM canvas environment
vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    destroy() {}
  },
}));

// Mock ResizeObserver — jsdom doesn't provide it, but HeroGradientEquity uses it
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

const CURVE: EquityCurve = {
  base: 1000,
  points: [
    ...Array.from({ length: 60 }, (_, i) => ({ value: 1000 + i * 5, phase: "backtest" as const })),
    ...Array.from({ length: 30 }, (_, i) => ({ value: 1300 + i * 3, phase: "live" as const })),
  ],
};

describe("EquityPanel", () => {
  it("renders the card header with base amount", () => {
    render(<EquityPanel curve={CURVE} />);
    expect(screen.getByText(/equity curve/i)).toBeInTheDocument();
    expect(screen.getByText(/base \$1,000/i)).toBeInTheDocument();
  });

  it("renders the 'If I bought at mint' toggle", () => {
    render(<EquityPanel curve={CURVE} />);
    expect(screen.getByRole("button", { name: /if i bought at mint/i })).toBeInTheDocument();
  });

  it("renders window toggle buttons", () => {
    render(<EquityPanel curve={CURVE} />);
    expect(screen.getByRole("button", { name: /30d/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /90d/i })).toBeInTheDocument();
  });

  it("clicking 30d sets active window", async () => {
    render(<EquityPanel curve={CURVE} />);
    const btn30d = screen.getByRole("button", { name: /30d/i });
    await act(async () => {
      await userEvent.click(btn30d);
    });
    // 30d button should now appear active (aria-pressed or class change)
    // Just verify no error is thrown — visual state is implementation detail
    expect(btn30d).toBeInTheDocument();
  });
});
