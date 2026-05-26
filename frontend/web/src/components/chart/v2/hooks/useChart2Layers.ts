import { useEffect, useState } from "react";

export type ChartV2LayerKey =
  | "candles"
  | "sma20"
  | "sma30"
  | "sma50"
  | "sma60"
  | "sma90"
  | "sma200"
  | "ema20"
  | "ema30"
  | "ema50"
  | "ema60"
  | "ema90"
  | "ema200"
  | "bollinger"
  | "donchian"
  | "markerBuy"
  | "markerSell"
  | "markerVeto"
  | "markerHold"
  | "positionBand"
  | "volume"
  | "rsi"
  | "macd"
  | "atr"
  | "equity"
  | "drawdown"
  | "compareOverlay";

export const DEFAULT_V2_LAYERS: Record<ChartV2LayerKey, boolean> = {
  candles: true,
  sma20: true,
  sma30: false,
  sma50: true,
  sma60: false,
  sma90: false,
  sma200: true,
  ema20: false,
  ema30: false,
  ema50: false,
  ema60: false,
  ema90: false,
  ema200: false,
  bollinger: false,
  donchian: false,
  markerBuy: true,
  markerSell: true,
  markerVeto: true,
  markerHold: false,
  positionBand: true,
  volume: false,
  rsi: true,
  macd: false,
  atr: false,
  equity: true,
  drawdown: true,
  compareOverlay: false,
};

function storageKey(surface: string): string {
  return `xvision.chart2.layers.${surface}`;
}

function readFromStorage(key: string): Record<ChartV2LayerKey, boolean> {
  if (typeof window === "undefined") return DEFAULT_V2_LAYERS;
  try {
    const raw = localStorage.getItem(key);
    if (raw) return { ...DEFAULT_V2_LAYERS, ...JSON.parse(raw) };
  } catch {
    // corrupted storage — fall through to defaults
  }
  return DEFAULT_V2_LAYERS;
}

export function useChart2Layers(surface: string) {
  const key = storageKey(surface);

  const [layers, setLayers] = useState<Record<ChartV2LayerKey, boolean>>(
    () => readFromStorage(key),
  );

  useEffect(() => {
    try {
      localStorage.setItem(key, JSON.stringify(layers));
    } catch {
      // storage unavailable — silently ignore
    }
  }, [layers, key]);

  function toggle(k: ChartV2LayerKey) {
    setLayers((prev) => ({ ...prev, [k]: !prev[k] }));
  }

  function set(k: ChartV2LayerKey, v: boolean) {
    setLayers((prev) => ({ ...prev, [k]: v }));
  }

  function reset() {
    setLayers(DEFAULT_V2_LAYERS);
  }

  return { layers, toggle, set, reset };
}
