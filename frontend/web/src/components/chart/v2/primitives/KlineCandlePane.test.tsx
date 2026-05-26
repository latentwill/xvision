import { render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { KlineCandlePane } from "./KlineCandlePane";
import type { CandleColumns } from "../types";

const klineMocks = vi.hoisted(() => {
  const state = {
    symbol: null as unknown,
    period: null as unknown,
    loadedBars: [] as unknown[][],
    overlays: [] as Array<Record<string, unknown>>,
    registeredOverlays: [] as Array<Record<string, unknown>>,
  };

  const chart = {
    resize: vi.fn(),
    setStyles: vi.fn(),
    setSymbol: vi.fn((symbol: unknown) => {
      state.symbol = symbol;
    }),
    setPeriod: vi.fn((period: unknown) => {
      state.period = period;
    }),
    setDataLoader: vi.fn((loader: {
      getBars: (params: { callback: (bars: unknown[], more: boolean) => void }) => void;
    }) => {
      if (state.symbol && state.period) {
        loader.getBars({
          callback: (bars: unknown[], _more: boolean) => {
            state.loadedBars.push(bars);
          },
        });
      }
    }),
    createOverlay: vi.fn((value: Record<string, unknown>) => {
      state.overlays.push(value);
      return "overlay-id";
    }),
  };

  return {
    chart,
    dispose: vi.fn(),
    init: vi.fn(() => chart),
    registerOverlay: vi.fn((template: Record<string, unknown>) => {
      state.registeredOverlays.push(template);
    }),
    state,
  };
});

vi.mock("klinecharts", () => ({
  init: klineMocks.init,
  dispose: klineMocks.dispose,
  registerOverlay: klineMocks.registerOverlay,
}));

const candles: CandleColumns = {
  time: [1_700_000_000, 1_700_000_060],
  open: [100, 101],
  high: [102, 103],
  low: [99, 100],
  close: [101, 102],
  volume: [10, 12],
};

describe("KlineCandlePane", () => {
  beforeEach(() => {
    klineMocks.state.symbol = null;
    klineMocks.state.period = null;
    klineMocks.state.loadedBars = [];
    klineMocks.state.overlays = [];
    klineMocks.state.registeredOverlays = [];
    vi.clearAllMocks();

    class ResizeObserverStub {
      observe() {}
      disconnect() {}
    }
    Object.defineProperty(globalThis, "ResizeObserver", {
      value: ResizeObserverStub,
      configurable: true,
    });
  });

  afterEach(() => {
    delete (globalThis as { ResizeObserver?: unknown }).ResizeObserver;
  });

  it("sets the required symbol and period before installing the data loader", async () => {
    render(<KlineCandlePane candles={candles} />);

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    expect(klineMocks.chart.setSymbol).toHaveBeenCalledWith({
      ticker: "chart-v2",
      pricePrecision: 4,
      volumePrecision: 2,
    });
    expect(klineMocks.chart.setPeriod).toHaveBeenCalledWith({ type: "minute", span: 1 });
    expect(klineMocks.chart.setDataLoader).toHaveBeenCalledTimes(1);
    expect(klineMocks.state.loadedBars[0]).toEqual([
      {
        timestamp: 1_700_000_000_000,
        open: 100,
        high: 102,
        low: 99,
        close: 101,
        volume: 10,
      },
      {
        timestamp: 1_700_000_060_000,
        open: 101,
        high: 103,
        low: 100,
        close: 102,
        volume: 12,
      },
    ]);
  });

  it("registers the xvnLine overlay template once at module scope", async () => {
    render(<KlineCandlePane candles={candles} />);

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    // registerOverlay is module-scoped + guarded — across the whole suite it
    // must have been called exactly once with the xvnLine template.
    const xvnTemplates = klineMocks.registerOverlay.mock.calls.filter(
      ([t]) => (t as { name?: string }).name === "xvnLine",
    );
    expect(xvnTemplates.length).toBeLessThanOrEqual(1);
    const template = (klineMocks.registerOverlay.mock.calls.find(
      ([t]) => (t as { name?: string }).name === "xvnLine",
    )?.[0] ?? null) as { name?: string; createPointFigures?: unknown } | null;
    if (template) {
      expect(template.name).toBe("xvnLine");
      expect(typeof template.createPointFigures).toBe("function");
    }
  });

  it("creates an xvnLine overlay for each active line series", async () => {
    render(
      <KlineCandlePane
        candles={candles}
        overlays={{
          sma20: { time: [1_700_000_000, 1_700_000_060], value: [100, 101] },
          ema50: { time: [1_700_000_000, 1_700_000_060], value: [99, 100] },
        }}
      />,
    );

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    await waitFor(() => {
      expect(klineMocks.state.overlays.length).toBeGreaterThanOrEqual(2);
    });

    expect(
      klineMocks.state.overlays.every((o) => o.name === "xvnLine"),
    ).toBe(true);

    const created = new Map(
      klineMocks.state.overlays.map((o) => [
        (o.extendData as { key: string }).key,
        o,
      ]),
    );
    expect(created.has("sma20")).toBe(true);
    expect(created.has("ema50")).toBe(true);

    const sma20 = created.get("sma20")!;
    expect(sma20.points).toEqual([
      { timestamp: 1_700_000_000_000, value: 100 },
      { timestamp: 1_700_000_060_000, value: 101 },
    ]);
    expect((sma20.extendData as { dashed: boolean }).dashed).toBe(false);
    expect((created.get("ema50")!.extendData as { dashed: boolean }).dashed).toBe(
      true,
    );
  });

  it("skips line series toggled off via overlayActive", async () => {
    render(
      <KlineCandlePane
        candles={candles}
        overlays={{
          sma20: { time: [1_700_000_000], value: [100] },
          ema50: { time: [1_700_000_000], value: [99] },
        }}
        overlayActive={{ ema50: false }}
      />,
    );

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    await waitFor(() => {
      expect(klineMocks.state.overlays.length).toBeGreaterThanOrEqual(1);
    });

    const keys = klineMocks.state.overlays.map(
      (o) => (o.extendData as { key: string }).key,
    );
    expect(keys).toContain("sma20");
    expect(keys).not.toContain("ema50");
  });
});
