// frontend/web/src/features/decisions/positions.ts
//
// Per-decision open-position derivation.
//
// The eval-runs decisions table renders one row per cycle, but the
// raw `DecisionRowDto` only carries the *action* taken (long_open,
// short_open, flat, hold) plus the fill detail when an order crossed
// the book. From those rows we can walk the sequence in time order
// and reconstruct the running per-asset position state after each
// decision. Operators reported (2026-05-18) that a short_open
// followed by a CLOSE/HOLD row was ambiguous because position state
// wasn't visible — this module is the data half of that fix.
//
// The derivation is intentionally client-side so this contract
// doesn't have to expand the on-the-wire `DecisionRowDto`. If a
// future contract teaches the engine to persist position state
// directly, the table can fall back to the server value while this
// derivation stays available for legacy runs.

import type { DecisionRowDto } from "@/api/types.gen";

export type PositionSide = "long" | "short" | "flat";

/**
 * A single open position at a point in time. `qty` is always
 * positive; `side` carries the direction. `entry_price` is the
 * price at which the open leg was filled — for averaging or partial
 * fills we keep the most recent open's price (v1 sim does not
 * scale-in, so this matches the engine's `entry_price` state).
 *
 * Flat positions are represented by *absence from the list*, not
 * by `{ side: "flat", qty: 0 }`. Callers can render an empty list
 * as "—" or "flat" — the table's display layer decides the copy.
 */
export type OpenPosition = {
  asset: string;
  side: Exclude<PositionSide, "flat">;
  qty: number;
  entry_price: number;
};

type RunningState = Map<string, { side: PositionSide; qty: number; entry: number }>;

/**
 * Walk decisions in `decision_index` order and emit the open-position
 * list **after** each row's fill is applied. Returns a map keyed by
 * `decision_index`, sorted ascending in the caller's order.
 *
 * The map is keyed by `decision_index` (not array index) so callers
 * can filter the visible rows without breaking the lookup —
 * filtering is purely display-side; the derivation walks the full
 * unfiltered sequence.
 */
export function derivePositionsByDecision(
  rows: ReadonlyArray<DecisionRowDto>,
): Map<number, OpenPosition[]> {
  const ordered = [...rows].sort((a, b) => a.decision_index - b.decision_index);
  const state: RunningState = new Map();
  const out = new Map<number, OpenPosition[]>();

  for (const row of ordered) {
    applyAction(state, row);
    out.set(row.decision_index, snapshot(state));
  }
  return out;
}

/**
 * Per-decision snapshot of the position **before** the row's action
 * is applied. Used to disambiguate `flat` (which is rendered as SELL
 * when it closes a long and COVER when it closes a short) — see
 * `decisionActionLabel` in `eval-runs-detail.tsx`. Same walk and
 * sort order as `derivePositionsByDecision` so the two maps stay
 * aligned on `decision_index`.
 */
export function derivePriorPositionsByDecision(
  rows: ReadonlyArray<DecisionRowDto>,
): Map<number, OpenPosition[]> {
  const ordered = [...rows].sort((a, b) => a.decision_index - b.decision_index);
  const state: RunningState = new Map();
  const out = new Map<number, OpenPosition[]>();

  for (const row of ordered) {
    out.set(row.decision_index, snapshot(state));
    applyAction(state, row);
  }
  return out;
}

function applyAction(state: RunningState, row: DecisionRowDto): void {
  const asset = row.asset;
  const current = state.get(asset) ?? { side: "flat" as PositionSide, qty: 0, entry: 0 };

  switch (row.action) {
    case "long_open":
      applyOpen(state, asset, "long", current, row);
      return;
    case "short_open":
      applyOpen(state, asset, "short", current, row);
      return;
    case "flat": {
      // Close. Drop the asset out of the state map so it stops
      // appearing in the snapshot — this is what makes "the bar
      // after CLOSE shows zero open positions" trivially true.
      if (current.side === "flat") return;
      state.delete(asset);
      return;
    }
    case "hold":
    default:
      // No change. Includes HOLD plus any future / unknown actions
      // — defensive default so an unrecognised action doesn't
      // silently mutate position state.
      return;
  }
}

/**
 * Apply an open action (`long_open` or `short_open`) against the
 * current per-asset state. Handles three cases the engine
 * distinguishes:
 *
 * 1. **Already in the requested direction** — same-side reopen.
 *    Backtest's `simulate_fill` short-circuits (`fill_price: None,
 *    fill_size: None`) on direction match, so the row carries null
 *    fills and we no-op via the zero/null guard.
 *
 * 2. **Reverse from the opposite direction.** The engine records
 *    `traded_units = |old| + |new|` in
 *    `crates/xvision-engine/src/eval/executor/backtest.rs:761-767`,
 *    so the new leg's size is `fill_size - |prev_qty|`. Paper-mode
 *    Alpaca crypto can't hold a short, so
 *    `crates/xvision-engine/src/eval/executor/paper.rs:472-486`
 *    collapses `short_open` while long into a sell sized exactly to
 *    the open long. The row still records
 *    `action: "short_open"` and `fill_size` equal to the closed
 *    long, so `fill_size - |prev|` resolves to 0 → flat. The same
 *    formula handles both modes — see engine test
 *    `crates/xvision-engine/tests/eval_executor_paper.rs:265`
 *    (`paper_executor_crypto_short_open_closes_existing_long`),
 *    which asserts the broker is flat after that sequence.
 *
 * 3. **From flat** — straight open at `fill_size`.
 */
function applyOpen(
  state: RunningState,
  asset: string,
  want: Exclude<PositionSide, "flat">,
  current: { side: PositionSide; qty: number; entry: number },
  row: DecisionRowDto,
): void {
  if (current.side === want) return;

  const fill = row.fill_size ?? 0;
  const price = row.fill_price ?? 0;
  if (fill <= 0 || price <= 0) return;

  const opposite = want === "long" ? "short" : "long";
  if (current.side === opposite) {
    // Reverse / collapse: traded units cover the closed leg plus
    // whatever new size the engine opened (paper close-out → 0).
    const newQty = fill - Math.abs(current.qty);
    if (newQty <= 1e-9) {
      state.delete(asset);
      return;
    }
    state.set(asset, { side: want, qty: newQty, entry: price });
    return;
  }

  // From flat → pure open.
  state.set(asset, { side: want, qty: fill, entry: price });
}

function snapshot(state: RunningState): OpenPosition[] {
  const out: OpenPosition[] = [];
  for (const [asset, pos] of state) {
    if (pos.side === "flat" || pos.qty <= 0) continue;
    out.push({ asset, side: pos.side, qty: pos.qty, entry_price: pos.entry });
  }
  // Stable order: alphabetical by asset, so the rendered chips
  // don't reshuffle across renders.
  out.sort((a, b) => a.asset.localeCompare(b.asset));
  return out;
}
