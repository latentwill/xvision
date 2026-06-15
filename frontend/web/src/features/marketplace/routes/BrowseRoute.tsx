// src/features/marketplace/routes/BrowseRoute.tsx
// The /marketplace browse surface (spec 3.1).
// Single full-width vertical stack: header → Toolbar → AppliedChips → SliceChips
// → list of ListingEntry rows. No leaderboard rail, no list-row buy flow,
// no popups (the filter panel is an inline accordion in document flow). Rows are
// whole <Link>s to the inspector — inspect-before-buy is the browse ethos.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import { useFilterState } from "@/features/marketplace/hooks/useFilterState";
import { EmptyState } from "@/components/chart/v2/primitives/EmptyState";
import { FilterDrawer } from "@/features/marketplace/components/FilterDrawer";
import { HeaderStrip } from "./browse/HeaderStrip";
import { Toolbar, type BrowseView } from "./browse/Toolbar";
import { AppliedChips } from "./browse/AppliedChips";
import { SliceChips } from "./browse/SliceChips";
import { ListingEntry, humanize } from "./browse/ListingEntry";
import { FilterDrawerContent } from "./browse/FilterDrawerContent";
import { isFreeListing } from "@/features/marketplace/data/pricing";
import type { FilterState, ListingRow, Slice, SliceId } from "@/features/marketplace/data/types";

function countActiveFilters(filter: FilterState): number {
  return (
    filter.assets.length +
    filter.models.length +
    filter.styles.length +
    filter.tier.length +
    (filter.trust.verifiedOnly ? 1 : 0) +
    (filter.trust.acceptsAgents ? 1 : 0) +
    (filter.trust.auditedOnly ? 1 : 0) +
    (filter.minBuyers > 0 ? 1 : 0) +
    (filter.priceUsdc.from !== 0 || filter.priceUsdc.to !== 500 ? 1 : 0)
  );
}

// Merge active slice's filter fields into the user filter.
function mergeSliceFilter(filter: FilterState, slices: Slice[]): FilterState {
  if (!filter.slice) return filter;
  const slice = slices.find((s) => s.id === filter.slice);
  if (!slice) return filter;
  return { ...filter, ...slice.filter };
}

export function BrowseRoute() {
  const mp = useMarketplaceData();
  const { filter, setFilter } = useFilterState();
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [view, setView] = useState<BrowseView>("list");

  // Whether the active client is the fixture/demo client (drives the DEMO
  // marker and the sort-options gating).
  const isDemo = mp.dataSource === "fixture";

  // Load slices first (needed to merge slice filter into listing query).
  const { data: slices = [] } = useQuery<Slice[]>({
    queryKey: ["marketplace", "slices"],
    queryFn: () => mp.getSlices(),
  });

  const effectiveFilter = mergeSliceFilter(filter, slices);

  const { data: listingsResult } = useQuery<{ rows: ListingRow[]; total: number; matched: number }>({
    queryKey: ["marketplace", "listings", effectiveFilter],
    queryFn: () => mp.listListings(effectiveFilter),
    placeholderData: { rows: [], total: 0, matched: 0 },
  });

  const rows = listingsResult?.rows ?? [];
  const matched = listingsResult?.matched ?? 0;
  const total = listingsResult?.total ?? 0;

  // On the real client, hide return/sharpe sort options when every value is 0
  // (sorting on zeros is meaningless). The demo client always allows them.
  const allowPerformanceSort = useMemo(() => {
    if (isDemo) return true;
    return rows.some((r) => r.return30dPct !== 0 || r.sharpe !== 0);
  }, [isDemo, rows]);

  const handleSliceClick = useCallback(
    (sliceId: SliceId) => {
      // Toggle: click same slice again to deselect.
      setFilter({ slice: filter.slice === sliceId ? undefined : sliceId });
    },
    [filter.slice, setFilter]
  );

  // Inline filter accordion closes on Escape (no overlay, but keyboard-friendly).
  useEffect(() => {
    if (!filtersOpen) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setFiltersOpen(false);
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [filtersOpen]);

  // When the accordion opens below the hero, its sticky footer (Clear all ·
  // matches · Done) can land just past the viewport bottom. The reveal is
  // instant, so scroll the section minimally into view right after layout so
  // the footer is always clickable without hunting.
  const filterSectionRef = useRef<HTMLElement>(null);
  useEffect(() => {
    if (!filtersOpen) return;
    const t = setTimeout(() => {
      filterSectionRef.current?.scrollIntoView?.({ block: "nearest" });
    }, 30);
    return () => clearTimeout(t);
  }, [filtersOpen]);

  const filterCount = countActiveFilters(filter);

  return (
    <div className="flex flex-col min-h-0">
      <HeaderStrip rows={rows} />

      <Toolbar
        filter={filter}
        setFilter={setFilter}
        filterCount={filterCount}
        filtersOpen={filtersOpen}
        onToggleFilters={() => setFiltersOpen((o) => !o)}
        matchCount={matched}
        view={view}
        setView={setView}
        allowPerformanceSort={allowPerformanceSort}
        isDemo={isDemo}
      />

      {/* Inline filter accordion — in document flow, pushes the list down */}
      <FilterDrawer open={filtersOpen} title="Filter strategies" sectionRef={filterSectionRef}>
        <FilterDrawerContent
          filter={filter}
          setFilter={setFilter}
          matchCount={matched}
          totalCount={total}
          onClose={() => setFiltersOpen(false)}
        />
      </FilterDrawer>

      <AppliedChips filter={filter} setFilter={setFilter} matchCount={matched} />

      {/* Slice chip strip — renders only when a slice has a real count > 0 */}
      <SliceChips
        slices={slices}
        activeSliceId={filter.slice}
        onSliceClick={handleSliceClick}
      />

      {/* Listing list (single full-width column) */}
      <div className="flex-1 min-h-0 overflow-auto pb-6">
        {total === 0 ? (
          <div className="px-4 sm:px-7 py-10">
            <EmptyState
              title="No strategies yet"
              message="No strategies minted yet."
            />
            <div className="mt-4 text-center">
              <Link
                to="/marketplace/sell"
                className="text-[12px] text-gold hover:underline underline-offset-2"
              >
                List your strategy
              </Link>
            </div>
          </div>
        ) : matched === 0 ? (
          <div className="px-4 sm:px-7 py-10 text-center text-[13px] text-text-3">
            No strategies match the current filters.
          </div>
        ) : view === "index" ? (
          <IndexTable rows={rows} />
        ) : (
          // Honest-data spine (spec §3.1E): real on-chain rows carry no equity
          // series, so they fall through to the dignified "pending first live
          // cycle" caption — never a fabricated micro-curve.
          //
          // On the dev fixture client only, the curated named listings carry
          // real curated returns (e.g. btc-momentum-v3 at +47.2%); the spec
          // (§3.1E) calls for a real MiniSparkline there so the demo doesn't
          // look uniformly blank on performance. The 200 deterministic
          // `wall-strat-*` filler rows are NOT real data, so they stay on the
          // honest pending caption.
          rows.map((row, i) => (
            <ListingEntry
              key={row.id}
              row={row}
              index={i}
              showSparkline={isDemo && !row.id.startsWith("wall-strat-")}
            />
          ))
        )}
      </div>
    </div>
  );
}

// Dense mono fallback table for power users (spec 3.1B view toggle). Real
// fields only, hairline rules, NO sparkline. The List view is the default;
// the Index view is opt-in.
function IndexTable({ rows }: { rows: ListingRow[] }) {
  return (
    <table className="w-full border-collapse font-mono text-[12px]">
      <thead>
        <tr className="border-b border-border text-left">
          {["Strategy", "Tier", "Price", "Creator"].map((h) => (
            <th
              key={h}
              className="px-4 sm:px-7 py-2 font-semibold text-[9px] tracking-[0.18em] uppercase text-text-3"
            >
              {h}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => {
          const isOpen = isFreeListing(row);
          return (
            <tr key={row.id} className="border-b border-border-soft hover:bg-surface-hover">
              <td className="px-4 sm:px-7 py-2">
                <Link
                  to={`/marketplace/lineage/${row.id}`}
                  className="text-text hover:underline underline-offset-2"
                >
                  {row.name ?? humanize(row.id)}
                </Link>
                <span className="text-text-3 ml-1.5">{row.version}</span>
              </td>
              <td className="px-4 sm:px-7 py-2 text-text-2">{isOpen ? "Open" : "Sealed"}</td>
              <td className="px-4 sm:px-7 py-2 text-text-2 tabular-nums">
                {isOpen ? "—" : `${row.priceUsdc} USDC`}
              </td>
              <td className="px-4 sm:px-7 py-2 text-text-3">
                {row.creator.handle ?? `${row.creator.address.slice(0, 8)}…`}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
