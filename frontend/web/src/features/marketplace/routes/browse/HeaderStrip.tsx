// src/features/marketplace/routes/browse/HeaderStrip.tsx
// Standard app page header for the marketplace browse surface — same anatomy as
// the strategies / eval-runs page headers: plain title, one-line muted
// description, an honest stats line (entries / creators) in muted mono, the
// primary "List your strategy" button, the Wallet link, and a single
// TestnetBadge. Honest data discipline: only ENTRIES / CREATORS are surfaced
// (both real, derived counts); no fabricated paid/minted/purchase metrics. A
// quiet "dev fixtures" marker renders only when running the in-dev fixture
// client. No "Share" button (QA3).
import { Link, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import { TestnetBadge } from "@/features/marketplace/components/TestnetBadge";
import { Icon } from "@/components/primitives/Icon";
import { Topbar } from "@/components/shell/Topbar";
import type { ListingRow, MarketplaceStats } from "@/features/marketplace/data/types";

function fmt(n: number): string {
  return n.toLocaleString("en-US");
}

interface HeaderStripProps {
  /** Loaded listing rows — used to compute the honest CREATORS count. */
  rows?: ListingRow[];
}

export function HeaderStrip({ rows = [] }: HeaderStripProps) {
  const mp = useMarketplaceData();
  const navigate = useNavigate();
  const { data: stats } = useQuery<MarketplaceStats>({
    queryKey: ["marketplace", "stats"],
    queryFn: () => mp.getStats(),
  });

  // Quiet dev marker — only when this build is a dev bundle AND the active
  // client is the in-memory fixture client. Production never shows it.
  const isDevFixture = import.meta.env.DEV && mp.dataSource === "fixture";

  // Honest CREATORS count: distinct creator addresses across the loaded rows.
  const distinctCreators = new Set(rows.map((r) => r.creator.address.toLowerCase())).size;
  const entries = stats?.totalStrategies ?? 0;

  return (
    <div className="px-4 sm:px-7 pt-5 pb-4 border-b border-border">
      <Topbar
        title="Marketplace"
        sub="Buy and sell trading strategies as on-chain agents on Mantle."
      />
      <div className="flex flex-col gap-3 sm:flex-row sm:justify-between sm:items-center sm:gap-6">
        <div className="min-w-0 font-mono text-[11.5px] text-text-3 flex items-center flex-wrap gap-0 tracking-[0.01em]">
          {isDevFixture && (
            <span
              data-testid="dev-fixtures-marker"
              className="mr-2.5 font-mono text-[10px] tracking-[0.04em] rounded border border-border bg-surface-elev px-1.5 py-0.5 text-text-3"
            >
              dev fixtures
            </span>
          )}
          {stats ? (
            <>
              <span>
                <span className="text-text-2">{fmt(entries)}</span> entries
              </span>
              <span className="mx-2.5 text-text-4">·</span>
              <span>
                <span className="text-text-2">{fmt(distinctCreators)}</span> creators
              </span>
            </>
          ) : (
            <span className="text-text-4">Loading…</span>
          )}
        </div>

        <div className="flex gap-2 items-center shrink-0 flex-wrap">
        <TestnetBadge size="sm" />
        <Link
          to="/marketplace/mine"
          aria-label="my listings"
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded border border-border bg-transparent text-text-2 text-[12px] font-medium hover:text-text hover:border-border-strong"
        >
          My Listings
        </Link>
        <Link
          to="/marketplace/wallet"
          aria-label="wallet"
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded border border-border bg-transparent text-text-2 text-[12px] font-medium hover:text-text hover:border-border-strong"
        >
          Wallet
        </Link>
        <button
          type="button"
          aria-label="list your strategy"
          onClick={() => navigate("/marketplace/sell")}
          className="inline-flex items-center justify-center gap-2 rounded bg-gold px-3.5 py-1.5 text-[13px] font-medium text-bg transition-colors hover:bg-gold-soft motion-safe:active:scale-[0.96]"
        >
          <Icon name="plus" size={13} />
          List your strategy
        </button>
        </div>
      </div>
    </div>
  );
}
