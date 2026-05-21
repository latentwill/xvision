import { useState } from "react";
import {
  ChartFrame,
  KlineCandlePane,
  LayerPanel,
  Legend,
  PaneStack,
  UplotCompareOverlayPane,
  UplotDrawdownPane,
  type RangePreset,
} from "../primitives";
import { useChart2Layers } from "../hooks/useChart2Layers";
import { useChart2Sync } from "../hooks/useChart2Sync";
import { useChart2Theme } from "../hooks/useChart2Theme";
import type { StrategyChartV2Payload } from "../types";

type Props = { payload: StrategyChartV2Payload };

export function StrategyChartV2({ payload }: Props) {
  const [range, setRange] = useState<RangePreset>("All");
  const { layers, toggle } = useChart2Layers("strategy");
  const syncKey = useChart2Sync("strategy");
  const theme = useChart2Theme();

  const arms = [
    { id: "live", label: "Live", equity: payload.liveEquity },
    { id: "paper", label: "Paper", equity: payload.paperEquity },
  ];

  return (
    <ChartFrame
      title={`Strategy · ${payload.asset} · ${payload.granularity}`}
      range={range}
      onRange={setRange}
      layersPanel={
        <LayerPanel
          groups={[
            {
              title: "Panes",
              items: [
                { key: "candles", label: "Candles", on: layers.candles },
                { key: "compareOverlay", label: "Live vs paper", on: layers.compareOverlay },
                { key: "drawdown", label: "Drawdown", on: layers.drawdown },
              ],
            },
          ]}
          onToggle={(k) => toggle(k as Parameters<typeof toggle>[0])}
        />
      }
    >
      <PaneStack syncKey={syncKey}>
        {layers.candles ? <KlineCandlePane candles={payload.candles} height={280} /> : null}
        {layers.compareOverlay ? (
          <UplotCompareOverlayPane arms={arms} height={140} />
        ) : null}
        {layers.drawdown ? (
          <UplotDrawdownPane points={payload.drawdown} height={100} />
        ) : null}
      </PaneStack>
      <div className="px-3 py-2 border-t border-border">
        <Legend
          items={[
            { label: "Live", color: theme.compare.palette[0] },
            { label: "Paper", color: theme.compare.palette[1] },
          ]}
        />
      </div>
    </ChartFrame>
  );
}
