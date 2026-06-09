// src/features/marketplace/routes/BrowseRoute.tsx
// F1 implementation of the /marketplace browse surface.
// Replaces MarketplaceBrowseStub. All data via useMarketplaceData() + useQuery.
// No popups. FilterDrawer is the F0 docked panel; its content is FilterDrawerContent.
import { useCallback, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import { useFilterState } from "@/features/marketplace/hooks/useFilterState";
import { FilterDrawer } from "@/features/marketplace/components/FilterDrawer";
import { HeaderStrip } from "./browse/HeaderStrip";
import { Toolbar } from "./browse/Toolbar";
import { AppliedChips } from "./browse/AppliedChips";
import { LeaderboardRail } from "./browse/LeaderboardRail";
import { ListingCard } from "./browse/ListingCard";
import { FilterDrawerContent } from "./browse/FilterDrawerContent";
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
// When a slice is active, its asset/model/sort constraints overlay the URL filter.
function mergeSliceFilter(filter: FilterState, slices: Slice[]): FilterState {
  if (!filter.slice) return filter;
  const slice = slices.find((s) => s.id === filter.slice);
  if (!slice) return filter;
  return { ...filter, ...slice.filter };
}

// List header column labels matching the 8-column grid from the design ref.
function ListHeader() {
  return (
    <div
      className="grid items-center gap-3.5 px-[22px] py-2.5 border-b border-border/50 sticky top-0 bg-bg z-[1]"
      style={{ gridTemplateColumns: "56px 1.8fr 0.75fr 1.1fr 1.05fr 0.6fr 0.85fr 110px" }}
    >
      {["", "Strategy", "Assets", "30d return", "Buyers", "Sharpe", "Price", ""].map(
        (h, i) => (
          <div
            key={i}
            className={`font-mono text-[9px] tracking-[0.2em] uppercase text-text-3 font-semibold ${
              i === 3 || i === 5 ? "text-right" : "text-left"
            }`}
          >
            {h}
          </div>
        )
      )}
    </div>
  );
}

export function BrowseRoute() {
  const mp = useMarketplaceData();
  const { filter, setFilter } = useFilterState();
  const [drawerOpen, setDrawerOpen] = useState(false);

  // Load slices first (needed to merge slice filter into listing query)
  const { data: slices = [] } = useQuery<Slice[]>({
    queryKey: ["marketplace", "slices"],
    queryFn: () => mp.getSlices(),
  });

  // Merge the active slice's filter fields into the effective filter for the query
  const effectiveFilter = mergeSliceFilter(filter, slices);

  // Load listings using the merged filter
  const { data: listingsResult } = useQuery<{ rows: ListingRow[]; total: number; matched: number }>({
    queryKey: ["marketplace", "listings", effectiveFilter],
    queryFn: () => mp.listListings(effectiveFilter),
    placeholderData: { rows: [], total: 0, matched: 0 },
  });

  const rows = listingsResult?.rows ?? [];
  const matched = listingsResult?.matched ?? 0;

  const handleBuy = useCallback(
    async (id: string) => {
      // DEPLOY WALL (C7 / AM6 + signer): fixture `purchaseIntent` returns a fake
      // TxRef. The real on-chain EIP-3009 `buyWithAuthorization` swaps in here
      // once contracts are deployed and `useWallet` exposes a signer. Do NOT
      // fake the signing flow. See LineageRoute buyMutation for the full note.
      await mp.purchaseIntent(id);
      // TODO(F6): navigate(`/marketplace/receipts/${ref.txHash}`)
    },
    [mp]
  );

  const handleSliceClick = useCallback(
    (sliceId: SliceId) => {
      // Toggle: click same slice again to deselect
      setFilter({ slice: filter.slice === sliceId ? undefined : sliceId });
    },
    [filter.slice, setFilter]
  );

  const filterCount = countActiveFilters(filter);

  return (
    <div className="flex flex-col min-h-0">
      <HeaderStrip />

      <Toolbar
        filter={filter}
        setFilter={setFilter}
        filterCount={filterCount}
        onOpenDrawer={() => setDrawerOpen(true)}
        matchCount={matched}
      />

      <AppliedChips filter={filter} setFilter={setFilter} matchCount={matched} />

      {/* Body: leaderboard rail | list + optional drawer overlay */}
      <div
        className="flex-1 min-h-0 grid overflow-hidden relative"
        style={{ gridTemplateColumns: "232px 1fr" }}
      >
        <LeaderboardRail
          activeSliceId={filter.slice}
          onSliceClick={handleSliceClick}
        />

        {/* List area */}
        <div className="overflow-auto pb-2">
          <ListHeader />
          {rows.length === 0 ? (
            <div className="px-[22px] py-10 text-[13px] text-text-3 text-center">
              No strategies match the current filters.
            </div>
          ) : (
            rows.map((row) => (
              <ListingCard key={row.id} row={row} onBuy={handleBuy} />
            ))
          )}
        </div>

        {/* FilterDrawer docked panel — covers list area, rail stays visible */}
        <FilterDrawer
          open={drawerOpen}
          onClose={() => setDrawerOpen(false)}
          title="Filter strategies"
        >
          <FilterDrawerContent
            filter={filter}
            setFilter={setFilter}
            matchCount={matched}
            onClose={() => setDrawerOpen(false)}
          />
        </FilterDrawer>
      </div>
    </div>
  );
}
