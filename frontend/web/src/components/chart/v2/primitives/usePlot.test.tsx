/**
 * Regression tests for usePlot dep-array correctness (W14 fix).
 *
 * Root cause (W14): the original dep array used `JSON.stringify(opts)`, which
 * silently drops function-valued fields such as `axes[].values` (the %
 * formatter). When only the formatter changed the effect did NOT re-run, so
 * the stale uPlot instance kept rendering "00.0%" ticks.
 *
 * Fix: `optsKey(opts)` appends the `.toString()` of `axes[].values` so a
 * changed formatter busts the dep and the plot is recreated.
 *
 * These tests verify:
 *  1. The plot IS created on initial mount.
 *  2. The plot IS recreated (destroy + new) when ONLY the axes.values
 *     function changes and data is unchanged.
 *  3. The plot is NOT needlessly recreated when opts and data are stable
 *     across re-renders.
 */
import { renderHook } from "@testing-library/react";
import { useRef } from "react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import type uPlot from "uplot";

// ─── Mock uPlot ──────────────────────────────────────────────────────────────
// vi.hoisted ensures these variables are available when vi.mock factory runs.

const { mockDestroy, mockSetSize, MockuPlot } = vi.hoisted(() => {
  const mockDestroy = vi.fn();
  const mockSetSize = vi.fn();
  const MockuPlot = vi.fn(() => ({
    destroy: mockDestroy,
    setSize: mockSetSize,
    data: [[], []],
    scales: { x: {} },
  }));
  return { mockDestroy, mockSetSize, MockuPlot };
});

vi.mock("uplot", () => ({
  default: MockuPlot,
}));

// Mock ResizeObserver (jsdom doesn't ship it).
const mockObserve = vi.fn();
const mockDisconnect = vi.fn();
vi.stubGlobal(
  "ResizeObserver",
  vi.fn(() => ({ observe: mockObserve, disconnect: mockDisconnect })),
);

// ─── Import under test (after mocks are registered) ──────────────────────────
import { usePlot } from "./usePlot";

// ─── Helpers ─────────────────────────────────────────────────────────────────

function makeOpts(
  valuesFormatter: uPlot.Axis["values"] = (_u, vals: number[]) =>
    vals.map(String),
): uPlot.Options {
  return {
    width: 0,
    height: 200,
    axes: [
      {},
      {
        values: valuesFormatter,
      },
    ],
    series: [{}, { label: "y", stroke: "#0f0" }],
  } as uPlot.Options;
}

const DATA: uPlot.AlignedData = [[1, 2, 3], [10, 20, 30]];

// Wrapper that renders the hook with a real div ref.
function renderUsePlot(opts: uPlot.Options, data: uPlot.AlignedData = DATA) {
  return renderHook(
    ({ o, d }: { o: uPlot.Options; d: uPlot.AlignedData }) => {
      const ref = useRef<HTMLDivElement>(document.createElement("div"));
      usePlot(o, d, ref, 200);
    },
    { initialProps: { o: opts, d: data } },
  );
}

// ─── Tests ───────────────────────────────────────────────────────────────────

describe("usePlot dep-array regression (W14)", () => {
  beforeEach(() => {
    MockuPlot.mockClear();
    mockDestroy.mockClear();
    mockSetSize.mockClear();
    mockObserve.mockClear();
    mockDisconnect.mockClear();
  });

  it("creates a uPlot instance on initial mount", () => {
    renderUsePlot(makeOpts());
    expect(MockuPlot).toHaveBeenCalledTimes(1);
  });

  it("recreates the plot when only the axes.values formatter function changes", () => {
    const firstFormatter: uPlot.Axis["values"] = (_u, vals: number[]) =>
      vals.map((v) => v + "%");
    const secondFormatter: uPlot.Axis["values"] = (_u, vals: number[]) =>
      vals.map((v) => v.toFixed(1) + "%");

    const { rerender } = renderUsePlot(makeOpts(firstFormatter));
    expect(MockuPlot).toHaveBeenCalledTimes(1);

    // Re-render with a different formatter function — same data, same structural opts.
    rerender({ o: makeOpts(secondFormatter), d: DATA });

    // The old instance must have been destroyed and a new one created.
    expect(mockDestroy).toHaveBeenCalledTimes(1);
    expect(MockuPlot).toHaveBeenCalledTimes(2);
  });

  it("does NOT recreate the plot on a NEW but structurally-identical opts object (the real production path)", () => {
    // Production reality: each render builds a FRESH opts object with a FRESH
    // inline formatter arrow whose SOURCE TEXT is identical. optsKey must be
    // structurally stable (not merely reference-stable), or every parent
    // re-render would recreate the plot and destroy zoom/pan state.
    const makeStableFormatter = (): uPlot.Axis["values"] =>
      (_u, vals: number[]) => vals.map((v) => v.toFixed(1) + "%");

    // First render: fresh object + fresh formatter instance.
    const { rerender } = renderUsePlot(makeOpts(makeStableFormatter()), DATA);
    expect(MockuPlot).toHaveBeenCalledTimes(1);

    // Re-render with a DIFFERENT object reference and a DIFFERENT function
    // instance that has identical source text + same data reference.
    rerender({ o: makeOpts(makeStableFormatter()), d: DATA });

    // optsKey is identical (same JSON structure + same function source) → no recreation.
    expect(mockDestroy).toHaveBeenCalledTimes(0);
    expect(MockuPlot).toHaveBeenCalledTimes(1);
  });
});
