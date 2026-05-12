import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { storageKey } from "./chart-layers";
import samplePayload from "./__fixtures__/sample-run-chart.json";

// lightweight-charts touches canvas/WebGL which jsdom can't provide. The
// mock returns a Proxy whose every method is a no-op that returns another
// Proxy, so RunChart's chained `.addCandlestickSeries().setData(...)`
// style calls all succeed.
function chainStub(): unknown {
  return new Proxy(() => chainStub(), {
    get() {
      return chainStub();
    },
  });
}

vi.mock("lightweight-charts", () => ({
  ColorType: { Solid: "solid" },
  CrosshairMode: { Normal: 0 },
  createChart: () => chainStub(),
}));

// The test file is imported after vi.mock — pull RunChart in via a
// dynamic import so the mock is hoisted correctly under all vitest
// transform paths.
import { RunChart } from "./RunChart";

describe("RunChart", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  // Without `globals: true` in vitest config, RTL's auto-cleanup hook
  // isn't installed — DOM from one test would otherwise leak into the
  // next and confuse `screen.getByText` queries.
  afterEach(() => {
    cleanup();
  });

  it("renders the layers toggle without crashing on a valid payload", () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    render(<RunChart payload={samplePayload as any} />);
    expect(screen.getByText(/Layers/)).toBeInTheDocument();
  });

  it("persists layer toggles to localStorage", () => {
    const key = storageKey("run-detail");
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    render(<RunChart payload={samplePayload as any} />);

    // Open the Layers panel.
    fireEvent.click(screen.getByText(/Layers/));

    // Default for sma20 is true (per DEFAULT_LAYERS in chart-layers.ts).
    // The label wraps the bare key text and the checkbox — getByText
    // returns the label element itself, so the checkbox lives inside it.
    const sma20Label = screen.getByText(/^sma20$/).closest("label");
    expect(sma20Label).not.toBeNull();
    const sma20Checkbox = sma20Label!.querySelector(
      "input[type='checkbox']",
    ) as HTMLInputElement | null;
    expect(sma20Checkbox).not.toBeNull();
    expect(sma20Checkbox!.checked).toBe(true);

    // Toggle it off; the useChartLayers effect writes to localStorage.
    fireEvent.click(sma20Checkbox!);

    const raw = localStorage.getItem(key);
    expect(raw).not.toBeNull();
    const persisted = JSON.parse(raw!) as Record<string, boolean>;
    expect(persisted.sma20).toBe(false);
  });
});
