// Adapter: real `DecisionRowDto` → the Signal Decisions-table view model.
//
// The design handoff's mock `Decision` shape (README §"Data shape") splits each
// step into `engaged` vs `filtered` phases, where a filtered step carries only
// `{ i, t, phase }` — no action/conviction/justification. The real engine wire
// shape (`DecisionRowDto`) does NOT have an explicit `phase` field, so we derive
// it here from fields already present. No backend change.
//
// Phase derivation (engaged vs filtered):
//   A step is FILTERED when no engaged trader decision happened — i.e. it was
//   intercepted by a risk/freshness/regime filter or synthesized by a guardrail.
//   We treat a row as filtered when EITHER:
//     (a) its justification/reasoning carries a synthesized-row marker
//         (`noop_skip`, `inherited from early-stop policy`,
//         `trader_skipped_by_graph`) — these are the same markers the existing
//         Decision-provenance panel counts as "synthesized, not a direct trader
//         model decision"; OR
//     (b) it is a no-op HOLD/flat-from-flat step that produced no order, no
//         conviction, and no justification (the trader never engaged).
//   Everything else (any real BUY/SELL/SHORT/COVER, or a HOLD that the trader
//   deliberately reasoned out) is ENGAGED.
//
// Action mapping reuses the same prior-side logic the legacy table used so the
// pill verb matches intent: long_open→BUY, short_open→SELL(short entry),
// flat-after-long→SELL, flat-after-short→CLOSE(cover), hold→HOLD.

import type { DecisionRowDto } from "@/api/types.gen";
import {
  derivePriorSideByDecision,
  type PositionSide,
} from "@/features/decisions/positions";
import type { ActionPillAction } from "./ActionPill";
import type { Phase } from "./PhaseChip";

export type TimelineDecision = {
  /** decision index (matches `decision_index`, used as the jump key). */
  i: number;
  /** raw ISO timestamp (rendered as HH:MM:SS.mmm by callers). */
  t: string;
  phase: Phase;
  /** present only when engaged. */
  action?: ActionPillAction;
  /** 0..1 conviction, present only when engaged. */
  conv?: number;
  /** justification/reasoning text, present only when engaged. */
  just?: string;
  /** realized PnL for the step, present only when engaged. */
  pnl?: number | null;
  /** asset symbol — kept for search hay + tooltip context. */
  asset: string;
};

const SYNTHETIC_MARKERS = [
  "noop_skip",
  "inherited from early-stop policy",
  "trader_skipped_by_graph",
];

function isSyntheticRow(row: DecisionRowDto): boolean {
  const text = `${row.justification ?? ""} ${row.reasoning ?? ""}`.toLowerCase();
  return SYNTHETIC_MARKERS.some((m) => text.includes(m));
}

function derivePhase(row: DecisionRowDto): Phase {
  if (isSyntheticRow(row)) return "filtered";
  const isNoopHold = row.action === "hold" || row.action === "flat";
  const noOrder = row.order_size == null || row.order_size === 0;
  const noConviction = row.conviction == null;
  const justification = `${row.justification ?? ""}${row.reasoning ?? ""}`.trim();
  if (isNoopHold && noOrder && noConviction && justification.length === 0) {
    return "filtered";
  }
  return "engaged";
}

export function mapAction(action: string, priorSide: PositionSide): ActionPillAction {
  if (action === "long_open") return "BUY";
  if (action === "short_open") return "SELL";
  if (action === "flat") {
    if (priorSide === "long") return "SELL";
    if (priorSide === "short") return "CLOSE";
    return "HOLD";
  }
  return "HOLD";
}

export function justificationText(row: DecisionRowDto): string {
  return row.reasoning?.trim() || row.justification?.trim() || "";
}

/** "BTC/USD" → "BTC"; bare symbols and the empty string pass through. The full
 *  pair stays available for tooltip/search; this is just the column label. */
export function shortAsset(asset: string): string {
  return asset.split("/")[0] ?? asset;
}

/**
 * Assign a 1-based *step* ordinal per distinct timestamp, ranked chronologically,
 * returning a map keyed by `i` (decision_index).
 *
 * A multi-asset run fans one decision step out into one row per asset, all sharing
 * that step's timestamp (e.g. decision_index 0=BTC and 1=ETH both at 20:00). Those
 * rows collapse to the same step number here, so the table can show the step on the
 * first row and blank the rest instead of counting per-asset rows as separate steps.
 *
 * Computed over the FULL decision list (not a filtered view) so a row's step number
 * stays stable when the table is filtered — step 33 reads "33" even if step 32 is
 * filtered out.
 */
export function stepOrdinalsByDecision(rows: TimelineDecision[]): Map<number, number> {
  const distinct = [...new Set(rows.map((r) => r.t))].sort(
    (a, b) => new Date(a).getTime() - new Date(b).getTime(),
  );
  const stepByTs = new Map<string, number>();
  distinct.forEach((t, idx) => stepByTs.set(t, idx + 1));
  const out = new Map<number, number>();
  for (const r of rows) out.set(r.i, stepByTs.get(r.t) ?? 0);
  return out;
}

/**
 * Project the real decision rows into the Signal table/timeline view model,
 * computing `phase` and the direction-aware action verb per row.
 */
export function toTimelineDecisions(rows: DecisionRowDto[]): TimelineDecision[] {
  const priorSide = derivePriorSideByDecision(rows);
  return rows.map((row) => {
    const phase = derivePhase(row);
    if (phase === "filtered") {
      return { i: row.decision_index, t: row.timestamp, phase, asset: row.asset };
    }
    const action = mapAction(row.action, priorSide.get(row.decision_index) ?? "flat");
    return {
      i: row.decision_index,
      t: row.timestamp,
      phase,
      action,
      conv: row.conviction ?? undefined,
      just: justificationText(row) || undefined,
      pnl: row.pnl_realized,
      asset: row.asset,
    };
  });
}
