import { describe, expect, it, vi } from "vitest";
import { applyVerticalAutoScale, fitChartContent } from "./chart-fit";

function makeMockChart() {
  const applyOptionsCalls: Array<{ id: string; opts: unknown }> = [];
  const fitContent = vi.fn();
  const chart = {
    timeScale: vi.fn(() => ({ fitContent })),
    priceScale: vi.fn((id: string) => ({
      applyOptions: (opts: unknown) => {
        applyOptionsCalls.push({ id, opts });
      },
    })),
  };
  return { chart, fitContent, applyOptionsCalls };
}

describe("applyVerticalAutoScale", () => {
  it("calls applyOptions({ autoScale: true }) on the requested price scales", () => {
    const { chart, applyOptionsCalls } = makeMockChart();
    applyVerticalAutoScale(chart as never, ["right"]);
    expect(applyOptionsCalls).toEqual([
      { id: "right", opts: { autoScale: true } },
    ]);
  });

  it("supports multiple price scales (e.g. volume + price panes)", () => {
    const { chart, applyOptionsCalls } = makeMockChart();
    applyVerticalAutoScale(chart as never, ["right", "volume"]);
    expect(applyOptionsCalls.map((c) => c.id)).toEqual(["right", "volume"]);
  });

  it("defaults to the right scale when no ids are passed", () => {
    const { chart, applyOptionsCalls } = makeMockChart();
    applyVerticalAutoScale(chart as never);
    expect(applyOptionsCalls).toEqual([
      { id: "right", opts: { autoScale: true } },
    ]);
  });

  it("is a no-op when chart is null or undefined", () => {
    expect(() => applyVerticalAutoScale(null)).not.toThrow();
    expect(() => applyVerticalAutoScale(undefined)).not.toThrow();
  });

  it("swallows priceScale lookup errors so unknown ids don't break callers", () => {
    const chart = {
      priceScale: vi.fn(() => {
        throw new Error("unknown scale id");
      }),
      timeScale: vi.fn(),
    };
    expect(() =>
      applyVerticalAutoScale(chart as never, ["unknown"]),
    ).not.toThrow();
  });
});

describe("fitChartContent", () => {
  it("fits the time axis and forces a price-axis re-fit on the same chart", () => {
    const { chart, fitContent, applyOptionsCalls } = makeMockChart();
    fitChartContent(chart as never, ["right"]);
    expect(fitContent).toHaveBeenCalledTimes(1);
    expect(applyOptionsCalls).toEqual([
      { id: "right", opts: { autoScale: true } },
    ]);
  });

  it("is a no-op when chart is null or undefined", () => {
    expect(() => fitChartContent(null)).not.toThrow();
    expect(() => fitChartContent(undefined)).not.toThrow();
  });
});
