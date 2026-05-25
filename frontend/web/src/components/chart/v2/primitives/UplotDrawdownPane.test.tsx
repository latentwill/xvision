import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { UplotDrawdownPane } from "./UplotDrawdownPane";

const plotCalls = vi.hoisted(() => ({
  usePlot: vi.fn(),
}));

vi.mock("./usePlot", () => ({
  usePlot: plotCalls.usePlot,
}));

vi.mock("./PaneStack", () => ({
  useSyncKey: () => null,
}));

vi.mock("../hooks/useChart2Theme", () => ({
  useChart2Theme: () => ({
    panes: {
      drawdown: "#f00",
      drawdownFillTop: "rgba(255,0,0,0.2)",
    },
  }),
}));

vi.mock("../adapters/theme-to-uplot", () => ({
  themeToUplotOptions: () => ({}),
}));

describe("UplotDrawdownPane", () => {
  it("uses a non-degenerate y range for all-zero drawdown data", () => {
    render(
      <UplotDrawdownPane
        points={[
          { time: 1, value: 0 },
          { time: 2, value: 0 },
        ]}
      />,
    );

    const opts = plotCalls.usePlot.mock.calls[0][0];
    expect(opts.scales.y.range).toEqual([-0.01, 0]);
  });
});
