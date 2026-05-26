// src/features/marketplace/routes/leaderboard/LeaderboardIndex.tsx
// F4 — /marketplace/leaderboard
// Fetches all canonical slices and renders them as a browsable index,
// each linking to /marketplace/leaderboard/<id>.
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import type { Slice } from "@/features/marketplace/data/types";

export function LeaderboardIndex() {
  const mp = useMarketplaceData();

  const { data: slices = [], isLoading } = useQuery<Slice[]>({
    queryKey: ["marketplace", "slices"],
    queryFn: () => mp.getSlices(),
  });

  if (isLoading) {
    return (
      <div className="px-7 py-8 text-[13px] text-text-3">
        Loading leaderboard…
      </div>
    );
  }

  return (
    <div className="flex flex-col min-h-0 px-7 py-8">
      <h1 className="font-mono text-[18px] font-semibold text-text mb-1">
        Leaderboard
      </h1>
      <p className="font-mono text-[12px] text-text-3 mb-8">
        Canonical slices — ranked lists of top-performing strategies.
      </p>

      <div className="flex flex-col gap-2" data-testid="slices-index">
        {slices.map((slice) => (
          <Link
            key={slice.id}
            to={`/marketplace/leaderboard/${slice.id}`}
            data-testid={`slice-link-${slice.id}`}
            className="flex items-center justify-between px-5 py-4 rounded border border-border hover:border-gold/40 hover:bg-surface-elev/30 transition-colors group"
          >
            <div className="min-w-0">
              <div className="font-mono text-[13px] font-semibold text-text group-hover:text-gold transition-colors">
                {slice.label}
              </div>
              <div className="font-mono text-[11px] text-text-3 mt-0.5">
                {slice.hint}
              </div>
            </div>
            <div className="flex items-center gap-3 shrink-0 ml-4">
              <span
                data-testid={`slice-count-${slice.id}`}
                className="font-mono text-[12px] text-text-2"
              >
                {slice.count.toLocaleString()}
              </span>
              <span className="font-mono text-[11px] text-text-3">
                strategies
              </span>
              <span className="text-text-3 group-hover:text-gold transition-colors text-[14px]">
                →
              </span>
            </div>
          </Link>
        ))}
      </div>
    </div>
  );
}
