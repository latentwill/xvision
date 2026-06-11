import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RunChartPayload } from "@/api/types.gen";
import { PulseDrawdownChart } from "./PulseDrawdownChart";
import { PulseFieldChart } from "./PulseFieldChart";
import { PulseHoldChart } from "./PulseHoldChart";
import { PulseTradesChart } from "./PulseTradesChart";

// ── jsdom environment mocks ────────────────────────────────────────────────
// Mirror what KlineCandlePane.test.tsx sets up: stub klinecharts so KlineCandlePane
// can mount without a real canvas, and stub ResizeObserver for usePlot/KlineCandlePane.

vi.mock("klinecharts", () => ({
  init: vi.fn(() => ({
    resize: vi.fn(),
    setStyles: vi.fn(),
    setSymbol: vi.fn(),
    setPeriod: vi.fn(),
    setDataLoader: vi.fn(),
    createOverlay: vi.fn(() => "overlay-0"),
    removeOverlay: vi.fn(),
    getDataList: vi.fn(() => []),
    getOffsetRightDistance: vi.fn(() => 0),
    setOffsetRightDistance: vi.fn(),
    getBarSpace: vi.fn(() => ({ bar: 8 })),
    scrollToRealTime: vi.fn(),
    getDom: vi.fn(() => ({ clientWidth: 800 })),
  })),
  dispose: vi.fn(),
  registerOverlay: vi.fn(),
}));

// Stub uPlot so usePlot can be called without a real canvas/DOM environment.
vi.mock("uplot", () => ({
  default: vi.fn().mockImplementation(() => ({
    destroy: vi.fn(),
    setSize: vi.fn(),
    setScale: vi.fn(),
    data: [[], []],
    scales: { x: { min: 0, max: 100 }, y: { min: -10, max: 10 } },
    series: [],
    ctx: {},
    bbox: { left: 0, top: 0, width: 300, height: 200 },
  })),
}));

// Stub ResizeObserver (not in jsdom).
class ResizeObserverStub {
  observe() {}
  disconnect() {}
}
Object.defineProperty(globalThis, "ResizeObserver", {
  value: ResizeObserverStub,
  configurable: true,
  writable: true,
});

// ── test payload ────────────────────────────────────────────────────────────

function slimPayload(over: Partial<RunChartPayload> = {}): RunChartPayload {
  return {
    run_id: "r1",
    scenario_id: "s1",
    asset: "ETH",
    granularity: "1h",
    time_window: { start: "2025-01-01T00:00:00Z", end: "2025-01-02T00:00:00Z" },
    bars: [],
    indicators: {
      sma_20: [], sma_30: [], sma_50: [], sma_60: [], sma_90: [], sma_200: [],
      ema_20: [], ema_30: [], ema_50: [], ema_60: [], ema_90: [], ema_200: [],
      bollinger: { upper: [], middle: [], lower: [] },
      donchian: { upper: [], lower: [] },
      rsi_14: [],
      macd: { line: [], signal: [], histogram: [] },
      atr_14: [],
    },
    equity: [
      { time: 100, equity_usd: 100_000 },
      { time: 200, equity_usd: 105_000 },
    ],
    drawdown: [],
    position: [],
    markers: { trades: [], vetoes: [], holds: [] },
    baseline_equity: null,
    ...over,
  } as RunChartPayload;
}

// ── tests ───────────────────────────────────────────────────────────────────

describe("pulse view charts", () => {
  it("PulseTradesChart renders a candle host for a bars payload", () => {
    const payload = slimPayload({
      bars: [
        { time: 100, open: 1, high: 2, low: 0.5, close: 1.5, volume: 10 },
        { time: 200, open: 1.5, high: 2.5, low: 1, close: 2, volume: 12 },
      ],
      markers: {
        trades: [
          {
            time: 100, side: "Buy", price: 1.2, size: 1, fee: 0,
            pnl_realized: null, decision_index: 0, justification: null,
          },
        ],
        vetoes: [],
        holds: [],
      },
    });
    render(<PulseTradesChart payload={payload} />);
    expect(screen.getByTestId("pulse-trades-chart")).toBeInTheDocument();
  });

  it("PulseHoldChart renders for an equity+baseline payload", () => {
    const payload = slimPayload({
      baseline_equity: [
        { time: 100, equity_usd: 100_000 },
        { time: 200, equity_usd: 101_000 },
      ],
    });
    render(<PulseHoldChart payload={payload} />);
    expect(screen.getByTestId("pulse-hold-chart")).toBeInTheDocument();
  });

  it("PulseDrawdownChart renders from a slim equity payload", () => {
    render(<PulseDrawdownChart payload={slimPayload()} />);
    expect(screen.getByTestId("pulse-drawdown-chart")).toBeInTheDocument();
  });

  it("PulseFieldChart renders an overlay and inline caption row", () => {
    render(
      <PulseFieldChart
        runs={[
          {
            runId: "r1",
            label: "Alpha",
            equity: [
              { time: 100, equity_usd: 100_000 },
              { time: 200, equity_usd: 104_000 },
            ],
          },
          {
            runId: "r2",
            label: "Beta",
            equity: [
              { time: 50, equity_usd: 100_000 },
              { time: 60, equity_usd: 98_000 },
            ],
          },
        ]}
        heroRunId="r1"
      />,
    );
    expect(screen.getByTestId("pulse-field-chart")).toBeInTheDocument();
    expect(screen.getByTestId("pulse-field-caption")).toHaveTextContent("Alpha");
  });
});
