import { render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { KlineCandlePane } from "./KlineCandlePane";
import type { CandleColumns } from "../types";

const klineMocks = vi.hoisted(() => {
  const state = {
    symbol: null as unknown,
    period: null as unknown,
    loadedBars: [] as unknown[][],
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
  };

  return {
    chart,
    dispose: vi.fn(),
    init: vi.fn(() => chart),
    state,
  };
});

vi.mock("klinecharts", () => ({
  init: klineMocks.init,
  dispose: klineMocks.dispose,
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
});
