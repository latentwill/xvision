// src/features/marketplace/routes/leaderboard/LeaderboardSlice.tsx
// F4 — /marketplace/leaderboard/:sliceId
// Fetches the leaderboard for a given slice and renders the slice header
// plus a ranked list of rows reusing F1's ListingCard.
import { useCallback } from "react";
import { useParams, Link, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import { ListingCard } from "@/features/marketplace/routes/browse/ListingCard";
import type { ListingRow, Slice } from "@/features/marketplace/data/types";

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

export function LeaderboardSlice() {
  const { sliceId = "" } = useParams<{ sliceId: string }>();
  const mp = useMarketplaceData();
  const navigate = useNavigate();

  const { data, isLoading } = useQuery<{ slice: Slice; rows: ListingRow[] }>({
    queryKey: ["marketplace", "leaderboard", sliceId],
    queryFn: () => mp.getLeaderboard(sliceId),
    enabled: !!sliceId,
  });

  // QA #11: the marketplace list CTA must NOT instant-purchase. Route to the
  // strategy detail page (LineageRoute), where requirements are shown before
  // the buyer confirms via the real Acquire / Run-free CTA.
  const handleBuy = useCallback(
    (id: string) => {
      navigate(`/marketplace/lineage/${id}`);
    },
    [navigate]
  );

  if (isLoading) {
    return (
      <div className="px-7 py-8 text-[13px] text-text-3">
        Loading leaderboard…
      </div>
    );
  }

  if (!data) {
    return (
      <div className="px-7 py-8 text-[13px] text-text-3">
        Slice not found.
      </div>
    );
  }

  const { slice, rows } = data;

  return (
    <div className="flex flex-col min-h-0">
      {/* Slice header */}
      <div className="px-7 py-6 border-b border-border/50">
        <div className="flex items-center gap-3 mb-1">
          <Link
            to="/marketplace/leaderboard"
            className="font-mono text-[11px] text-text-3 hover:text-gold transition-colors"
          >
            ← Leaderboard
          </Link>
        </div>
        <h1
          data-testid="slice-label"
          className="font-mono text-[18px] font-semibold text-text mt-2"
        >
          {slice.label}
        </h1>
        <p className="font-mono text-[12px] text-text-3 mt-1">
          {slice.hint}
          <span className="ml-3 text-text-2">
            {slice.count.toLocaleString()} strategies
          </span>
        </p>
      </div>

      {/* Ranked rows */}
      <div className="flex-1 min-h-0 overflow-auto pb-2">
        <ListHeader />
        {rows.length === 0 ? (
          <div className="px-[22px] py-10 text-[13px] text-text-3 text-center">
            No strategies in this slice.
          </div>
        ) : (
          rows.map((row) => (
            <ListingCard key={row.id} row={row} onBuy={handleBuy} />
          ))
        )}
      </div>
    </div>
  );
}
