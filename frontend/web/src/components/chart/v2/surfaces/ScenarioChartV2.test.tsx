/**
 * Render contract tests for ScenarioChartV2 (Task 7 — v1 parity).
 *
 * The candle pane (KlineCandlePane) instantiates klinecharts and the
 * equity/volume panes instantiate uPlot — neither plays well in jsdom
 * (canvas + matchMedia). We stub them to lightweight markers so we can
 * assert the surface's composition contract: candle pane present, bars
 * data table rendered, overlays/overlayActive threaded through, and the
 * equity pane suppressed when there is no equity series.
 */
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";

// Capture the props KlineCandlePane is called with so we can assert the
// overlays/overlayActive maps are threaded through from the payload + layers.
const candlePropsSpy = vi.fn();

vi.mock("../primitives/KlineCandlePane", () => ({
  KlineCandlePane: (props: Record<string, unknown>) => {
    candlePropsSpy(props);
    return <div data-testid="kline-candle-pane" />;
  },
}));

vi.mock("../primitives/UplotEquityPane", () => ({
  UplotEquityPane: () => <div data-testid="uplot-equity-pane" />,
}));

vi.mock("../primitives/UplotHistogramPane", () => ({
  UplotHistogramPane: () => <div data-testid="uplot-histogram-pane" />,
}));

import { ScenarioChartV2 } from "./ScenarioChartV2";
import type { ScenarioChartV2Payload, IndicatorMap } from "../types";

function makePayload(
  overrides: Partial<ScenarioChartV2Payload> = {},
): ScenarioChartV2Payload {
  const time = [1700000000, 1700003600, 1700007200];
  const indicators: IndicatorMap = {
    sma20: { time, value: [100, 101, 102] },
    bollUpper: { time, value: [110, 111, 112] },
    bollMiddle: { time, value: [100, 100, 100] },
    bollLower: { time, value: [90, 89, 88] },
  };
  return {
    kind: "scenario",
    asset: "BTC",
    granularity: "1h",
    candles: {
      time,
      open: [100, 101, 102],
      high: [105, 106, 107],
      low: [95, 96, 97],
      close: [101, 102, 103.5],
      volume: [10, 20, 30],
    },
    indicators,
    markers: [],
    positions: [],
    equity: [],
    ...overrides,
  };
}

describe("ScenarioChartV2", () => {
  beforeEach(() => {
    candlePropsSpy.mockClear();
    localStorage.clear();
  });

  it("renders the candle pane and a bars DataTable", () => {
    render(<ScenarioChartV2 payload={makePayload()} />);
    expect(screen.getByTestId("kline-candle-pane")).toBeInTheDocument();
    // The bars table is wired through ChartFrame's inline dataTable slot,
    // collapsed by default. Expand it (in-flow, not a popup) before asserting.
    fireEvent.click(screen.getByRole("button", { name: "Data table" }));
    // DataTable header cell from the bars columns.
    expect(screen.getByText("Time")).toBeInTheDocument();
    // A known close value must appear in the table body.
    expect(screen.getByText("103.5")).toBeInTheDocument();
  });

  it("does NOT render an equity pane when payload.equity is empty", () => {
    // equity defaults to layers.equity === true; the surface must still
    // suppress the pane because the series is empty.
    render(<ScenarioChartV2 payload={makePayload({ equity: [] })} />);
    expect(screen.queryByTestId("uplot-equity-pane")).not.toBeInTheDocument();
  });

  it("threads overlays + overlayActive maps into KlineCandlePane", () => {
    render(<ScenarioChartV2 payload={makePayload()} />);
    expect(candlePropsSpy).toHaveBeenCalled();
    const props = candlePropsSpy.mock.calls[0][0] as Record<string, unknown>;
    const overlays = props.overlays as Record<string, unknown>;
    const overlayActive = props.overlayActive as Record<string, boolean>;
    expect(overlays).toBeTruthy();
    // The sma20 series from the payload is surfaced as an overlay.
    expect(overlays.sma20).toEqual({
      time: [1700000000, 1700003600, 1700007200],
      value: [100, 101, 102],
    });
    // overlayActive must carry the toggle subset keys.
    expect(overlayActive).toHaveProperty("sma20");
    expect(overlayActive).toHaveProperty("bollUpper");
    expect(overlayActive).toHaveProperty("donchianUpper");
  });
});
