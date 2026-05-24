// Alpaca crypto whitelist — the assets a strategy can trade.
export const ALPACA_ASSETS = [
  'BTC', 'ETH', 'LTC', 'SOL', 'AVAX', 'LINK', 'AAVE', 'UNI',
  'DOT', 'DOGE', 'SHIB', 'MATIC', 'BCH', 'USDT', 'USDC',
] as const;

/** Bare ticker → venue pair, e.g. "BTC" → "BTC/USD". */
export function toVenuePair(sym: string): string {
  return sym.includes('/') ? sym : `${sym}/USD`;
}
