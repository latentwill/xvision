// src/features/marketplace/data/pricing.ts
/** A listing is free when its price is null (no price set) or explicitly 0. */
export const isFreeListing = (l: { priceUsdc: number | null }): boolean =>
  l.priceUsdc === null || l.priceUsdc === 0;
