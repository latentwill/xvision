import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { MultiStrategyEquityPane } from "./MultiStrategyEquityPane";

const plotCalls = vi.hoisted(() => ({
  usePlot: vi.fn(),
}));

vi.mock("./usePlot", () => ({
  usePlot: plotCalls.usePlot,
}));

vi.mock("../hooks/useChart2Theme", () => ({
  useChart2Theme: () => ({
    panes: {
      equity: "#00c16a",
    },
  }),
}));

vi.mock("../adapters/theme-to-uplot", () => ({
  themeToUplotOptions: () => ({
    axes: [
      {
        stroke: "#8a8f98",
        grid: { stroke: "#222" },
        ticks: { stroke: "#333" },
        font: "11px sans-serif",
      },
      {
        stroke: "#8a8f98",
      },
    ],
    cursor: {
      points: { size: 6 },
    },
  }),
}));

describe("MultiStrategyEquityPane", () => {
  it("reserves readable x-axis label space for performance history charts", () => {
    const jan1 = Date.UTC(2025, 0, 1) / 1000;
    const jan12 = Date.UTC(2025, 0, 12) / 1000;

    render(
      <MultiStrategyEquityPane
        time={[jan1, jan12]}
        compactXAxisLabels
        series={[
          {
            id: "run-1",
            label: "Run 1",
            values: [0, 12.3],
            color: "#00c16a",
          },
        ]}
      />,
    );

    const opts = plotCalls.usePlot.mock.calls[0][0];
    const xAxis = opts.axes[0];
    expect(xAxis.size).toBeGreaterThanOrEqual(44);
    expect(xAxis.gap).toBeGreaterThanOrEqual(8);
    expect(xAxis.values(null, [jan1, jan12])).toEqual(["1/1", "1/12"]);
  });
});
