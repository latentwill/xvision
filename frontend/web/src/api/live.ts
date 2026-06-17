// frontend/web/src/api/live.ts
//
// Live execution-venue surface: `GET /api/live/venue-account` — the real
// Orderly Network account (equity / USDC / unrealized PnL / open positions)
// that backs live-trading runs. Connection state is data, not an error:
// the endpoint returns `{ connected: false, reason }` when the daemon has
// no ORDERLY_* credentials, so the live page renders a "not configured"
// state instead of failing.
//
// DTO mirror of `xvision_engine::api::live_broker::VenueAccountDto`
// (hand-written: the engine DTO is not ts-rs exported).

import { apiFetch } from "./client";

export interface VenuePosition {
  /** Venue market string, e.g. `"PERP_BTC_USDC"`. */
  symbol: string;
  /** Signed base-asset quantity (positive = long, negative = short). */
  qty: number;
  entry_price: number;
  mark_price: number;
  unrealized_pnl: number;
}

export interface VenueAccount {
  connected: boolean;
  /** Always `"orderly"` in the current live scope. */
  venue: string;
  /** `"testnet"` or `"mainnet"`; absent when disconnected. */
  network?: string | null;
  account_id?: string | null;
  equity_usd?: number | null;
  usdc_holding?: number | null;
  unrealized_pnl?: number | null;
  positions: VenuePosition[];
  /** Populated when `connected === false`. */
  reason?: string | null;
}

export const liveKeys = {
  all: ["live"] as const,
  /** Keyed by venue so each broker's account is cached independently. */
  venueAccount: (venue?: string) =>
    [...liveKeys.all, "venue-account", venue ?? "default"] as const,
};

/**
 * Fetch a venue's account snapshot. `venue` selects the broker (`"orderly"`,
 * `"hyperliquid"`, …); omitted ⇒ the daemon's default (Orderly). The backend
 * returns `{ connected:false, reason }` for venues whose live ledger isn't
 * wired yet, so the panel renders an honest state rather than erroring.
 */
export function getVenueAccount(venue?: string): Promise<VenueAccount> {
  const qs = venue ? `?venue=${encodeURIComponent(venue)}` : "";
  return apiFetch<VenueAccount>(`/api/live/venue-account${qs}`);
}
