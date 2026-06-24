// Shared filter/sort vocabulary for the Signal Decisions table + density strip.
// Keeping the matcher in one place guarantees the pill row, the table rows, and
// the strip-dimming all agree on what a given filter selects.

import type { TimelineDecision } from "./decision-view";

// Mutually-exclusive action filter (radio semantics). Mirrors README §7's pill
// row All/Long/Sell/Short/Hold. CLOSE (cover) rows are exit/sell-side, so the
// SELL filter matches both SELL and CLOSE — otherwise short-cover steps would
// be unreachable by any action filter.
export type ActionFilter = "all" | "LONG" | "SELL" | "SHORT" | "HOLD";

export type SortKey = "time-asc" | "time-desc" | "conv-desc" | "pnl-desc";

export function matchesActionFilter(d: TimelineDecision, filter: ActionFilter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "SHORT":
      return d.phase !== "filtered" && d.action === "SHORT";
    case "LONG":
      return d.phase !== "filtered" && d.action === "LONG";
    case "SELL":
      return d.phase !== "filtered" && (d.action === "SELL" || d.action === "CLOSE");
    case "HOLD":
      return d.phase !== "filtered" && d.action === "HOLD";
    default:
      return true;
  }
}

export function searchHay(d: TimelineDecision): string {
  return [String(d.i), d.t, d.phase, d.action ?? "", d.asset, d.just ?? ""]
    .join(" ")
    .toLowerCase();
}

export function sortDecisions(rows: TimelineDecision[], sortKey: SortKey): TimelineDecision[] {
  const cp = [...rows];
  switch (sortKey) {
    case "time-asc":
      cp.sort((a, b) => a.i - b.i);
      break;
    case "time-desc":
      cp.sort((a, b) => b.i - a.i);
      break;
    case "conv-desc":
      cp.sort((a, b) => (b.conv ?? 0) - (a.conv ?? 0));
      break;
    case "pnl-desc":
      cp.sort((a, b) => (b.pnl ?? 0) - (a.pnl ?? 0));
      break;
  }
  return cp;
}

export type CountMap = Record<ActionFilter, number>;

export function actionCounts(rows: TimelineDecision[]): CountMap {
  return {
    all: rows.length,
    LONG: rows.filter((d) => matchesActionFilter(d, "LONG")).length,
    SELL: rows.filter((d) => matchesActionFilter(d, "SELL")).length,
    SHORT: rows.filter((d) => matchesActionFilter(d, "SHORT")).length,
    HOLD: rows.filter((d) => matchesActionFilter(d, "HOLD")).length,
  };
}
