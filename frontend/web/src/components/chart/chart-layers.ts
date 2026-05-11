// chart-layers.ts
export type LayerKey =
  | 'candles' | 'sma20' | 'sma50' | 'sma200'
  | 'ema20' | 'ema50' | 'ema200'
  | 'bollinger' | 'donchian'
  | 'markerBuy' | 'markerSell' | 'markerVeto' | 'markerHold'
  | 'positionBand'
  | 'subpaneRsi' | 'subpaneMacd' | 'subpaneAtr' | 'subpaneOff'
  | 'equity' | 'drawdown'
  | 'volume';

export const DEFAULT_LAYERS: Record<LayerKey, boolean> = {
  candles: true, sma20: true, sma50: true, sma200: true,
  ema20: false, ema50: false, ema200: false,
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
