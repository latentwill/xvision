/**
 * Asset utility helpers.
 *
 * The legacy `ALPACA_ASSETS` hardcoded whitelist has been removed.
 * Use `useAlpacaAssets()` from `@/api/assets` to get the live list
 * served by `GET /api/assets`.
 */

/** Bare ticker → venue pair, e.g. "BTC" → "BTC/USD". */
export function toVenuePair(sym: string): string {
  return sym.includes("/") ? sym : `${sym}/USD`;
}
