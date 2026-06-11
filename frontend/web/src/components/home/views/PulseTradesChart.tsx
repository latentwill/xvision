// frontend/web/src/components/home/views/PulseTradesChart.tsx
//
// "Price + trades" Pulse view: the run's market candles with buy/sell
// markers, reusing the chart-v2 KlineCandlePane BARE — no ChartFrame
// wrapper, so no chart-v2 range/zoom window events fire on the home page.

import type { ReactElement } from "react";
import type { RunChartPayload } from "@/api/types.gen";
import { runChartPayloadToV2 } from "@/components/chart/v2/adapters/run-chart-payload";
import { KlineCandlePane } from "@/components/chart/v2/primitives/KlineCandlePane";

export function PulseTradesChart({
  payload,
  height = 210,
}: {
  payload: RunChartPayload;
  height?: number;
}): ReactElement {
  const v2 = runChartPayloadToV2(payload);
  return (
    <div data-testid="pulse-trades-chart">
      <KlineCandlePane
        candles={v2.candles}
        markers={v2.markers.filter((m) => m.kind === "buy" || m.kind === "sell")}
        height={height}
      />
    </div>
  );
}
