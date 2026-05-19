import type { LayerKey } from "./chart-layers";

type Props = {
  layers: Record<LayerKey, boolean>;
  toggle: (key: LayerKey) => void;
  set: (key: LayerKey, value: boolean) => void;
  markers?: boolean;
  equity?: boolean;
  volume?: boolean;
  subpane?: boolean;
  radioName?: string;
};

const BASE_PRICE_KEYS = [
  "candles",
  "sma20",
  "sma30",
  "sma50",
  "sma60",
  "sma90",
  "sma200",
  "ema20",
  "ema30",
  "ema50",
  "ema60",
  "ema90",
  "ema200",
  "bollinger",
  "donchian",
] as const;

const MARKER_KEYS = [
  "markerBuy",
  "markerSell",
  "markerVeto",
  "markerHold",
  "positionBand",
] as const;

const SUBPANE_KEYS = [
  "subpaneRsi",
  "subpaneMacd",
  "subpaneAtr",
  "subpaneOff",
] as const;

const EQUITY_KEYS = ["equity", "drawdown"] as const;

const LAYER_LABELS: Record<LayerKey, string> = {
  candles: "Candles",
  sma20: "SMA 20",
  sma30: "SMA 30",
  sma50: "SMA 50",
  sma60: "SMA 60",
  sma90: "SMA 90",
  sma200: "SMA 200",
  ema20: "EMA 20",
  ema30: "EMA 30",
  ema50: "EMA 50",
  ema60: "EMA 60",
  ema90: "EMA 90",
  ema200: "EMA 200",
  bollinger: "Bollinger Bands",
  donchian: "Donchian Channel",
  markerBuy: "Buy markers",
  markerSell: "Sell markers",
  markerVeto: "Veto markers",
  markerHold: "Hold markers",
  positionBand: "Position band",
  subpaneRsi: "RSI 14",
  subpaneMacd: "MACD",
  subpaneAtr: "ATR 14",
  subpaneOff: "Off",
  equity: "Earnings",
  drawdown: "Drawdown",
  volume: "Volume",
};

export function ChartLayersPanel({
  layers,
  toggle,
  set,
  markers = false,
  equity = false,
  volume = true,
  subpane = true,
  radioName = "chart-subpane",
}: Props) {
  const priceKeys = markers
    ? [...BASE_PRICE_KEYS, ...MARKER_KEYS]
    : BASE_PRICE_KEYS;

  return (
    <div className="space-y-2">
      <div className="text-text-3 mb-1">Price pane</div>
      {priceKeys.map((key) => (
        <label key={key} className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={layers[key]}
            onChange={() => toggle(key)}
          />{" "}
          {LAYER_LABELS[key]}
        </label>
      ))}

      {subpane ? (
        <>
          <div className="text-text-3 mb-1 mt-3">Subpane</div>
          {SUBPANE_KEYS.map((key) => (
            <label key={key} className="flex items-center gap-2">
              <input
                type="radio"
                name={radioName}
                checked={layers[key]}
                onChange={() => {
                  SUBPANE_KEYS.forEach((candidate) =>
                    set(candidate, candidate === key),
                  );
                }}
              />{" "}
              {LAYER_LABELS[key]}
            </label>
          ))}
        </>
      ) : null}

      {equity ? (
        <>
          <div className="text-text-3 mb-1 mt-3">Earnings pane</div>
          {EQUITY_KEYS.map((key) => (
            <label key={key} className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={layers[key]}
                onChange={() => toggle(key)}
              />{" "}
              {LAYER_LABELS[key]}
            </label>
          ))}
        </>
      ) : null}

      {volume ? (
        <>
          <div className="text-text-3 mb-1 mt-3">Volume</div>
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={layers.volume}
              onChange={() => toggle("volume")}
            />{" "}
            {LAYER_LABELS.volume}
          </label>
        </>
      ) : null}
    </div>
  );
}
