import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const candlePropsSpy = vi.fn();

vi.mock("../primitives/KlineCandlePane", () => ({
  KlineCandlePane: (props: Record<string, unknown>) => {
    candlePropsSpy(props);
    return <div data-testid="kline-candle-pane" />;
  },
}));

vi.mock("../primitives/UplotDrawdownPane", () => ({
  UplotDrawdownPane: () => <div data-testid="uplot-drawdown-pane" />,
}));

vi.mock("../primitives/UplotEquityPane", () => ({
  UplotEquityPane: () => <div data-testid="uplot-equity-pane" />,
}));

vi.mock("../primitives/UplotHistogramPane", () => ({
  UplotHistogramPane: () => <div data-testid="uplot-histogram-pane" />,
}));

vi.mock("../primitives/UplotOscillatorPane", () => ({
  UplotOscillatorPane: () => <div data-testid="uplot-oscillator-pane" />,
}));

import { RunChartV2 } from "./RunChartV2";
import type { IndicatorMap, RunChartV2Payload } from "../types";

const time = [1_700_000_000, 1_700_003_600];

function makePayload(): RunChartV2Payload {
  const indicators: IndicatorMap = {
    sma20: { time, value: [100, 101] },
    sma30: { time, value: [99, 100] },
    ema200: { time, value: [95, 96] },
    rsi: { time, value: [45, 48] },
  };
  return {
    kind: "run",
    asset: "BTC",
    granularity: "1h",
    candles: {
      time,
      open: [100, 101],
      high: [105, 106],
      low: [95, 96],
      close: [101, 102],
      volume: [10, 20],
    },
    indicators,
    equity: [{ time: time[0], value: 10_000 }],
    drawdown: [{ time: time[0], value: 0 }],
    markers: [],
    positions: [],
  };
}

describe("RunChartV2", () => {
  beforeEach(() => {
    candlePropsSpy.mockClear();
    localStorage.clear();
  });

  it("threads the full v1 moving-average overlay set through to the candle pane", () => {
    localStorage.setItem(
      "xvision.chart2.layers.run",
      JSON.stringify({ sma30: true, ema200: true }),
    );

    render(<RunChartV2 payload={makePayload()} />);

    expect(candlePropsSpy).toHaveBeenCalled();
    const props = candlePropsSpy.mock.calls[0][0] as Record<string, unknown>;
    const overlays = props.overlays as Record<string, unknown>;
    expect(overlays.sma20).toEqual({ time, value: [100, 101] });
    expect(overlays.sma30).toEqual({ time, value: [99, 100] });
    expect(overlays.ema200).toEqual({ time, value: [95, 96] });
  });

  it("renders data-table values when fewer than 200 candles are present", () => {
    render(<RunChartV2 payload={makePayload()} />);

    fireEvent.click(screen.getByRole("button", { name: "Data table" }));

    expect(screen.getByText("100")).toBeInTheDocument();
    expect(screen.getAllByText("101").length).toBeGreaterThan(0);
    expect(screen.getByText("102")).toBeInTheDocument();
  });
});
