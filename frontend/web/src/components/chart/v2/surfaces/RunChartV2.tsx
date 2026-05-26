import { useState } from "react";
import {
  CacheStatusBadge,
  ChartFrame,
  DataTable,
  KlineCandlePane,
  LayerPanel,
  Legend,
  MarkerDock,
  PaneStack,
  UplotDrawdownPane,
  UplotEquityPane,
  UplotHistogramPane,
  UplotOscillatorPane,
  type RangePreset,
} from "../primitives";
import { useChart2Layers } from "../hooks/useChart2Layers";
import { useChart2Sync } from "../hooks/useChart2Sync";
import { useChart2Theme } from "../hooks/useChart2Theme";
import type { RunChartV2Payload } from "../types";

type Props = {
  payload: RunChartV2Payload;
  showMarkerDock?: boolean;
};

export function RunChartV2({ payload, showMarkerDock = true }: Props) {
  const [range, setRange] = useState<RangePreset>("All");
  const { layers, toggle } = useChart2Layers("run");
  const syncKey = useChart2Sync("run");
  const theme = useChart2Theme();

  const overlays = {
    sma20: layers.sma20 ? payload.indicators.sma20 : undefined,
    sma30: layers.sma30 ? payload.indicators.sma30 : undefined,
    sma50: layers.sma50 ? payload.indicators.sma50 : undefined,
    sma60: layers.sma60 ? payload.indicators.sma60 : undefined,
    sma90: layers.sma90 ? payload.indicators.sma90 : undefined,
    sma200: layers.sma200 ? payload.indicators.sma200 : undefined,
    ema20: layers.ema20 ? payload.indicators.ema20 : undefined,
    ema30: layers.ema30 ? payload.indicators.ema30 : undefined,
    ema50: layers.ema50 ? payload.indicators.ema50 : undefined,
    ema60: layers.ema60 ? payload.indicators.ema60 : undefined,
    ema90: layers.ema90 ? payload.indicators.ema90 : undefined,
    ema200: layers.ema200 ? payload.indicators.ema200 : undefined,
    bollUpper: layers.bollinger ? payload.indicators.bollUpper : undefined,
    bollMiddle: layers.bollinger ? payload.indicators.bollMiddle : undefined,
    bollLower: layers.bollinger ? payload.indicators.bollLower : undefined,
    donchianUpper: layers.donchian ? payload.indicators.donchianUpper : undefined,
    donchianLower: layers.donchian ? payload.indicators.donchianLower : undefined,
  };

  const markers = payload.markers.filter((m) =>
    m.kind === "buy" ? layers.markerBuy :
    m.kind === "sell" ? layers.markerSell :
    m.kind === "veto" ? layers.markerVeto :
    layers.markerHold,
  );

  const legendItems = [
    layers.sma20 && { label: "SMA 20", color: theme.overlay.sma20 },
    layers.sma30 && { label: "SMA 30", color: theme.overlay.sma30 },
    layers.sma50 && { label: "SMA 50", color: theme.overlay.sma50 },
    layers.sma60 && { label: "SMA 60", color: theme.overlay.sma60 },
    layers.sma90 && { label: "SMA 90", color: theme.overlay.sma90 },
    layers.sma200 && { label: "SMA 200", color: theme.overlay.sma200 },
    layers.ema20 && { label: "EMA 20", color: theme.overlay.ema20, dashed: true },
    layers.ema30 && { label: "EMA 30", color: theme.overlay.ema30, dashed: true },
    layers.ema50 && { label: "EMA 50", color: theme.overlay.ema50, dashed: true },
    layers.ema60 && { label: "EMA 60", color: theme.overlay.ema60, dashed: true },
    layers.ema90 && { label: "EMA 90", color: theme.overlay.ema90, dashed: true },
    layers.ema200 && { label: "EMA 200", color: theme.overlay.ema200, dashed: true },
  ].filter(Boolean) as { label: string; color: string; dashed?: boolean }[];

  const tableStart = Math.max(0, payload.candles.time.length - 200);
  const dataTableRows = payload.candles.time.slice(tableStart).map((t, i) => {
    const idx = tableStart + i;
    return {
      time: new Date(t * 1000).toISOString().slice(0, 19).replace("T", " "),
      open: payload.candles.open[idx],
      high: payload.candles.high[idx],
      low: payload.candles.low[idx],
      close: payload.candles.close[idx],
      volume: payload.candles.volume[idx],
    };
  });

  return (
    <div className="grid grid-cols-[1fr_240px] gap-3">
      <ChartFrame
        title={`Run · ${payload.asset} · ${payload.granularity}`}
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
                  { key: "bollinger", label: "Bollinger", on: layers.bollinger },
                  { key: "donchian", label: "Donchian", on: layers.donchian },
                ],
              },
              {
                title: "Markers",
                items: [
                  { key: "markerBuy", label: "Buy", on: layers.markerBuy },
                  { key: "markerSell", label: "Sell", on: layers.markerSell },
                  { key: "markerVeto", label: "Veto", on: layers.markerVeto },
                  { key: "markerHold", label: "Hold", on: layers.markerHold },
                ],
              },
              {
                title: "Panes",
                items: [
                  { key: "rsi", label: "RSI", on: layers.rsi },
                  { key: "macd", label: "MACD", on: layers.macd },
                  { key: "atr", label: "ATR", on: layers.atr },
                  { key: "equity", label: "Equity", on: layers.equity },
                  { key: "drawdown", label: "Drawdown", on: layers.drawdown },
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
              { key: "open", header: "O", align: "right" },
              { key: "high", header: "H", align: "right" },
              { key: "low", header: "L", align: "right" },
              { key: "close", header: "C", align: "right" },
              { key: "volume", header: "Vol", align: "right" },
            ]}
            rows={dataTableRows}
          />
        }
      >
        <PaneStack syncKey={syncKey}>
          {layers.candles ? (
            <KlineCandlePane
              candles={payload.candles}
              overlays={overlays}
              markers={markers}
              positions={layers.positionBand ? payload.positions : undefined}
            />
          ) : null}
          {layers.rsi && payload.indicators.rsi ? (
            <UplotOscillatorPane
              kind="rsi"
              series={{ primary: payload.indicators.rsi }}
              guides={[30, 70]}
              height={100}
            />
          ) : null}
          {layers.macd && payload.indicators.macdLine ? (
            <UplotOscillatorPane
              kind="macd"
              series={{
                primary: payload.indicators.macdLine,
                signal: payload.indicators.macdSignal,
                histogram: payload.indicators.macdHist,
              }}
              height={100}
            />
          ) : null}
          {layers.atr && payload.indicators.atr ? (
            <UplotOscillatorPane
              kind="atr"
              series={{ primary: payload.indicators.atr }}
              height={80}
            />
          ) : null}
          {layers.equity ? <UplotEquityPane points={payload.equity} height={110} /> : null}
          {layers.drawdown ? <UplotDrawdownPane points={payload.drawdown} height={80} /> : null}
          {layers.volume ? <UplotHistogramPane candles={payload.candles} height={70} /> : null}
        </PaneStack>
        <div className="px-3 py-2 border-t border-border flex items-center gap-3">
          <Legend items={legendItems} />
          <div className="ml-auto"><CacheStatusBadge state="fresh" /></div>
        </div>
      </ChartFrame>
      {showMarkerDock ? <MarkerDock markers={markers} /> : null}
    </div>
  );
}
