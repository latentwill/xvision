// Shared display helpers for the Live cockpit (Task B-III DRY cleanup).
//
// B-II duplicated these formatters + the `barsByAsset` adapter across
// `LiveAccountStrip` and `LivePositionsTable`. Hoisted here so both import
// one copy. Pure functions — covered by `live-format.test.ts`.

import type { ChartBar, RunChartPayload } from "@/api/types.gen";

export const DASH = "—";

/** Theme-token tone for a signed PnL value: gold up, danger down, plain flat. */
export function pnlTone(n: number | null): string {
  if (n == null || n === 0) return "text-text";
  return n > 0 ? "text-gold" : "text-danger";
}

/** Signed USD with a unicode minus for negatives (e.g. `+$1,200.00`, `−$3.40`). */
export function fmtUsdSigned(n: number | null): string {
  if (n == null) return DASH;
  if (n === 0) return "$0.00";
  const abs = Math.abs(n).toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  return n > 0 ? `+$${abs}` : `−$${abs}`;
}

/** Plain (unsigned) USD with thousands separators (e.g. `$12,000.00`). */
export function fmtUsdPlain(n: number | null): string {
  if (n == null) return DASH;
  return `$${n.toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

/** Signed percent with a unicode minus for negatives (e.g. `+2.50%`, `−1.10%`). */
export function fmtPctSigned(n: number | null): string {
  if (n == null) return DASH;
  const abs = Math.abs(n).toFixed(2);
  const sign = n > 0 ? "+" : n < 0 ? "−" : "";
  return `${sign}${abs}%`;
}

/**
 * Build the per-asset bar map the derivation helpers expect. The live stream
 * is single-asset per run today (`payload.asset` + `payload.bars`), but the
 * derivations take a map so multi-asset runs slot in later without a shape
 * change at the call sites.
 */
export function barsByAsset(payload: RunChartPayload): Map<string, ChartBar[]> {
  return new Map([[payload.asset, payload.bars]]);
}
