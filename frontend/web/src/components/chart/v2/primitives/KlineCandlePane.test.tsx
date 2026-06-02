import { render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { KlineCandlePane } from "./KlineCandlePane";
import { CHART_V2_RANGE_EVENT } from "./ChartFrame";
import type { CandleColumns } from "../types";

const klineMocks = vi.hoisted(() => {
  const state = {
    symbol: null as unknown,
    period: null as unknown,
    loadedBars: [] as unknown[][],
    // The currently-loaded bar list, replaced on each setDataLoader "init" load.
    // getDataList() returns this, so its length changes across rerenders just
    // like the real store — letting the frozen-restore math be exercised.
    dataList: [] as unknown[],
    overlays: [] as Array<Record<string, unknown>>,
    // Every overlay id minted by createOverlay, in creation order, paired with
    // the create payload, so tests can assert which ids get removed on rerun.
    createdOverlayIds: [] as string[],
    // Every id passed to removeOverlay (extracted from the OverlayFilter).
    removedOverlayIds: [] as string[],
    // Monotonic counter so each createOverlay call returns a UNIQUE id; the
    // accumulation bug is invisible if every overlay shares one id.
    overlayIdCounter: 0,
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
            // Mirror the real store: a fresh `setDataLoader` "init" load replaces
            // the active data list with the just-loaded bars, so getDataList()
            // reflects the latest loaded length. Capture the PRE-load length
            // first so the data effect's frozen-restore math (numNew = newLen -
            // prevLen) sees the old length when it reads getDataList() at the
            // top of the effect, before this callback runs.
            state.dataList = bars;
          },
        });
      }
    }),
    createOverlay: vi.fn((value: Record<string, unknown>) => {
      const id = `overlay-${state.overlayIdCounter++}`;
      // Tag the recorded payload with its minted id so removeOverlay can drop
      // the exact live entry by id, keeping `overlays` a faithful NET view.
      state.overlays.push({ ...value, __id: id });
      state.createdOverlayIds.push(id);
      return id;
    }),
    removeOverlay: vi.fn((filter: { id?: string } | string) => {
      const id = typeof filter === "string" ? filter : filter?.id;
      if (typeof id === "string") {
        state.removedOverlayIds.push(id);
        // Mirror the real store: drop the removed overlay from the live set so
        // tests can assert the NET overlay population, not just create-count.
        const idx = state.overlays.findIndex((o) => o.__id === id);
        if (idx > -1) state.overlays.splice(idx, 1);
      }
      return true;
    }),
    setBarSpace: vi.fn(),
    getBarSpace: vi.fn(() => ({ bar: 8, halfBar: 4, gapBar: 1, halfGapBar: 0.5 })),
    scrollToRealTime: vi.fn(),
    scrollToTimestamp: vi.fn(),
    getVisibleRange: vi.fn(() => ({ from: 0, to: 2, realFrom: 0, realTo: 2 })),
    getDom: vi.fn(() => ({ clientWidth: 800 }) as unknown as HTMLElement),
    getDataList: vi.fn(() => state.dataList),
    getOffsetRightDistance: vi.fn(() => 0),
    setOffsetRightDistance: vi.fn(),
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
    klineMocks.state.dataList = [];
    klineMocks.state.overlays = [];
    klineMocks.state.createdOverlayIds = [];
    klineMocks.state.removedOverlayIds = [];
    klineMocks.state.overlayIdCounter = 0;
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
    expect(buy.points).toEqual([{ timestamp: 1000, value: 10 }]);

    const veto = markerOverlays.find(
      (o) => (o.extendData as { kind: string }).kind === "veto",
    )!;
    expect(veto).toBeDefined();
    expect((veto.extendData as { text: string }).text).toBe("Veto: risk");
  });

  it("draws buy and sell markers as textless chevrons", async () => {
    render(<KlineCandlePane candles={candles} />);

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    const template = klineMocks.state.registeredTemplates.find(
      (t) => (t as { name?: string }).name === "xvnMarker",
    ) as {
      createPointFigures: (args: {
        coordinates: Array<{ x: number; y: number }>;
        overlay: { extendData: Record<string, unknown> };
      }) => unknown[];
    };

    const figures = template.createPointFigures({
      coordinates: [{ x: 40, y: 80 }],
      overlay: {
        extendData: { kind: "buy", text: "Buy 1", color: "#00E676" },
      },
    });

    expect(figures).toHaveLength(2);
    expect(figures.every((fig) => (fig as { type: string }).type === "line")).toBe(
      true,
    );
    expect(figures.some((fig) => (fig as { type: string }).type === "text")).toBe(
      false,
    );
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

  it("creates an xvnPositionBand overlay for each position span", async () => {
    render(
      <KlineCandlePane
        candles={candles}
        positions={[{ side: "long", start: 1, end: 2 }]}
      />,
    );

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    await waitFor(() => {
      expect(
        klineMocks.state.overlays.filter((o) => o.name === "xvnPositionBand")
          .length,
      ).toBeGreaterThanOrEqual(1);
    });

    const bandOverlays = klineMocks.state.overlays.filter(
      (o) => o.name === "xvnPositionBand",
    );
    expect(bandOverlays).toHaveLength(1);
    expect(bandOverlays[0].points).toEqual([
      { timestamp: 1000, value: 0 },
      { timestamp: 2000, value: 0 },
    ]);
  });

  it("uses a distinct band color for short vs long spans", async () => {
    render(
      <KlineCandlePane
        candles={candles}
        positions={[
          { side: "long", start: 1, end: 2 },
          { side: "short", start: 3, end: 4 },
        ]}
      />,
    );

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    await waitFor(() => {
      expect(
        klineMocks.state.overlays.filter((o) => o.name === "xvnPositionBand")
          .length,
      ).toBeGreaterThanOrEqual(2);
    });

    const bandOverlays = klineMocks.state.overlays.filter(
      (o) => o.name === "xvnPositionBand",
    );
    expect(bandOverlays).toHaveLength(2);

    const longBand = bandOverlays.find(
      (o) => (o.points as Array<{ timestamp: number }>)[0].timestamp === 1000,
    )!;
    const shortBand = bandOverlays.find(
      (o) => (o.points as Array<{ timestamp: number }>)[0].timestamp === 3000,
    )!;
    const longColor = (longBand.extendData as { color: string }).color;
    const shortColor = (shortBand.extendData as { color: string }).color;
    expect(longColor).toBeTruthy();
    expect(shortColor).toBeTruthy();
    expect(longColor).not.toBe(shortColor);
  });

  it("registers the xvnPositionBand overlay template exactly once at module scope", async () => {
    render(<KlineCandlePane candles={candles} />);

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    const bandTemplates = klineMocks.state.registeredTemplates.filter(
      (t) => (t as { name?: string }).name === "xvnPositionBand",
    );
    expect(bandTemplates).toHaveLength(1);
    expect(
      typeof (bandTemplates[0] as { createPointFigures?: unknown })
        .createPointFigures,
    ).toBe("function");
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

  it("applies a finite range preset via setBarSpace + scrollToRealTime", async () => {
    render(<KlineCandlePane candles={candles} />);

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    expect(() => {
      window.dispatchEvent(
        new CustomEvent(CHART_V2_RANGE_EVENT, { detail: "1d" }),
      );
    }).not.toThrow();

    expect(klineMocks.chart.setBarSpace).toHaveBeenCalled();
    expect(klineMocks.chart.scrollToRealTime).toHaveBeenCalled();
  });

  it("applies the All preset without throwing", async () => {
    render(<KlineCandlePane candles={candles} />);

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    expect(() => {
      window.dispatchEvent(
        new CustomEvent(CHART_V2_RANGE_EVENT, { detail: "All" }),
      );
    }).not.toThrow();

    expect(klineMocks.chart.setBarSpace).toHaveBeenCalled();
    expect(klineMocks.chart.scrollToRealTime).toHaveBeenCalled();
  });

  it("removes the range listener on unmount (no throw after teardown)", async () => {
    const { unmount } = render(<KlineCandlePane candles={candles} />);

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    unmount();
    klineMocks.chart.setBarSpace.mockClear();

    window.dispatchEvent(
      new CustomEvent(CHART_V2_RANGE_EVENT, { detail: "1w" }),
    );

    // After unmount the listener is gone, so no chart calls should fire.
    expect(klineMocks.chart.setBarSpace).not.toHaveBeenCalled();
  });

  it("removes the previous run's overlays on rerender so live ticks don't accumulate", async () => {
    // Live surface (LiveChartV2Container) re-adapts a fresh payload every SSE
    // tick, so candles/markers change identity each render and the data effect
    // re-runs. Without effect-cleanup the effect stacks NEW overlays on top of
    // the old ones every tick — unbounded duplicates. This asserts the cleanup
    // removes the prior run's overlay ids before the next run creates new ones.
    const { rerender } = render(
      <KlineCandlePane
        candles={candles}
        markers={[{ kind: "buy", time: 1, price: 10, text: "Buy 1" }]}
      />,
    );

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    // First render created exactly one marker overlay and removed nothing yet.
    await waitFor(() => {
      expect(
        klineMocks.state.overlays.filter((o) => o.name === "xvnMarker"),
      ).toHaveLength(1);
    });
    expect(klineMocks.state.createdOverlayIds).toHaveLength(1);
    expect(klineMocks.state.removedOverlayIds).toHaveLength(0);
    const firstRunIds = [...klineMocks.state.createdOverlayIds];

    // A new tick: changed candles AND a grown markers array (two markers).
    const nextCandles: CandleColumns = {
      time: [1_700_000_000, 1_700_000_060, 1_700_000_120],
      open: [100, 101, 102],
      high: [102, 103, 104],
      low: [99, 100, 101],
      close: [101, 102, 103],
      volume: [10, 12, 14],
    };
    rerender(
      <KlineCandlePane
        candles={nextCandles}
        markers={[
          { kind: "buy", time: 1, price: 10, text: "Buy 1" },
          { kind: "sell", time: 2, price: 12, text: "Sell 1" },
        ]}
      />,
    );

    // React fires the prior run's cleanup BEFORE the next run's body, so the
    // first run's overlay id must be removed.
    await waitFor(() => {
      expect(klineMocks.state.removedOverlayIds).toEqual(
        expect.arrayContaining(firstRunIds),
      );
    });

    // The NET live overlay set reflects the NEW props (two markers), not the
    // accumulation of old + new (which would be three).
    await waitFor(() => {
      expect(
        klineMocks.state.overlays.filter((o) => o.name === "xvnMarker"),
      ).toHaveLength(2);
    });
    expect(
      klineMocks.state.overlays.filter((o) => o.name === "xvnMarker"),
    ).not.toHaveLength(3);
  });

  // ── follow / freeze / resume viewport contract ─────────────────────────────

  it("does NOT touch scroll when follow is undefined (static consumers)", async () => {
    // Static consumers (RunChartV2/ScenarioChartV2/WizardPreviewChartV2) pass no
    // follow prop. The data effect re-runs on overlay/marker/layer-toggle changes
    // too — unconditionally scrolling would snap away a user's pan/zoom. With
    // follow undefined the pane must leave scroll completely alone.
    const { rerender } = render(<KlineCandlePane candles={candles} />);

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    // A rerender (e.g. overlay toggle) re-runs the data effect.
    rerender(
      <KlineCandlePane
        candles={candles}
        overlays={{ sma20: { time: [1_700_000_000], value: [100] } }}
      />,
    );

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(2);
    });

    expect(klineMocks.chart.scrollToRealTime).not.toHaveBeenCalled();
    expect(klineMocks.chart.setOffsetRightDistance).not.toHaveBeenCalled();
  });

  it("pins to realtime after each data load when follow is true", async () => {
    const { rerender } = render(
      <KlineCandlePane candles={candles} follow />,
    );

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });
    await waitFor(() => {
      expect(klineMocks.chart.scrollToRealTime).toHaveBeenCalled();
    });

    const callsAfterFirst = klineMocks.chart.scrollToRealTime.mock.calls.length;

    // A new tick with more candles, still following.
    const nextCandles: CandleColumns = {
      time: [1_700_000_000, 1_700_000_060, 1_700_000_120],
      open: [100, 101, 102],
      high: [102, 103, 104],
      low: [99, 100, 101],
      close: [101, 102, 103],
      volume: [10, 12, 14],
    };
    rerender(<KlineCandlePane candles={nextCandles} follow />);

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(2);
    });
    await waitFor(() => {
      expect(
        klineMocks.chart.scrollToRealTime.mock.calls.length,
      ).toBeGreaterThan(callsAfterFirst);
    });

    // Frozen-restore must NOT fire while following.
    expect(klineMocks.chart.setOffsetRightDistance).not.toHaveBeenCalled();
  });

  it("freezes the window (no realtime snap, compensated offset) when follow is false", async () => {
    const { rerender } = render(
      <KlineCandlePane candles={candles} follow={false} />,
    );

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    klineMocks.chart.scrollToRealTime.mockClear();
    klineMocks.chart.setOffsetRightDistance.mockClear();

    // A new tick appends one more bar (2 → 3). getDataList() returns the prior
    // (length-2) list at the top of the effect; after the load it is length 3.
    const nextCandles: CandleColumns = {
      time: [1_700_000_000, 1_700_000_060, 1_700_000_120],
      open: [100, 101, 102],
      high: [102, 103, 104],
      low: [99, 100, 101],
      close: [101, 102, 103],
      volume: [10, 12, 14],
    };
    rerender(<KlineCandlePane candles={nextCandles} follow={false} />);

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(2);
    });
    await waitFor(() => {
      expect(klineMocks.chart.setOffsetRightDistance).toHaveBeenCalled();
    });

    // Frozen must never yank the view to realtime.
    expect(klineMocks.chart.scrollToRealTime).not.toHaveBeenCalled();

    // setDataLoader's "init" reset moves the last bar to a default right offset;
    // the restore pushes the freshly-appended bars off the right edge so the
    // prior window stays put. offsetBefore (0) + numNew (1) * barWidth (8) = 8.
    expect(klineMocks.chart.setOffsetRightDistance).toHaveBeenCalledWith(8);
  });

  it("snaps to realtime immediately when follow flips false → true (resume)", async () => {
    const { rerender } = render(
      <KlineCandlePane candles={candles} follow={false} />,
    );

    await waitFor(() => {
      expect(klineMocks.state.loadedBars).toHaveLength(1);
    });

    klineMocks.chart.scrollToRealTime.mockClear();

    // Clicking "Resume live" flips follow to true WITHOUT new candle data.
    rerender(<KlineCandlePane candles={candles} follow />);

    await waitFor(() => {
      expect(klineMocks.chart.scrollToRealTime).toHaveBeenCalled();
    });
  });
});
