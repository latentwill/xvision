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
  const ind = payload.indicators ?? {};
  const overlays = {
    sma20: ind.sma20, sma30: ind.sma30, sma50: ind.sma50,
    sma60: ind.sma60, sma90: ind.sma90, sma200: ind.sma200,
    ema20: ind.ema20, ema30: ind.ema30, ema50: ind.ema50,
    ema60: ind.ema60, ema90: ind.ema90, ema200: ind.ema200,
    bollUpper: ind.bollUpper, bollMiddle: ind.bollMiddle, bollLower: ind.bollLower,
    donchianUpper: ind.donchianUpper, donchianLower: ind.donchianLower,
  };
  const overlayActive = {
    sma20: layers.sma20, sma30: layers.sma30, sma50: layers.sma50,
    sma60: layers.sma60, sma90: layers.sma90, sma200: layers.sma200,
    ema20: layers.ema20, ema30: layers.ema30, ema50: layers.ema50,
    ema60: layers.ema60, ema90: layers.ema90, ema200: layers.ema200,
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
                  { key: "sma30", label: "SMA 30", on: layers.sma30 },
                  { key: "sma50", label: "SMA 50", on: layers.sma50 },
                  { key: "sma60", label: "SMA 60", on: layers.sma60 },
                  { key: "sma90", label: "SMA 90", on: layers.sma90 },
                  { key: "sma200", label: "SMA 200", on: layers.sma200 },
                  { key: "ema20", label: "EMA 20", on: layers.ema20 },
                  { key: "ema30", label: "EMA 30", on: layers.ema30 },
                  { key: "ema50", label: "EMA 50", on: layers.ema50 },
                  { key: "ema60", label: "EMA 60", on: layers.ema60 },
                  { key: "ema90", label: "EMA 90", on: layers.ema90 },
                  { key: "ema200", label: "EMA 200", on: layers.ema200 },
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
                  { key: "equity", label: "Return %", on: layers.equity },
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
