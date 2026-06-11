// src/features/marketplace/routes/browse/HeaderStrip.tsx
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import type { MarketplaceStats } from "@/features/marketplace/data/types";

function fmt(n: number): string {
  return n.toLocaleString("en-US");
}

function fmtUsd(n: number): string {
  return `$${n.toLocaleString("en-US")}`;
}

export function HeaderStrip() {
  const mp = useMarketplaceData();
  const { data: stats } = useQuery<MarketplaceStats>({
    queryKey: ["marketplace", "stats"],
    queryFn: () => mp.getStats(),
  });

  return (
    <div className="px-4 sm:px-7 py-5 border-b border-border flex flex-col gap-4 sm:flex-row sm:justify-between sm:items-end sm:gap-6">
      <div className="min-w-0 max-w-[780px]">
        <h1 className="m-0 text-[24px] font-semibold tracking-[-0.025em] leading-[1.15]">
          Buy a strategy. Run it. Or share yours and get paid.
        </h1>
        <div className="mt-2.5 text-[11.5px] font-mono text-text-3 flex items-center flex-wrap gap-0 tracking-[0.01em]">
          {stats ? (
            <>
              <span>
                <span className="text-text-2">{fmt(stats.totalStrategies)}</span> strategies
              </span>
              <span className="mx-2.5 text-text-4">·</span>
              <span>
                <span className="text-gold">{fmtUsd(stats.paidThisWeekUsd)}</span> paid this week
              </span>
              <span className="mx-2.5 text-text-4">·</span>
              <span className="inline-flex items-center gap-1">
                <AgentIcon size={11} />
                <span>
                  <span className="text-text-2">{fmt(stats.agentPurchases)}</span> agent purchases
                </span>
              </span>
              <span className="mx-2.5 text-text-4">·</span>
              <span>
                <span className="text-text-2">{fmt(stats.mintedLast24h)}</span> minted in 24h
              </span>
            </>
          ) : (
            <span className="text-text-4">Loading…</span>
          )}
        </div>
      </div>
      <div className="flex gap-2 items-center shrink-0">
        <Link
          to="/marketplace/wallet"
          aria-label="wallet"
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded border border-border-strong bg-transparent text-text-2 text-[12px] font-medium hover:text-text hover:border-border"
        >
          Wallet
        </Link>
        <button
          type="button"
          aria-label="share"
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded border border-border-strong bg-transparent text-text-2 text-[12px] font-medium hover:text-text hover:border-border"
        >
          Share
        </button>
        <button
          type="button"
          aria-label="share your strategy"
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded border border-gold/60 bg-gold/10 text-gold text-[12px] font-medium hover:bg-gold/20"
        >
          + Share your strategy
        </button>
      </div>
    </div>
  );
}
