// chart-layers.ts
export type LayerKey =
  | 'candles' | 'sma20' | 'sma30' | 'sma50' | 'sma60' | 'sma90' | 'sma200'
  | 'ema20' | 'ema30' | 'ema50' | 'ema60' | 'ema90' | 'ema200'
  | 'bollinger' | 'donchian'
  | 'markerBuy' | 'markerSell' | 'markerVeto' | 'markerHold'
  | 'positionBand'
  | 'subpaneRsi' | 'subpaneMacd' | 'subpaneAtr' | 'subpaneOff'
  | 'equity' | 'drawdown'
  | 'volume';

export const DEFAULT_LAYERS: Record<LayerKey, boolean> = {
  candles: true, sma20: true, sma30: false, sma50: true, sma60: false, sma90: false, sma200: true,
  ema20: false, ema30: false, ema50: false, ema60: false, ema90: false, ema200: false,
  bollinger: false, donchian: false,
  markerBuy: true, markerSell: true, markerVeto: true, markerHold: false,
  positionBand: true,
  subpaneRsi: true, subpaneMacd: false, subpaneAtr: false, subpaneOff: false,
  equity: true, drawdown: true,
  volume: false,
};

export function storageKey(surface: string): string {
  return `xvision.chart.layers.${surface}`;
}
