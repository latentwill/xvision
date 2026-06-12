import { render } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import type { StrategyChartPayload } from "@/api/types.gen/StrategyChartPayload";
import { StrategyHistoryChartV2 } from "./StrategyHistoryChartV2";

const paneCalls = vi.hoisted(() => ({
  props: [] as Array<Record<string, unknown>>,
}));

vi.mock("../primitives/ChartFrame", () => ({
  ChartFrame: ({ children }: { children: ReactNode }) => (
    <div data-testid="chart-frame">{children}</div>
  ),
}));

vi.mock("../primitives/MultiStrategyEquityPane", () => ({
  MultiStrategyEquityPane: (props: Record<string, unknown>) => {
    paneCalls.props.push(props);
    return <div data-testid="multi-strategy-pane" />;
  },
}));

describe("StrategyHistoryChartV2", () => {
  it("wires compact x-axis labels into the performance history pane", () => {
    const payload: StrategyChartPayload = {
      strategy_id: "strategy-1",
      scenarios: [["baseline", "Baseline"]],
      run_series: [
        {
          run_id: "run-1",
          label: "Run 1",
          scenario_id: "baseline",
          final_pnl_usd: 120,
          max_drawdown_pct: -4.2,
          sharpe: 1.1,
          equity_normalised: [
            { time: Date.UTC(2025, 0, 1) / 1000, equity_usd: 0 },
            { time: Date.UTC(2025, 0, 12) / 1000, equity_usd: 12.3 },
          ],
        },
      ],
    };

    render(<StrategyHistoryChartV2 payload={payload} />);

    expect(paneCalls.props[0]).toMatchObject({
      height: 360,
      syncKey: "strategy-history-strategy-1",
      compactXAxisLabels: true,
    });
  });
});
