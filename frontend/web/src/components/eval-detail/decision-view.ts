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
// pill verb matches intent: long_open→BUY, short_open→SHORT,
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
  /** raw ISO timestamp. Callers render via `fmtStepStamp` (UTC, date-bearing)
   *  so multi-day runs can be read at a glance; keep the raw ISO for `title=`
   *  tooltips and downstream tooling. */
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
  /** Exit reason for mechanistic strategy decisions. Present when the
   *  backend populates `exit_reason` on the `DecisionRowDto`. */
  exit_reason?: string | null;
  /** true when the decision was accepted on a stale bar (bar age >
   *  stale-data-max-age-ms). Live/forward-test only. */
  delayed?: boolean;
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
  if (action === "short_open") return "SHORT";
  if (action === "flat") {
    if (priorSide === "long") return "SELL";
    if (priorSide === "short") return "CLOSE";
    return "HOLD";
  }
  // sltp force-close rows: action is the exit reason ("stop_loss", "take_profit", etc.)
  if (
    action === "stop_loss" ||
    action === "take_profit" ||
    action === "trailing_stop" ||
    action === "partial_tp1" ||
    action === "partial_tp2"
  ) {
    if (priorSide === "short") return "CLOSE";
    return "SELL";
  }
  return "HOLD";
}

/** Extract the sltp exit reason from a justification string of the form "sltp: <reason>". */
function extractSltpExitReason(justification: string | null | undefined): string | null {
  const j = justification?.trim() ?? "";
  if (j.startsWith("sltp: ")) return j.slice(6);
  return null;
}

export function justificationText(row: DecisionRowDto): string {
  const j = row.justification?.trim() ?? "";
  // Don't show raw "sltp: stop_loss" as justification text — it's surfaced via ExitReasonTag
  if (j.startsWith("sltp: ")) return row.reasoning?.trim() || "";
  return row.reasoning?.trim() || j || "";
}

/** "BTC/USD" → "BTC"; bare symbols and the empty string pass through. The full
 *  pair stays available for tooltip/search; this is just the column label. */
export function shortAsset(asset: string): string {
  return asset.split("/")[0] ?? asset;
}

/**
 * Render a step's raw ISO timestamp as `YYYY-MM-DD HH:MM:SS` in UTC.
 *
 * Shared by the Decisions table TIMESTAMP column and the density-strip hover
 * tooltip so the two surfaces can't drift. The format is intentionally
 * locale-free and tabular — sortable as a string, copy-pasteable into the CLI,
 * and unambiguous regardless of the operator's locale.
 *
 * Why date + seconds, no milliseconds: scenarios span days to months, so the
 * date is load-bearing for orientation; bar boundaries are integer seconds
 * (the engine writes `YYYY-MM-DDTHH:MM:SSZ`), so milliseconds are always `.000`
 * and would only add noise. Anyone who needs the original ISO can read the
 * full string from the row's `title=` attribute.
 *
 * Returns the raw input on parse failure (matches the previous behaviour of
 * the two local formatters this replaced).
 */
export function fmtStepStamp(t: string): string {
  const d = new Date(t);
  if (Number.isNaN(d.getTime())) return t;
  const yyyy = String(d.getUTCFullYear()).padStart(4, "0");
  const MM = String(d.getUTCMonth() + 1).padStart(2, "0");
  const dd = String(d.getUTCDate()).padStart(2, "0");
  const hh = String(d.getUTCHours()).padStart(2, "0");
  const mm = String(d.getUTCMinutes()).padStart(2, "0");
  const ss = String(d.getUTCSeconds()).padStart(2, "0");
  return `${yyyy}-${MM}-${dd} ${hh}:${mm}:${ss}`;
}

/**
 * Step- vs row-level counts for the Decisions summary chips.
 *
 * A multi-asset wakeup is ONE decision step in the strategy's perspective; the
 * per-asset trader calls are children of that step. The deployed UI used to
 * count rows where it meant steps ("22 of 22 decisions · 5 steps · 22 engaged"
 * for a 5-step / 5-asset run), inflating the cardinality the operator was
 * trying to read. These four numbers are the source of truth for the
 * summary-chip strip on the desktop table, the density-strip header, and the
 * mobile Decisions tab.
 *
 * Semantics:
 *   - `totalSteps` / `viewedSteps` count distinct timestamps in the full data
 *     and the filtered/searched view respectively.
 *   - `engagedSteps` counts visible timestamps where at least one row produced
 *     a real trader decision (phase !== "filtered"). Scoped to the view so it
 *     stays consistent with `viewedSteps` under filtering.
 *   - `viewedTraderCalls` / `totalTraderCalls` are the per-asset row counts —
 *     i.e. how many trader invocations happened. A "trader call" can be either
 *     engaged (real decision) or filtered (synthesized no-op).
 */
export type DecisionCounts = {
  viewedSteps: number;
  totalSteps: number;
  engagedSteps: number;
  viewedTraderCalls: number;
  totalTraderCalls: number;
};

export function decisionCounts(
  filteredView: TimelineDecision[],
  all: TimelineDecision[],
): DecisionCounts {
  const viewSteps = new Set<string>();
  const engagedSteps = new Set<string>();
  for (const d of filteredView) {
    viewSteps.add(d.t);
    if (d.phase !== "filtered") engagedSteps.add(d.t);
  }
  const totalSteps = new Set(all.map((d) => d.t));
  return {
    viewedSteps: viewSteps.size,
    totalSteps: totalSteps.size,
    engagedSteps: engagedSteps.size,
    viewedTraderCalls: filteredView.length,
    totalTraderCalls: all.length,
  };
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
      return { i: row.decision_index, t: row.timestamp, phase, asset: row.asset, delayed: row.delayed };
    }
    const priorSideForRow = priorSide.get(row.decision_index) ?? "flat";
    // exit_reason: either a future DTO field or extracted from the "sltp: <reason>" justification prefix
    const exit_reason =
      extractSltpExitReason(row.justification);
    let action: ActionPillAction;
    if (exit_reason) {
      // Position was force-closed by risk engine; show actual closing action
      action = priorSideForRow === "short" ? "CLOSE" : "SELL";
    } else {
      action = mapAction(row.action, priorSideForRow);
    }
    return {
      i: row.decision_index,
      t: row.timestamp,
      phase,
      action,
      conv: row.conviction ?? undefined,
      just: justificationText(row) || undefined,
      pnl: row.pnl_realized,
      asset: row.asset,
      exit_reason,
      delayed: row.delayed,
    };
  });
}
