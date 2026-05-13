import type { LayerKey } from "./chart-layers";

type Props = {
  layers: Record<LayerKey, boolean>;
  toggle: (key: LayerKey) => void;
  set: (key: LayerKey, value: boolean) => void;
  markers?: boolean;
  equity?: boolean;
  volume?: boolean;
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

export function ChartLayersPanel({
  layers,
  toggle,
  set,
  markers = false,
  equity = false,
  volume = true,
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
          {key}
        </label>
      ))}

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
          {key}
        </label>
      ))}

      {equity ? (
        <>
          <div className="text-text-3 mb-1 mt-3">Equity pane</div>
          {EQUITY_KEYS.map((key) => (
            <label key={key} className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={layers[key]}
                onChange={() => toggle(key)}
              />{" "}
              {key}
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
            volume
          </label>
        </>
      ) : null}
    </div>
  );
}
