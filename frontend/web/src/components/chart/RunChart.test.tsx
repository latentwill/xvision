import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { storageKey } from "./chart-layers";
import samplePayload from "./__fixtures__/sample-run-chart.json";

const chartMocks = vi.hoisted(() => ({
  createChart: vi.fn(),
  scrollToRealTime: vi.fn(),
  fitContent: vi.fn(),
  subscribeVisibleLogicalRangeChange: vi.fn(),
  setVisibleLogicalRange: vi.fn(),
  subscribeRegistrationCount: 0,
  subscribeRegistrationCountsAtScroll: [] as number[],
  initialRangesQueue: [] as Array<LogicalRangeStub | null>,
  createdCharts: [] as Array<{
    getCurrentRange: () => LogicalRangeStub | null;
    emitVisibleLogicalRangeChange: (range: LogicalRangeStub | null) => void;
    timeScaleApi: {
      scrollToRealTime: ReturnType<typeof vi.fn>;
      getVisibleLogicalRange: ReturnType<typeof vi.fn>;
      setVisibleLogicalRange: ReturnType<typeof vi.fn>;
    };
    remove: ReturnType<typeof vi.fn>;
  }>,
}));

type LogicalRangeStub = { from: number; to: number };
const realtimeRange: LogicalRangeStub = { from: 90, to: 110 };

function createSeriesStub() {
  return {
    setData: vi.fn(),
    update: vi.fn(),
    createPriceLine: vi.fn(),
    setMarkers: vi.fn(),
  };
}

function createChartStub() {
  const initialRange = chartMocks.initialRangesQueue.length > 0
    ? chartMocks.initialRangesQueue.shift() ?? null
    : {
        from: chartMocks.createdCharts.length * 10,
        to: chartMocks.createdCharts.length * 10 + 20,
      };
  let currentRange: LogicalRangeStub | null = initialRange;
  const visibleRangeHandlers: Array<
    (range: LogicalRangeStub | null) => void
  > = [];
  const timeScaleApi = {
    scrollToRealTime: vi.fn(() => {
      chartMocks.subscribeRegistrationCountsAtScroll.push(
        chartMocks.subscribeRegistrationCount,
      );
      chartMocks.scrollToRealTime();
    }),
    fitContent: vi.fn(() => chartMocks.fitContent()),
    subscribeVisibleLogicalRangeChange: vi.fn(
      (handler: (range: LogicalRangeStub | null) => void) => {
        visibleRangeHandlers.push(handler);
        chartMocks.subscribeRegistrationCount += 1;
        chartMocks.subscribeVisibleLogicalRangeChange(handler);
      },
    ),
    setVisibleLogicalRange: vi.fn((range: LogicalRangeStub) => {
      currentRange = range;
      chartMocks.setVisibleLogicalRange(range);
    }),
    getVisibleLogicalRange: vi.fn(() => currentRange),
  };

  return {
    addCandlestickSeries: vi.fn(() => createSeriesStub()),
    addLineSeries: vi.fn(() => createSeriesStub()),
    addAreaSeries: vi.fn(() => createSeriesStub()),
    addHistogramSeries: vi.fn(() => createSeriesStub()),
    getCurrentRange: () => currentRange,
    emitVisibleLogicalRangeChange: (range: LogicalRangeStub | null) => {
      currentRange = range;
      visibleRangeHandlers.forEach((handler) => handler(range));
    },
    timeScaleApi,
    timeScale: vi.fn(() => timeScaleApi),
    remove: vi.fn(),
  };
}

function expectChartsToHaveRange(
  charts: Array<(typeof chartMocks.createdCharts)[number]>,
  expectedRange: LogicalRangeStub,
) {
  expect(charts.length).toBeGreaterThan(0);
  charts.forEach((chart) => {
    expect(chart.getCurrentRange()).toEqual(expectedRange);
  });
}

vi.mock("lightweight-charts", () => ({
  ColorType: { Solid: "solid" },
  CrosshairMode: { Normal: 0 },
  createChart: chartMocks.createChart,
}));

// The test file is imported after vi.mock — pull RunChart in via a
// dynamic import so the mock is hoisted correctly under all vitest
// transform paths.
import { RunChart } from "./RunChart";

describe("RunChart", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    chartMocks.createdCharts.length = 0;
    chartMocks.subscribeRegistrationCount = 0;
    chartMocks.subscribeRegistrationCountsAtScroll.length = 0;
    chartMocks.initialRangesQueue.length = 0;
    chartMocks.createChart.mockImplementation(() => {
      const chart = createChartStub();
      chartMocks.createdCharts.push(chart);
      return chart;
    });
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

  it("does not scroll to real time while follow mode is disabled", () => {
    const payload = samplePayload as any;
    const { rerender } = render(<RunChart payload={payload} follow={false} />);

    rerender(<RunChart payload={payload} follow={false} themeMode="light" />);

    expect(chartMocks.scrollToRealTime).not.toHaveBeenCalled();
  });

  it("updates existing chart instances in place when only payload data changes", () => {
    const payload = samplePayload as any;
    const { rerender } = render(<RunChart payload={payload} follow />);
    const createChartCallsBeforePayloadUpdate =
      chartMocks.createChart.mock.calls.length;

    rerender(
      <RunChart
        payload={{
          ...payload,
          bars: [
            {
              time: 1_700_000_000,
              open: 100,
              high: 105,
              low: 95,
              close: 102,
              volume: 12,
            },
          ],
          equity: [
            {
              time: 1_700_000_000,
              equity_usd: 100_000,
            },
          ],
        }}
        follow
      />,
    );

    expect(chartMocks.createChart.mock.calls.length).toBe(
      createChartCallsBeforePayloadUpdate,
    );
  });

  it("restores the frozen visible logical range across rebuilds after follow mode is disabled", () => {
    const payload = samplePayload as any;
    const frozenRange = { from: 12, to: 34 };
    const { rerender } = render(<RunChart payload={payload} follow />);
    const initialCharts = [...chartMocks.createdCharts];
    const scrollCallsBeforeFreeze = chartMocks.scrollToRealTime.mock.calls.length;

    rerender(<RunChart payload={payload} follow={false} />);

    initialCharts[0]?.timeScaleApi.getVisibleLogicalRange.mockReturnValue(
      frozenRange,
    );

    const createChartCallsBeforeRebuild =
      chartMocks.createChart.mock.calls.length;

    rerender(<RunChart payload={payload} follow={false} themeMode="light" />);

    const rebuiltCharts = chartMocks.createdCharts.slice(
      createChartCallsBeforeRebuild,
    );

    expect(
      initialCharts[0]?.timeScaleApi.getVisibleLogicalRange,
    ).toHaveBeenCalled();
    expect(rebuiltCharts.length).toBeGreaterThan(0);
    rebuiltCharts.forEach((chart: (typeof chartMocks.createdCharts)[number]) => {
      expect(chart.timeScaleApi.setVisibleLogicalRange).toHaveBeenCalledWith(
        frozenRange,
      );
    });
    expect(chartMocks.scrollToRealTime.mock.calls.length).toBe(
      scrollCallsBeforeFreeze,
    );
  });

  it("captures and restores the frozen visible logical range when follow mode is disabled during the same rerender that rebuilds charts", () => {
    const payload = samplePayload as any;
    const frozenRange = { from: 21, to: 55 };
    const { rerender } = render(<RunChart payload={payload} follow />);
    const initialCharts = [...chartMocks.createdCharts];
    const scrollCallsBeforeRebuild =
      chartMocks.scrollToRealTime.mock.calls.length;

    initialCharts[0]?.timeScaleApi.getVisibleLogicalRange.mockReturnValue(
      frozenRange,
    );

    const createChartCallsBeforeRebuild =
      chartMocks.createChart.mock.calls.length;

    rerender(<RunChart payload={payload} follow={false} themeMode="light" />);

    const rebuiltCharts = chartMocks.createdCharts.slice(
      createChartCallsBeforeRebuild,
    );

    expect(
      initialCharts[0]?.timeScaleApi.getVisibleLogicalRange,
    ).toHaveBeenCalled();
    expect(rebuiltCharts.length).toBeGreaterThan(0);
    rebuiltCharts.forEach((chart: (typeof chartMocks.createdCharts)[number]) => {
      expect(chart.timeScaleApi.setVisibleLogicalRange).toHaveBeenCalledWith(
        frozenRange,
      );
    });
    expect(chartMocks.scrollToRealTime.mock.calls.length).toBe(
      scrollCallsBeforeRebuild,
    );
  });

  it("applies the same follow viewport across the current chart set when follow mode toggles on", () => {
    const payload = samplePayload as any;
    const { rerender } = render(<RunChart payload={payload} follow={false} />);
    const initialCharts = [...chartMocks.createdCharts];
    const initialAnchorRange = initialCharts[0]?.getCurrentRange();

    rerender(<RunChart payload={payload} follow />);

    expect(chartMocks.scrollToRealTime).toHaveBeenCalledTimes(1);
    expect(initialCharts[0]?.timeScaleApi.scrollToRealTime).toHaveBeenCalledTimes(
      1,
    );
    expectChartsToHaveRange(initialCharts, initialAnchorRange!);

    initialCharts[0]?.emitVisibleLogicalRangeChange(realtimeRange);

    expectChartsToHaveRange(initialCharts, realtimeRange);
  });

  it("re-enters follow mode when an already-live chart is frozen and then resumed", () => {
    const payload = samplePayload as any;
    const { rerender } = render(<RunChart payload={payload} follow />);
    const initialCharts = [...chartMocks.createdCharts];

    expect(chartMocks.scrollToRealTime).toHaveBeenCalledTimes(1);

    rerender(<RunChart payload={payload} follow={false} />);
    rerender(<RunChart payload={payload} follow />);

    expect(chartMocks.scrollToRealTime).toHaveBeenCalledTimes(2);
    expect(initialCharts[0]?.timeScaleApi.scrollToRealTime).toHaveBeenCalledTimes(
      2,
    );
  });

  it("applies the same follow viewport across all charts on mount and rebuilds while enabled", () => {
    const payload = samplePayload as any;
    const { rerender } = render(<RunChart payload={payload} follow />);
    const initialCharts = [...chartMocks.createdCharts];

    expect(chartMocks.scrollToRealTime).toHaveBeenCalledTimes(1);
    expect(initialCharts[0]?.timeScaleApi.scrollToRealTime).toHaveBeenCalledTimes(
      1,
    );
    expectChartsToHaveRange(initialCharts, initialCharts[0]!.getCurrentRange()!);

    const createChartCallsBeforeThemeChange =
      chartMocks.createChart.mock.calls.length;

    rerender(
      <RunChart payload={payload} follow themeMode="light" />,
    );

    expect(chartMocks.createChart.mock.calls.length).toBeGreaterThan(
      createChartCallsBeforeThemeChange,
    );
    const themeRebuiltCharts = chartMocks.createdCharts.slice(
      createChartCallsBeforeThemeChange,
    );

    expect(
      themeRebuiltCharts[0]?.timeScaleApi.scrollToRealTime,
    ).toHaveBeenCalledTimes(1);
    expect(chartMocks.scrollToRealTime).toHaveBeenCalledTimes(2);
    expectChartsToHaveRange(
      themeRebuiltCharts,
      themeRebuiltCharts[0]!.getCurrentRange()!,
    );
  });

  it("does not enter follow mode twice when follow toggles on during the same rerender that rebuilds charts", () => {
    const payload = samplePayload as any;
    const { rerender } = render(<RunChart payload={payload} follow={false} />);
    const createChartCallsBeforeRebuild = chartMocks.createChart.mock.calls.length;

    rerender(
      <RunChart
        payload={payload}
        follow
        themeMode="light"
      />,
    );

    const rebuiltCharts = chartMocks.createdCharts.slice(
      createChartCallsBeforeRebuild,
    );

    expect(chartMocks.scrollToRealTime).toHaveBeenCalledTimes(1);
    expect(
      rebuiltCharts[0]?.timeScaleApi.scrollToRealTime,
    ).toHaveBeenCalledTimes(1);
    expectChartsToHaveRange(rebuiltCharts, rebuiltCharts[0]!.getCurrentRange()!);
  });

  it("applies the anchor's immediate visible range to peers even when scroll-to-realtime emits no event", () => {
    const payload = samplePayload as any;
    chartMocks.initialRangesQueue.push(realtimeRange);
    render(<RunChart payload={payload} follow />);
    const charts = [...chartMocks.createdCharts];

    expect(chartMocks.scrollToRealTime).toHaveBeenCalledTimes(1);
    expect(charts[0]?.timeScaleApi.scrollToRealTime).toHaveBeenCalledTimes(1);
    expectChartsToHaveRange(charts, realtimeRange);
  });

  it("does not bounce synchronized range updates back to the originating chart", () => {
    const payload = samplePayload as any;
    render(<RunChart payload={payload} />);
    const charts = [...chartMocks.createdCharts];
    const anchor = charts[0]!;
    const peer = charts[1]!;

    anchor.emitVisibleLogicalRangeChange(realtimeRange);
    expect(peer.timeScaleApi.setVisibleLogicalRange).toHaveBeenCalledWith(
      realtimeRange,
    );

    anchor.timeScaleApi.setVisibleLogicalRange.mockClear();
    peer.emitVisibleLogicalRangeChange(realtimeRange);

    expect(anchor.timeScaleApi.setVisibleLogicalRange).not.toHaveBeenCalled();
  });
});
