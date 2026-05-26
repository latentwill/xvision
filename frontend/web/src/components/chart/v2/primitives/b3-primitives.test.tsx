/**
 * Tests for B3 primitives + the kline-anchor adapter math + the
 * AIAnnotationDashboard surface filter helper.
 *
 * Canvas-heavy bits (KlineCandlePane mount) are NOT exercised here;
 * the chart-lab/dashboards/annotated route covers manual visual review.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act, waitFor } from "@testing-library/react";

import {
  createKlineAnchor,
  DEFAULT_BOUNDS,
  deriveRange,
  xForIndex,
  yForPrice,
  type AnchorBounds,
} from "../adapters/kline-anchor";

import { Callout, CALLOUT_WIDTH } from "./Callout";
import { spreadRowXs } from "./AnnotationOverlay";
import { InsightLog } from "./InsightLog";
import { AiEnginePill } from "./AiEnginePill";
import { typesForFilter } from "../surfaces/AIAnnotationDashboard";
import type { Annotation } from "../types";

const SAMPLE: Annotation = {
  idx: 22,
  side: "top",
  type: "PATTERN",
  title: "Bull Flag",
  body: "test body",
  conf: 0.74,
  action: "WATCH",
};

const DANGER: Annotation = { ...SAMPLE, idx: 80, type: "RISK", danger: true, title: "Liquidation Wall" };

describe("kline-anchor: xForIndex", () => {
  const b: AnchorBounds = { ...DEFAULT_BOUNDS, width: 1000, height: 400 };
  it("returns the middle of usable when count = 1", () => {
    const usable = b.width - b.padLeft - b.padRight;
    expect(xForIndex(0, 1, b)).toBeCloseTo(b.padLeft + usable / 2, 4);
  });
  it("linearly distributes across usable width for count > 1", () => {
    // 11 candles → usable / 10 steps
    expect(xForIndex(0, 11, b)).toBeCloseTo(b.padLeft, 4);
    const usable = b.width - b.padLeft - b.padRight;
    expect(xForIndex(10, 11, b)).toBeCloseTo(b.padLeft + usable, 4);
    expect(xForIndex(5, 11, b)).toBeCloseTo(b.padLeft + usable * 0.5, 4);
  });
  it("returns NaN for out-of-range / empty inputs", () => {
    expect(Number.isNaN(xForIndex(-1, 10, b))).toBe(true);
    expect(Number.isNaN(xForIndex(10, 10, b))).toBe(true);
    expect(Number.isNaN(xForIndex(0, 0, b))).toBe(true);
  });
});

describe("kline-anchor: yForPrice", () => {
  const b: AnchorBounds = { ...DEFAULT_BOUNDS, width: 1000, height: 400 };
  it("higher prices map to smaller y (higher on screen)", () => {
    const r = { min: 0, max: 100 };
    const top = yForPrice(100, r, b);
    const bot = yForPrice(0, r, b);
    expect(top).toBeLessThan(bot);
  });
  it("midpoint price maps to vertical midpoint of usable region", () => {
    const r = { min: 0, max: 100 };
    const usable = b.height - b.padTop - b.padBottom;
    expect(yForPrice(50, r, b)).toBeCloseTo(b.padTop + usable / 2, 4);
  });
  it("returns NaN for a zero-span range", () => {
    expect(Number.isNaN(yForPrice(10, { min: 10, max: 10 }, b))).toBe(true);
  });
});

describe("kline-anchor: deriveRange", () => {
  it("includes 4% padding above the max and below the min", () => {
    const r = deriveRange([100, 110, 120], [80, 90, 85]);
    expect(r.min).toBeLessThan(80);
    expect(r.max).toBeGreaterThan(120);
    expect(r.max - r.min).toBeCloseTo((120 - 80) * 1.08, 5);
  });
  it("returns a placeholder range for empty input", () => {
    expect(deriveRange([], [])).toEqual({ min: 0, max: 1 });
  });
});

describe("spreadRowXs", () => {
  const b: AnchorBounds = { ...DEFAULT_BOUNDS, width: 1200, height: 480 };
  it("returns [] for zero count", () => {
    expect(spreadRowXs(0, b, CALLOUT_WIDTH)).toEqual([]);
  });
  it("centers a single callout in the usable width", () => {
    const xs = spreadRowXs(1, b, CALLOUT_WIDTH);
    const usable = b.width - b.padLeft - b.padRight - CALLOUT_WIDTH;
    expect(xs).toHaveLength(1);
    expect(xs[0]).toBeCloseTo(b.padLeft + usable / 2, 4);
  });
  it("evenly steps n>1 callouts across the usable width", () => {
    const xs = spreadRowXs(3, b, CALLOUT_WIDTH);
    expect(xs).toHaveLength(3);
    expect(xs[0]).toBeCloseTo(b.padLeft, 4);
    const last = b.width - b.padLeft - b.padRight - CALLOUT_WIDTH;
    expect(xs[2]).toBeCloseTo(b.padLeft + last, 4);
    expect(xs[1] - xs[0]).toBeCloseTo(xs[2] - xs[1], 4);
  });
});

describe("Callout", () => {
  it("renders annotation chrome (type, title, body, idx, action)", () => {
    render(<Callout annotation={SAMPLE} />);
    expect(screen.getByText("PATTERN")).toBeInTheDocument();
    expect(screen.getByText("Bull Flag")).toBeInTheDocument();
    expect(screen.getByText("test body")).toBeInTheDocument();
    expect(screen.getByText("idx · 22")).toBeInTheDocument();
    expect(screen.getByText("WATCH")).toBeInTheDocument();
  });
  it("uses red accent for danger annotations", () => {
    const { container } = render(<Callout annotation={DANGER} />);
    const card = container.firstChild as HTMLElement;
    // Card border switches to the danger rgba; we assert the inline style.
    // CSSOM normalises rgba() with spaces; match the canonical form.
    expect(card.style.border).toMatch(/rgba\(255,\s*77,\s*77/);
  });
});

describe("InsightLog", () => {
  it("renders expanded list with N events label", () => {
    render(
      <InsightLog
        annotations={[SAMPLE, DANGER]}
        open={true}
        onToggle={() => {}}
      />,
    );
    expect(screen.getByTestId("insight-log-open")).toBeInTheDocument();
    expect(screen.getByText("Insight Log · 2 events")).toBeInTheDocument();
  });
  it("renders collapsed rail when open=false", () => {
    render(
      <InsightLog
        annotations={[SAMPLE, DANGER]}
        open={false}
        onToggle={() => {}}
      />,
    );
    expect(screen.getByTestId("insight-log-collapsed")).toBeInTheDocument();
    expect(screen.queryByTestId("insight-log-open")).toBeNull();
  });
  it("filters by visibleTypes", () => {
    render(
      <InsightLog
        annotations={[SAMPLE, DANGER]}
        visibleTypes={new Set(["RISK"])}
        open={true}
        onToggle={() => {}}
      />,
    );
    expect(screen.getByText("Liquidation Wall")).toBeInTheDocument();
    expect(screen.queryByText("Bull Flag")).toBeNull();
  });
});

describe("AiEnginePill", () => {
  it("renders the label", () => {
    render(<AiEnginePill />);
    expect(screen.getByText("AI Engine · live")).toBeInTheDocument();
  });
  it("registers the xvnAiPulse keyframe in document.head exactly once", () => {
    render(
      <>
        <AiEnginePill />
        <AiEnginePill />
      </>,
    );
    expect(document.head.querySelectorAll("#xvn-ai-pulse-keyframe").length).toBe(1);
  });
});

describe("AIAnnotationDashboard.typesForFilter", () => {
  it("ALL returns undefined (no filter)", () => {
    expect(typesForFilter("ALL")).toBeUndefined();
  });
  it("PATTERN includes structural callouts too", () => {
    const set = typesForFilter("PATTERN");
    expect(set!.has("PATTERN")).toBe(true);
    expect(set!.has("STRUCTURE")).toBe(true);
    expect(set!.has("RISK")).toBe(false);
  });
  it("FLOW pairs with REVERSION", () => {
    const set = typesForFilter("FLOW");
    expect(set!.has("FLOW")).toBe(true);
    expect(set!.has("REVERSION")).toBe(true);
    expect(set!.has("RISK")).toBe(false);
  });
  it("RISK is exclusively RISK", () => {
    const set = typesForFilter("RISK");
    expect([...set!]).toEqual(["RISK"]);
  });
});

// ─── createKlineAnchor ────────────────────────────────────────────────────────

/** Build a minimal mock Chart that satisfies the convertToPixel / subscribeAction API. */
function makeMockChart(overrides: {
  convertToPixel?: (point: object) => object;
  subscribeAction?: (type: string, cb: () => void) => void;
  unsubscribeAction?: (type: string, cb?: () => void) => void;
  getDom?: () => HTMLElement | null;
} = {}) {
  return {
    convertToPixel: overrides.convertToPixel ?? vi.fn(() => ({ x: 42, y: 99 })),
    subscribeAction: overrides.subscribeAction ?? vi.fn(),
    unsubscribeAction: overrides.unsubscribeAction ?? vi.fn(),
    getDom: overrides.getDom ?? vi.fn(() => null),
  };
}

describe("createKlineAnchor", () => {
  it("returns NaN positions when convertToPixel throws (simulating disposed chart)", () => {
    const chart = makeMockChart({
      convertToPixel: vi.fn(() => { throw new Error("disposed"); }),
    });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const anchor = createKlineAnchor(chart as any);
    expect(Number.isNaN(anchor.xForIndex(0))).toBe(true);
    expect(Number.isNaN(anchor.yForPrice(100))).toBe(true);
  });

  it("returns NaN when convertToPixel returns an object with no x/y", () => {
    const chart = makeMockChart({
      convertToPixel: vi.fn(() => ({})),
    });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const anchor = createKlineAnchor(chart as any);
    expect(Number.isNaN(anchor.xForIndex(5))).toBe(true);
    expect(Number.isNaN(anchor.yForPrice(200))).toBe(true);
  });

  it("returns pixel coordinates from chart.convertToPixel when available", () => {
    const chart = makeMockChart({
      convertToPixel: vi.fn(() => ({ x: 123, y: 456 })),
    });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const anchor = createKlineAnchor(chart as any);
    expect(anchor.xForIndex(3)).toBe(123);
    expect(anchor.yForPrice(50)).toBe(456);
  });

  it("subscribeLayout calls chart.subscribeAction with onVisibleRangeChange and fires cb", () => {
    let capturedCb: (() => void) | undefined;
    const chart = makeMockChart({
      subscribeAction: vi.fn((_type: string, cb: () => void) => {
        capturedCb = cb;
      }),
      unsubscribeAction: vi.fn(),
    });
    const cb = vi.fn();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const anchor = createKlineAnchor(chart as any);
    const unsub = anchor.subscribeLayout(cb);

    expect(chart.subscribeAction).toHaveBeenCalledWith("onVisibleRangeChange", cb);

    // Simulate a visible-range change event by calling the captured callback.
    capturedCb?.();
    expect(cb).toHaveBeenCalledTimes(1);

    // Cleanup should unsubscribe.
    unsub();
    expect(chart.unsubscribeAction).toHaveBeenCalledWith("onVisibleRangeChange", cb);
  });
});

// ─── AnnotationOverlay fallback path ─────────────────────────────────────────

import { AnnotationOverlay } from "./AnnotationOverlay";
import type { CandleColumns } from "../types";

const CANDLES: CandleColumns = {
  time: [1_700_000_000, 1_700_000_060, 1_700_000_120],
  open: [100, 101, 102],
  high: [103, 104, 105],
  low:  [99, 100, 101],
  close: [101, 102, 103],
  volume: [10, 12, 11],
};

describe("AnnotationOverlay fallback path", () => {
  beforeEach(() => {
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

  it("renders the overlay host without a chart prop (geometric fallback)", () => {
    render(
      <AnnotationOverlay
        candles={CANDLES}
        annotations={[SAMPLE]}
      />,
    );
    expect(screen.getByTestId("annotation-overlay-host")).toBeInTheDocument();
  });

  it("renders the overlay host when chart prop is explicitly null (fallback)", () => {
    render(
      <AnnotationOverlay
        candles={CANDLES}
        annotations={[SAMPLE]}
        chart={null}
      />,
    );
    expect(screen.getByTestId("annotation-overlay-host")).toBeInTheDocument();
  });
});

// ─── KlineCandlePane onReady callback ────────────────────────────────────────

// We need the klinecharts mock for KlineCandlePane — use vi.hoisted to keep it
// module-scoped and avoid top-level side-effects.
const klineMocksForReady = vi.hoisted(() => {
  const chart = {
    resize: vi.fn(),
    setStyles: vi.fn(),
    setSymbol: vi.fn(),
    setPeriod: vi.fn(),
    setDataLoader: vi.fn(),
    // KlineCandlePane's data effect calls createOverlay per overlay/marker/band
    // and removeOverlay in its cleanup. These fixtures pass no overlay props so
    // neither fires here, but a complete mock keeps the surface resilient.
    createOverlay: vi.fn(() => "overlay-id"),
    removeOverlay: vi.fn(),
  };
  return {
    chart,
    init: vi.fn(() => chart),
    dispose: vi.fn(),
    registerOverlay: vi.fn(),
  };
});

vi.mock("klinecharts", () => ({
  init: klineMocksForReady.init,
  dispose: klineMocksForReady.dispose,
  // registerOverlay runs at module scope when KlineCandlePane is imported;
  // it's a real named export of klinecharts, so the mock must provide a no-op.
  registerOverlay: klineMocksForReady.registerOverlay,
}));

import { KlineCandlePane } from "./KlineCandlePane";

describe("KlineCandlePane onReady callback", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    klineMocksForReady.init.mockReturnValue(klineMocksForReady.chart);
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

  it("fires onReady with a non-null chart instance on mount", async () => {
    const onReady = vi.fn();
    render(<KlineCandlePane candles={CANDLES} onReady={onReady} />);
    await waitFor(() => {
      expect(onReady).toHaveBeenCalledWith(klineMocksForReady.chart);
    });
    expect(onReady).toHaveBeenCalledTimes(1);
  });

  it("fires onReady with null on unmount", async () => {
    const onReady = vi.fn();
    const { unmount } = render(<KlineCandlePane candles={CANDLES} onReady={onReady} />);
    await waitFor(() => {
      expect(onReady).toHaveBeenCalledWith(klineMocksForReady.chart);
    });
    act(() => unmount());
    expect(onReady).toHaveBeenLastCalledWith(null);
    expect(onReady).toHaveBeenCalledTimes(2);
  });
});
