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
    // Every template ever registered for the lifetime of this module. Unlike
    // `state`-reset arrays this is NEVER cleared in beforeEach, because the
    // module-scope `registerOverlay("xvnLine")` in KlineCandlePane fires once
    // at import — before any test's beforeEach runs.
    registeredTemplates: [] as Array<Record<string, unknown>>,
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
      state.registeredTemplates.push(template);
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
    // Intentionally NOT clearing state.registeredTemplates: the xvnLine
    // template is registered once at module-import time, which is before this
    // beforeEach ever runs. Clearing it would erase the only evidence the
    // module-scope registration happened.
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

  it("registers the xvnLine overlay template exactly once at module scope", async () => {
    render(<KlineCandlePane candles={candles} />);

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    // The xvnLine template is registered exactly once when KlineCandlePane is
    // imported (module scope). registeredTemplates captures that import-time
    // call and is never reset, so the count must be precisely one — zero
    // registrations (the original flag-before-call regression) must fail here.
    const xvnTemplates = klineMocks.state.registeredTemplates.filter(
      (t) => (t as { name?: string }).name === "xvnLine",
    );
    expect(xvnTemplates).toHaveLength(1);

    const template = xvnTemplates[0] as {
      name?: string;
      createPointFigures?: unknown;
    };
    expect(template.name).toBe("xvnLine");
    expect(typeof template.createPointFigures).toBe("function");
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

  it("creates an xvnMarker overlay for each priced marker", async () => {
    render(
      <KlineCandlePane
        candles={candles}
        markers={[
          { kind: "buy", time: 1, price: 10, text: "Buy 1" },
          { kind: "veto", time: 2, price: 11, text: "Veto: risk" },
        ]}
      />,
    );

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    await waitFor(() => {
      expect(klineMocks.state.overlays.length).toBeGreaterThanOrEqual(2);
    });

    const markerOverlays = klineMocks.state.overlays.filter(
      (o) => o.name === "xvnMarker",
    );
    expect(markerOverlays).toHaveLength(2);

    const buy = markerOverlays.find(
      (o) => (o.extendData as { kind: string }).kind === "buy",
    )!;
    expect(buy).toBeDefined();
    expect((buy.extendData as { kind: string }).kind).toBe("buy");
    expect((buy.extendData as { text: string }).text).toBe("Buy 1");
    expect(buy.points).toEqual([{ timestamp: 1000, value: 10 }]);

    const veto = markerOverlays.find(
      (o) => (o.extendData as { kind: string }).kind === "veto",
    )!;
    expect(veto).toBeDefined();
    expect((veto.extendData as { text: string }).text).toBe("Veto: risk");
  });

  it("registers the xvnMarker overlay template exactly once at module scope", async () => {
    render(<KlineCandlePane candles={candles} />);

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    const markerTemplates = klineMocks.state.registeredTemplates.filter(
      (t) => (t as { name?: string }).name === "xvnMarker",
    );
    expect(markerTemplates).toHaveLength(1);
    expect(
      typeof (markerTemplates[0] as { createPointFigures?: unknown })
        .createPointFigures,
    ).toBe("function");
  });

  it("skips markers without a price", async () => {
    render(
      <KlineCandlePane
        candles={candles}
        markers={[
          { kind: "buy", time: 1, price: 10, text: "Buy 1" },
          { kind: "hold", time: 2, text: "No price" },
        ]}
      />,
    );

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    await waitFor(() => {
      expect(
        klineMocks.state.overlays.filter((o) => o.name === "xvnMarker").length,
      ).toBeGreaterThanOrEqual(1);
    });

    const markerOverlays = klineMocks.state.overlays.filter(
      (o) => o.name === "xvnMarker",
    );
    expect(markerOverlays).toHaveLength(1);
    expect((markerOverlays[0].extendData as { kind: string }).kind).toBe("buy");
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
