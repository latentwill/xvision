import { useState } from "react";
import {
  ChartFrame,
  KlineCandlePane,
  Legend,
  PaneStack,
  UplotEquityPane,
  type RangePreset,
} from "../primitives";
import { useChart2Sync } from "../hooks/useChart2Sync";
import { useChart2Theme } from "../hooks/useChart2Theme";
import type { WizardPreviewV2Payload } from "../types";

type Props = { payload: WizardPreviewV2Payload };

export function WizardPreviewChartV2({ payload }: Props) {
  const [range, setRange] = useState<RangePreset>("All");
  const syncKey = useChart2Sync("wizard");
  const theme = useChart2Theme();

  return (
    <ChartFrame
      title={`Preview · ${payload.asset}`}
      range={range}
      onRange={setRange}
    >
      <PaneStack syncKey={syncKey}>
        <KlineCandlePane candles={payload.candles} height={220} />
        <UplotEquityPane points={payload.equity} height={80} />
      </PaneStack>
      <div className="px-3 py-2 border-t border-border">
        <Legend
          items={[
            { label: "Price", color: theme.candle.up },
            { label: "Projected equity", color: theme.panes.equity },
          ]}
        />
      </div>
    </ChartFrame>
  );
}
