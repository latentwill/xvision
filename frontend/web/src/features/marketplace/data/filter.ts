import type { FilterState, ListingRow, SortKey } from "./types";

export function defaultFilterState(): FilterState {
  return {
    segment: "trending",
    search: "",
    sort: "return30d",
    assets: [],
    models: [],
    styles: [],
    tier: [],
    trust: { verifiedOnly: false, acceptsAgents: false, auditedOnly: false },
    priceUsdc: { from: 0, to: 500 },
    minBuyers: 0,
  };
}

const totalBuyers = (r: ListingRow) => r.buyers.humans + r.buyers.agents;

const SORTERS: Record<SortKey, (a: ListingRow, b: ListingRow) => number> = {
  return30d: (a, b) => b.return30dPct - a.return30dPct,
  sharpe: (a, b) => b.sharpe - a.sharpe,
  buyers: (a, b) => totalBuyers(b) - totalBuyers(a),
  newest: (a, b) => b.id.localeCompare(a.id), // fixture proxy; real impl uses publishedAt
};

export function applyFilter(
  rows: ListingRow[],
  f: FilterState,
): { rows: ListingRow[]; total: number; matched: number } {
  const q = f.search.trim().toLowerCase();
  const matched = rows.filter((r) => {
    if (f.assets.length && !f.assets.some((a) => r.assets.includes(a))) return false;
    if (f.models.length && !f.models.includes(r.model)) return false;
    if (f.styles.length && !f.styles.includes(r.style)) return false;
    if (f.tier.length && !f.tier.includes(r.tier)) return false;
    if (f.trust.verifiedOnly && r.verification !== "verified") return false;
    if (f.trust.acceptsAgents && !r.acceptsX402) return false;
    // auditedOnly: no ListingRow field yet — applied in Phase 1.
    if (totalBuyers(r) < f.minBuyers) return false;
    const price = r.priceUsdc ?? 0;
    if (price < f.priceUsdc.from || price > f.priceUsdc.to) return false;
    // Search across every human-meaningful field, not just id + handle —
    // searching "test" must surface a listing named "test" (operator report:
    // the display name lives in `name`, which the old query never looked at).
    if (q) {
      const hay = [
        r.id,
        r.name ?? "",
        r.lineageId,
        r.creator.handle ?? "",
        r.model,
        r.style,
        ...r.assets,
      ]
        .join(" ")
        .toLowerCase();
      if (!hay.includes(q)) return false;
    }
    return true;
  });
  // Segment is a pure FILTER — it never overrides the sort chosen by the user
  // (or set by the segment click handler in Toolbar). The Sort dropdown label
  // ALWAYS matches the actual result order. Segment-canonical sorts are set via
  // setFilter({ segment, sort }) at the call site so the dropdown reflects them.
  const sorted = [...matched].sort(SORTERS[f.sort]);
  return { rows: sorted, total: rows.length, matched: matched.length };
}
