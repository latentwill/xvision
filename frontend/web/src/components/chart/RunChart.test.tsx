import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

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
  baselineSeries: [] as Array<{ setData: ReturnType<typeof vi.fn> }>,
  series: [] as Array<ReturnType<typeof createSeriesStub>>,
  createdCharts: [] as Array<{
    getCurrentRange: () => LogicalRangeStub | null;
    emitVisibleLogicalRangeChange: (range: LogicalRangeStub | null) => void;
    emitClick: (param: { hoveredObjectId?: unknown }) => void;
    timeScaleApi: {
      scrollToRealTime: ReturnType<typeof vi.fn>;
      fitContent: ReturnType<typeof vi.fn>;
      getVisibleLogicalRange: ReturnType<typeof vi.fn>;
      setVisibleLogicalRange: ReturnType<typeof vi.fn>;
    };
    remove: ReturnType<typeof vi.fn>;
  }>,
}));

type LogicalRangeStub = { from: number; to: number };
const realtimeRange: LogicalRangeStub = { from: 90, to: 110 };

function createSeriesStub() {
  const series = {
    setData: vi.fn(),
    update: vi.fn(),
    createPriceLine: vi.fn(),
    setMarkers: vi.fn(),
  };
  chartMocks.series.push(series);
  return series;
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
  const clickHandlers: Array<(param: { hoveredObjectId?: unknown }) => void> = [];
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
    addBaselineSeries: vi.fn(() => {
      const series = createSeriesStub();
      chartMocks.baselineSeries.push(series);
      return series;
    }),
    addHistogramSeries: vi.fn(() => createSeriesStub()),
    subscribeClick: vi.fn((handler: (param: { hoveredObjectId?: unknown }) => void) => {
      clickHandlers.push(handler);
    }),
    unsubscribeClick: vi.fn((handler: (param: { hoveredObjectId?: unknown }) => void) => {
      const idx = clickHandlers.indexOf(handler);
      if (idx >= 0) clickHandlers.splice(idx, 1);
    }),
    getCurrentRange: () => currentRange,
    emitVisibleLogicalRangeChange: (range: LogicalRangeStub | null) => {
      currentRange = range;
      visibleRangeHandlers.forEach((handler) => handler(range));
    },
    emitClick: (param: { hoveredObjectId?: unknown }) => {
      clickHandlers.forEach((handler) => handler(param));
    },
    timeScaleApi,
    timeScale: vi.fn(() => timeScaleApi),
    priceScale: vi.fn(() => ({ applyOptions: vi.fn() })),
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
import { ChartContainer } from "./ChartContainer";

describe("RunChart", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    chartMocks.createdCharts.length = 0;
    chartMocks.subscribeRegistrationCount = 0;
    chartMocks.subscribeRegistrationCountsAtScroll.length = 0;
    chartMocks.initialRangesQueue.length = 0;
    chartMocks.baselineSeries.length = 0;
    chartMocks.series.length = 0;
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

  it("keeps the position band off the asset price scale", () => {
    const payload = {
      ...(samplePayload as any),
      position: [
        { time: 1_700_000_000, side: "Long" },
        { time: 1_700_000_060, side: "Short" },
      ],
    };

    render(<RunChart payload={payload} />);

    const priceChart = chartMocks.createdCharts[0] as ReturnType<typeof createChartStub>;
    expect(priceChart.addAreaSeries).toHaveBeenCalledTimes(2);

    const areaCalls = priceChart.addAreaSeries.mock.calls as unknown as Array<
      [
        {
          autoscaleInfoProvider: () => unknown;
          crosshairMarkerVisible?: boolean;
          lastValueVisible?: boolean;
          priceLineVisible?: boolean;
          priceScaleId?: string;
        },
      ]
    >;
    areaCalls.forEach(([options]) => {
      expect(options).toEqual(
        expect.objectContaining({
          priceScaleId: "position-band",
          lastValueVisible: false,
          priceLineVisible: false,
          crosshairMarkerVisible: false,
        }),
      );
      expect(options.autoscaleInfoProvider()).toEqual({
        priceRange: { minValue: 0, maxValue: 1 },
      });
    });

    const [longSeries, shortSeries] = priceChart.addAreaSeries.mock.results.map(
      ({ value }) => value,
    );
    expect(longSeries.setData).toHaveBeenCalledWith([
      { time: 1_700_000_000, value: 1 },
    ]);
    expect(shortSeries.setData).toHaveBeenCalledWith([
      { time: 1_700_000_060, value: 1 },
    ]);
  });

  it("opens and closes the marker side panel from a chart marker click", () => {
    const payload = {
      ...(samplePayload as any),
      markers: {
        trades: [
          {
            time: 1_700_000_000,
            side: "Buy",
            price: 50_000,
            size: 0.25,
            fee: 1.5,
            pnl_realized: null,
            decision_index: 7,
            justification: "Entry signal",
          },
        ],
        vetoes: [],
        holds: [],
      },
    };

    render(<RunChart payload={payload} />);

    const markerSeries = chartMocks.series.find(
      (series) => series.setMarkers.mock.calls.length > 0,
    );
    expect(markerSeries?.setMarkers).toHaveBeenCalledWith(
      expect.arrayContaining([
        expect.objectContaining({ id: "trade:7", text: "Buy 0.25" }),
      ]),
    );

    act(() => {
      chartMocks.createdCharts[0]?.emitClick({ hoveredObjectId: "trade:7" });
    });

    expect(screen.getByText("Decision #7")).toBeInTheDocument();
    expect(screen.getByText("Entry signal")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "×" }));

    expect(screen.queryByText("Decision #7")).not.toBeInTheDocument();
  });

  it("renders chart shell range controls and data table toggle", () => {
    render(
      <ChartContainer
        title="Run chart"
        range="All"
        onRange={vi.fn()}
        layersPanel={<div>layers</div>}
        dataTable={<table><tbody><tr><td>row</td></tr></tbody></table>}
      >
        <div>chart</div>
      </ChartContainer>,
    );

    expect(screen.getByRole("button", { name: "1d" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "All" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Data table" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Data table" }));

    expect(screen.getByText("row")).toBeInTheDocument();
  });

  it("applies range preset buttons to every current chart viewport", () => {
    const bars = Array.from({ length: 48 }, (_, index) => ({
      time: 1_700_000_000 + index * 3_600,
      open: 100 + index,
      high: 101 + index,
      low: 99 + index,
      close: 100.5 + index,
      volume: 10,
    }));
    const indicatorPoints = bars.map((bar) => ({
      time: bar.time,
      value: bar.close,
    }));
    const payload = {
      ...(samplePayload as any),
      granularity: "1h",
      bars,
      indicators: {
        ...(samplePayload as any).indicators,
        sma_20: indicatorPoints,
        sma_50: indicatorPoints,
        sma_200: indicatorPoints,
        rsi_14: indicatorPoints,
      },
    };

    render(<RunChart payload={payload} follow={false} />);
    const charts = [...chartMocks.createdCharts];

    charts.forEach((chart) => chart.timeScaleApi.setVisibleLogicalRange.mockClear());
    fireEvent.click(screen.getByRole("button", { name: "1d" }));

    charts.forEach((chart) => {
      expect(chart.timeScaleApi.setVisibleLogicalRange).toHaveBeenCalledWith({
        from: 24,
        to: 50,
      });
    });

    charts.forEach((chart) => chart.timeScaleApi.fitContent.mockClear());
    chartMocks.fitContent.mockClear();
    fireEvent.click(screen.getByRole("button", { name: "All" }));

    charts.forEach((chart) => {
      expect(chart.timeScaleApi.fitContent).toHaveBeenCalledTimes(1);
    });
    expect(chartMocks.fitContent).toHaveBeenCalledTimes(charts.length);
  });

  it("persists layer toggles to localStorage", () => {
    const key = storageKey("run-detail");
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    render(<RunChart payload={samplePayload as any} />);

    // Open the Layers panel.
    fireEvent.click(screen.getByText(/Layers/));

    // Default for sma20 is true (per DEFAULT_LAYERS in chart-layers.ts).
    const sma20Checkbox = screen.getByLabelText("SMA 20") as HTMLInputElement;
    expect(sma20Checkbox.checked).toBe(true);

    // Toggle it off; the useChartLayers effect writes to localStorage.
    fireEvent.click(sma20Checkbox);

    const raw = localStorage.getItem(key);
    expect(raw).not.toBeNull();
    const persisted = JSON.parse(raw!) as Record<string, boolean>;
    expect(persisted.sma20).toBe(false);
  });

  it("plots the earnings baseline series as equity_usd - startingEquity", () => {
    const payload = {
      ...(samplePayload as any),
      equity: [
        { time: 1_700_000_000, equity_usd: 100_000 },
        { time: 1_700_000_060, equity_usd: 100_500 },
        { time: 1_700_000_120, equity_usd: 99_750 },
      ],
    };

    render(<RunChart payload={payload} follow={false} />);

    expect(chartMocks.baselineSeries.length).toBeGreaterThan(0);
    const earningsSeries = chartMocks.baselineSeries[0];
    expect(earningsSeries.setData).toHaveBeenCalledWith([
      { time: 1_700_000_000, value: 0 },
      { time: 1_700_000_060, value: 500 },
      { time: 1_700_000_120, value: -250 },
    ]);
  });

  it("removes optional panes and skips chart creation when their layers are disabled", () => {
    render(<RunChart payload={samplePayload as any} follow={false} />);

    expect(screen.getByTestId("run-chart-subpane")).toBeInTheDocument();
    expect(screen.getByTestId("run-chart-equity-pane")).toBeInTheDocument();
    expect(screen.getByTestId("run-chart-drawdown-pane")).toBeInTheDocument();

    fireEvent.click(screen.getByText(/Layers/));
    fireEvent.click(screen.getByLabelText("Off"));
    fireEvent.click(screen.getByLabelText("Earnings"));
    fireEvent.click(screen.getByLabelText("Drawdown"));

    expect(screen.queryByTestId("run-chart-subpane")).not.toBeInTheDocument();
    expect(screen.queryByTestId("run-chart-equity-pane")).not.toBeInTheDocument();
    expect(screen.queryByTestId("run-chart-drawdown-pane")).not.toBeInTheDocument();
    expect(
      chartMocks.createdCharts.filter((chart) => chart.remove.mock.calls.length === 0),
    ).toHaveLength(1);
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
    chartMocks.fitContent.mockClear();

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
    expect(chartMocks.fitContent).not.toHaveBeenCalled();
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
