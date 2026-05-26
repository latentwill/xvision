import { useState } from "react";
import {
  ChartFrame,
  DataTable,
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

  // ── Candle-pane indicator overlays ─────────────────────────────────────────
  // Build the toggleable overlay subset from the payload's IndicatorMap, and a
  // parallel on/off map driven by the layer system. `payload.indicators` is
  // read defensively (`?? {}`) because fixtures/old payloads predating Task 5
  // may omit the field entirely.
  //
  // Parity note: v2 surfaces the SMA20/50/200 + EMA20/50 + Bollinger + Donchian
  // toggle subset only. The extra v1 lines (sma30/60/90, ema30/60/90/200) are a
  // deliberate, documented gap — the v2 layer system (useChart2Layers) has
  // fewer keys than v1's overlay menu. The underlying series may still exist in
  // payload.indicators, but they are not surfaced as toggles this wave.
  const ind = payload.indicators ?? {};
  const overlays = {
    sma20: ind.sma20, sma50: ind.sma50, sma200: ind.sma200,
    ema20: ind.ema20, ema50: ind.ema50,
    bollUpper: ind.bollUpper, bollMiddle: ind.bollMiddle, bollLower: ind.bollLower,
    donchianUpper: ind.donchianUpper, donchianLower: ind.donchianLower,
  };
  const overlayActive = {
    sma20: layers.sma20, sma50: layers.sma50, sma200: layers.sma200,
    ema20: layers.ema20, ema50: layers.ema50,
    bollUpper: layers.bollinger, bollMiddle: layers.bollinger, bollLower: layers.bollinger,
    donchianUpper: layers.donchian, donchianLower: layers.donchian,
  };

  // ── Bars data table ─────────────────────────────────────────────────────────
  // First 200 candles, threaded through ChartFrame's inline dataTable slot
  // (no popup — the slot expands in-flow below the canvas).
  const tableRows = payload.candles.time.slice(0, 200).map((t, i) => ({
    time: t,
    open: payload.candles.open[i],
    high: payload.candles.high[i],
    low: payload.candles.low[i],
    close: payload.candles.close[i],
    volume: payload.candles.volume[i],
  }));

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
                title: "Overlays",
                items: [
                  { key: "sma20", label: "SMA 20", on: layers.sma20 },
                  { key: "sma50", label: "SMA 50", on: layers.sma50 },
                  { key: "sma200", label: "SMA 200", on: layers.sma200 },
                  { key: "ema20", label: "EMA 20", on: layers.ema20 },
                  { key: "ema50", label: "EMA 50", on: layers.ema50 },
                  { key: "bollinger", label: "Bollinger Bands", on: layers.bollinger },
                  { key: "donchian", label: "Donchian Channel", on: layers.donchian },
                ],
              },
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
        dataTable={
          <DataTable
            columns={[
              { key: "time", header: "Time" },
              { key: "open", header: "Open", align: "right" },
              { key: "high", header: "High", align: "right" },
              { key: "low", header: "Low", align: "right" },
              { key: "close", header: "Close", align: "right" },
              { key: "volume", header: "Volume", align: "right" },
            ]}
            rows={tableRows}
          />
        }
      >
        <PaneStack syncKey={syncKey}>
          <KlineCandlePane
            candles={payload.candles}
            overlays={overlays}
            overlayActive={overlayActive}
            markers={markers}
            positions={layers.positionBand ? payload.positions : undefined}
          />
          {layers.equity && payload.equity.length > 0 ? (
            <UplotEquityPane points={payload.equity} height={100} />
          ) : null}
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
