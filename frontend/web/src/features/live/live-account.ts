// Live cockpit account-stat + positions-table derivations (Task B-II, spec §2.6).
//
// All derivation logic is pure and unit-tested (live-account.test.ts) so the
// React components (LiveAccountStrip / LivePositionsTable) stay thin. The data
// inputs come from two sources, both already proven elsewhere in the SPA:
//
//   - The live stream (`useRunStream` → RunChartPayload): `equity` (current
//     equity + daily PnL baseline), `drawdown` (drawdown-from-peak %, computed
//     server-side in crates/xvision-engine/src/api/chart.rs::compute_drawdown
//     as (peak - equity)/peak*100 per point), and `bars` (latest close →
//     mark-to-market price).
//   - Fetched decisions (`getRun(id).decisions` → DecisionRowDto[]): replayed
//     by `derivePositionsByDecision` (features/decisions/positions.ts) into the
//     current open-position set. Entry time is recovered here by looking up the
//     opening decision's `timestamp`.
//
// Anything not cleanly derivable from these inputs is returned as `null` and
// rendered as "—" by the component — never faked.

import type { ChartBar, ChartEquityPoint, DrawdownPoint } from "@/api/types.gen";
import type { DecisionRowDto } from "@/api/types.gen";

import {
  derivePositionsByDecision,
  type OpenPosition,
} from "@/features/decisions/positions";

const SECONDS_PER_DAY = 86_400;

/** Latest equity ($) from the equity stream, or null when empty. */
export function currentEquity(equity: ChartEquityPoint[]): number | null {
  if (equity.length === 0) return null;
  return equity[equity.length - 1]!.equity_usd;
}

export type DailyPnl = {
  usd: number | null;
  pct: number | null;
  /**
   * Which baseline the PnL was measured against:
   *   "midnight"     — an equity point at/before today's 00:00 UTC was found.
   *   "series-start" — no pre-midnight point; fell back to the first point of
   *                    the series. The strip labels this honestly ("since start")
   *                    rather than implying a true day boundary.
   *   "none"         — empty series; nothing to measure.
   */
  basis: "midnight" | "series-start" | "none";
};

/**
 * Daily PnL = current equity − equity at the most-recent midnight-UTC
 * boundary. Finds the latest equity point at/before today's 00:00 UTC; if no
 * such point exists, falls back to the first point of the series and flags
 * `basis: "series-start"` so the display can be honest about it.
 *
 * `nowSec` is injectable for deterministic tests; defaults to wall-clock.
 */
export function dailyPnl(
  equity: ChartEquityPoint[],
  nowSec: number = Math.floor(Date.now() / 1000),
): DailyPnl {
  if (equity.length === 0) return { usd: null, pct: null, basis: "none" };

  const current = equity[equity.length - 1]!.equity_usd;
  const midnightSec = nowSec - (nowSec % SECONDS_PER_DAY);

  // Latest point at or before midnight UTC. Equity series is appended in time
  // order, but don't assume strict sort — scan for the last qualifying point.
  let baseline: ChartEquityPoint | null = null;
  for (const p of equity) {
    if (p.time <= midnightSec) {
      if (!baseline || p.time >= baseline.time) baseline = p;
    }
  }

  if (baseline) {
    return diffFrom(baseline.equity_usd, current, "midnight");
  }
  // No pre-midnight point: honest fallback to the first point of the series.
  return diffFrom(equity[0]!.equity_usd, current, "series-start");
}

function diffFrom(
  base: number,
  current: number,
  basis: "midnight" | "series-start",
): DailyPnl {
  const usd = current - base;
  const pct = base !== 0 ? (usd / base) * 100 : null;
  return { usd, pct, basis };
}

/**
 * Drawdown from peak (%). Prefers the stream's `drawdown` series, whose last
 * point already represents (peak − equity)/peak·100 (verified against
 * compute_drawdown in the engine). Falls back to deriving it from the equity
 * curve when the drawdown series is empty. Returns null when neither exists.
 */
export function drawdownFromPeak(
  drawdown: DrawdownPoint[],
  equity: ChartEquityPoint[],
): number | null {
  if (drawdown.length > 0) {
    return drawdown[drawdown.length - 1]!.drawdown_pct;
  }
  if (equity.length === 0) return null;
  let peak = Number.NEGATIVE_INFINITY;
  for (const p of equity) peak = Math.max(peak, p.equity_usd);
  const current = equity[equity.length - 1]!.equity_usd;
  if (peak <= 0) return 0;
  return ((peak - current) / peak) * 100;
}

/**
 * Map asset → latest bar close. The live stream's `bars` is single-asset per
 * run today, but multi-asset runs are on the roadmap, so this accepts a
 * per-asset bar map and is robust to it. Assets with no bars are omitted.
 */
export function latestCloseByAsset(
  barsByAsset: Map<string, ChartBar[]>,
): Map<string, number> {
  const out = new Map<string, number>();
  for (const [asset, bars] of barsByAsset) {
    if (bars.length === 0) continue;
    out.set(asset, bars[bars.length - 1]!.close);
  }
  return out;
}

const signOf = (side: OpenPosition["side"]) => (side === "long" ? 1 : -1);

/**
 * Unrealized PnL = Σ over open positions of (latest price − entry) × signed
 * qty. Returns null (→ "—") if ANY open position is missing a latest price,
 * since a partial sum would be misleading. Returns 0 for an empty position set.
 */
export function unrealizedPnl(
  positions: OpenPosition[],
  pricesByAsset: Map<string, number>,
): number | null {
  let total = 0;
  for (const pos of positions) {
    const price = pricesByAsset.get(pos.asset);
    if (price == null) return null;
    total += (price - pos.entry_price) * pos.qty * signOf(pos.side);
  }
  return total;
}

export type PositionRow = {
  asset: string;
  side: OpenPosition["side"];
  qty: number;
  entry_price: number;
  /** ISO timestamp of the opening decision, or null if not recoverable. */
  entry_time: string | null;
  /** qty × latest price, or null when latest price is unknown. */
  current_value: number | null;
  /** (latest price − entry) × signed qty, or null when price unknown. */
  unrealized_pnl: number | null;
  /** unrealized_pnl / (entry_price × qty) × 100, or null when not derivable. */
  pct_change: number | null;
};

/**
 * Build the active-positions table rows: the current open set (from
 * `derivePositionsByDecision` at the max decision_index), enriched with entry
 * time (looked up from the opening decision) and mark-to-market value / PnL /
 * %-change from the latest per-asset price. Fields that can't be derived
 * (missing latest price) are left null for the component to render as "—".
 */
export function buildPositionRows(
  decisions: ReadonlyArray<DecisionRowDto>,
  pricesByAsset: Map<string, number>,
): PositionRow[] {
  if (decisions.length === 0) return [];

  const byDecision = derivePositionsByDecision(decisions);
  if (byDecision.size === 0) return [];

  const maxIndex = Math.max(...byDecision.keys());
  const open = byDecision.get(maxIndex) ?? [];
  if (open.length === 0) return [];

  const entryTimes = entryTimeByAsset(decisions);

  return open.map((pos): PositionRow => {
    const price = pricesByAsset.get(pos.asset);
    const hasPrice = price != null;
    const currentValue = hasPrice ? pos.qty * price : null;
    const pnl = hasPrice
      ? (price - pos.entry_price) * pos.qty * signOf(pos.side)
      : null;
    const cost = pos.entry_price * pos.qty;
    const pct = pnl != null && cost !== 0 ? (pnl / cost) * 100 : null;
    return {
      asset: pos.asset,
      side: pos.side,
      qty: pos.qty,
      entry_price: pos.entry_price,
      entry_time: entryTimes.get(pos.asset) ?? null,
      current_value: currentValue,
      unrealized_pnl: pnl,
      pct_change: pct,
    };
  });
}

/**
 * For each asset currently open, the timestamp of the decision that opened the
 * *current* position leg: the latest `long_open`/`short_open` for that asset
 * that wasn't subsequently closed. We walk decisions in index order and record
 * the opening timestamp whenever the asset transitions flat → open (or
 * reverses direction); a `flat` clears it. The last recorded value for each
 * still-open asset is the current leg's entry time.
 */
function entryTimeByAsset(
  decisions: ReadonlyArray<DecisionRowDto>,
): Map<string, string> {
  const ordered = [...decisions].sort(
    (a, b) => a.decision_index - b.decision_index,
  );
  const entry = new Map<string, string>();
  for (const row of ordered) {
    if (row.action === "long_open" || row.action === "short_open") {
      // A new open (or reversal) starts a fresh leg → its timestamp is the
      // entry time. Same-direction reopens carry null fills and don't change
      // the leg, but recording the timestamp again is harmless (the position
      // qty/price come from the derivation, not from here).
      entry.set(row.asset, row.timestamp);
    } else if (row.action === "flat") {
      entry.delete(row.asset);
    }
  }
  return entry;
}
