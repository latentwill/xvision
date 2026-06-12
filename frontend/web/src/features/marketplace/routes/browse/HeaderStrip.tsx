// src/features/marketplace/routes/browse/HeaderStrip.tsx
// The Catalogue hero strip (spec 3.1A). Editorial eyebrow + Fraunces headline +
// honest stat ledger. No fake numbers: only ENTRIES / CREATORS / PAID TO
// CREATORS, with an em-dash for the unbacked field. DEMO CATALOGUE marker when
// the active data client is the fixture client. Single mint CTA, single TESTNET
// badge, Wallet link. No "Share" button (QA3).
import { Link, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import { TestnetBadge } from "@/features/marketplace/components/TestnetBadge";
import { GrainOverlay } from "@/components/chart/v2/primitives/GrainOverlay";
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

  const isDemo = mp.dataSource === "fixture";

  // Honest CREATORS count: distinct creator addresses across the loaded rows.
  const distinctCreators = new Set(rows.map((r) => r.creator.address.toLowerCase())).size;
  const entries = stats?.totalStrategies ?? 0;
  // A deliberately small, curated collection (spec §1/§3.1F): adapt the hero
  // copy for any catalogue up to ~9 entries so scarcity reads as curation.
  const scarce = entries > 0 && entries <= 9;

  return (
    <div className="relative px-4 sm:px-7 py-6 border-b border-ink-rule">
      {/* Subtle paper texture — spec §2.3 (hero + detail surfaces get a low-opacity
          GrainOverlay). Sits behind the content (pointer-events-none, zIndex 0). */}
      <GrainOverlay />
      <div className="relative flex flex-col gap-5 sm:flex-row sm:justify-between sm:items-end sm:gap-6">
        <div className="min-w-0 max-w-[640px]">
          {/* Eyebrow */}
          <div className="flex items-center gap-2 flex-wrap">
            <span className="font-mono text-[11px] font-semibold tracking-[0.18em] uppercase text-gilt">
              XVISION · STRATEGY CATALOGUE · MANTLE TESTNET
            </span>
            {isDemo && (
              <span
                data-testid="demo-catalogue-marker"
                className="font-mono text-[10px] tracking-[0.12em] uppercase bg-gilt-bg text-gilt border border-gilt/30 rounded-[2px] px-1.5 py-0.5"
              >
                Demo catalogue
              </span>
            )}
          </div>

          {/* Headline */}
          <h1 className="m-0 mt-2 font-display text-[40px] font-medium leading-[1.04] tracking-[-0.02em]">
            The Catalogue
          </h1>

          {/* Body */}
          <p className="mt-2.5 text-[13.5px] font-sans text-text-2 leading-[1.55] max-w-[520px]">
            {scarce
              ? "A small, curated collection. Algorithmic trading strategies, minted as on-chain works. Inspect freely. Acquire selectively."
              : "Algorithmic trading strategies, minted as on-chain works. Inspect freely. Acquire selectively."}
          </p>

          {/* Honest stat ledger */}
          <div className="mt-3.5 font-mono text-[11.5px] text-text-3 flex items-center flex-wrap gap-x-0 gap-y-1 tracking-[0.04em]">
            {stats ? (
              <>
                <span className="uppercase">
                  <span className="text-text-3">Entries </span>
                  <span className="text-text-2">{fmt(entries)}</span>
                </span>
                <span className="mx-2.5 text-text-4">·</span>
                <span className="uppercase">
                  <span className="text-text-3">Creators </span>
                  <span className="text-text-2">{fmt(distinctCreators)}</span>
                </span>
                <span className="mx-2.5 text-text-4">·</span>
                <span className="uppercase">
                  <span className="text-text-3">Paid to creators </span>
                  {/* No real backing total in this build — honest em-dash. */}
                  <span className="text-text-2">—</span>
                </span>
              </>
            ) : (
              <span className="text-text-4">Loading…</span>
            )}
          </div>
        </div>

        {/* Actions */}
        <div className="flex gap-2 items-center shrink-0 flex-wrap">
          <TestnetBadge size="sm" />
          <Link
            to="/marketplace/wallet"
            aria-label="wallet"
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-[3px] border border-border-strong bg-transparent text-text-2 text-[12px] font-medium hover:text-text hover:border-border"
          >
            Wallet
          </Link>
          <button
            type="button"
            aria-label="list your strategy"
            onClick={() => navigate("/marketplace/sell")}
            className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-[3px] bg-gold text-[#001A0A] text-[12px] font-bold hover:opacity-90 transition-opacity"
          >
            List your strategy →
          </button>
        </div>
      </div>
    </div>
  );
}
