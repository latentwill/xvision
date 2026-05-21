import { useState } from "react";
import {
  ChartFrame,
  KlineCandlePane,
  LayerPanel,
  Legend,
  MarkerDock,
  PaneStack,
  UplotEquityPane,
  UplotHistogramPane,
  type RangePreset,
} from "../primitives";
import { useChart2Layers } from "../hooks/useChart2Layers";
import { useChart2Sync } from "../hooks/useChart2Sync";
import { useChart2Theme } from "../hooks/useChart2Theme";
import type { ScenarioChartV2Payload } from "../types";

type Props = { payload: ScenarioChartV2Payload };

export function ScenarioChartV2({ payload }: Props) {
  const [range, setRange] = useState<RangePreset>("All");
  const { layers, toggle } = useChart2Layers("scenario");
  const syncKey = useChart2Sync("scenario");
  const theme = useChart2Theme();

  const markers = payload.markers.filter((m) =>
    m.kind === "buy" ? layers.markerBuy :
    m.kind === "sell" ? layers.markerSell :
    m.kind === "veto" ? layers.markerVeto :
    layers.markerHold,
  );

  return (
    <div className="grid grid-cols-[1fr_240px] gap-3">
      <ChartFrame
        title={`Scenario · ${payload.asset} · ${payload.granularity}`}
        range={range}
        onRange={setRange}
        layersPanel={
          <LayerPanel
            groups={[
              {
                title: "Markers",
                items: [
                  { key: "markerBuy", label: "Buy", on: layers.markerBuy },
                  { key: "markerSell", label: "Sell", on: layers.markerSell },
                  { key: "markerVeto", label: "Veto", on: layers.markerVeto },
                ],
              },
              {
                title: "Panes",
                items: [
                  { key: "equity", label: "Equity", on: layers.equity },
                  { key: "volume", label: "Volume", on: layers.volume },
                ],
              },
            ]}
            onToggle={(k) => toggle(k as Parameters<typeof toggle>[0])}
          />
        }
      >
        <PaneStack syncKey={syncKey}>
          <KlineCandlePane
            candles={payload.candles}
            markers={markers}
            positions={layers.positionBand ? payload.positions : undefined}
          />
          {layers.equity ? <UplotEquityPane points={payload.equity} height={100} /> : null}
          {layers.volume ? <UplotHistogramPane candles={payload.candles} height={70} /> : null}
        </PaneStack>
        <div className="px-3 py-2 border-t border-border">
          <Legend
            items={[
              { label: "Long", color: theme.position.longLine },
              { label: "Short", color: theme.position.shortLine },
            ]}
          />
        </div>
      </ChartFrame>
      <MarkerDock markers={markers} />
    </div>
  );
}
