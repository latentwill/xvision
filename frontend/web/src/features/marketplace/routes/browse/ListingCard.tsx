// src/features/marketplace/routes/browse/ListingCard.tsx
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { Sparkline } from "@/features/marketplace/components/Sparkline";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { X402Badge } from "@/features/marketplace/components/X402Badge";
import { TestnetBadge } from "@/features/marketplace/components/TestnetBadge";
import type { ListingRow } from "@/features/marketplace/data/types";

interface ListingCardProps {
  row: ListingRow;
  onBuy: (id: string) => void;
}

// The Buy CTA is a chain-bound action (purchaseIntent → TxRef with network,
// mantle-sepolia for fixture data). It is labeled [Testnet] via the shared
// TestnetBadge so every chain-bound affordance reads consistently.
export function ListingCard({ row, onBuy }: ListingCardProps) {
  const positive = row.return30dPct >= 0;
  const retSign = positive ? "+" : "";
  const isFree = row.priceUsdc === null || row.tier === "open";

  return (
    <div
      className="grid items-center gap-3.5 px-[22px] py-3 border-b border-[var(--border-soft,theme(colors.border))] cursor-pointer hover:bg-surface-elev/40 transition-colors"
      style={{
        gridTemplateColumns: "56px 1.8fr 0.75fr 1.1fr 1.05fr 0.6fr 0.85fr 110px",
      }}
    >
      {/* Gen-art thumb */}
      <div>
        <GenArtPlaceholder seed={row.genArtSeed} size={48} className="rounded-[4px]" />
      </div>

      {/* Name + version + badges + creator line */}
      <div className="min-w-0">
        <div className="flex items-center gap-1.5 flex-nowrap whitespace-nowrap overflow-hidden">
          <span className="font-mono text-[13px] text-text font-semibold truncate">
            {row.id}
          </span>
          <span className="font-mono text-[11px] text-text-3 shrink-0">{row.version}</span>
          {row.verification === "verified" && <VerifiedBadge />}
          {row.acceptsX402 && <X402Badge />}
        </div>
        <div className="flex items-center gap-2 mt-1 whitespace-nowrap overflow-hidden">
          <span className="font-mono text-[11px] text-text-2">{row.creator.handle ?? row.creator.address.slice(0, 8)}</span>
          <span className="text-text-4 text-[10px]">·</span>
          <span className="font-mono text-[10.5px] text-text-3 truncate">{row.model}</span>
          <span className="text-text-4 text-[10px]">·</span>
          <span className="font-mono text-[10.5px] text-text-3">{row.style}</span>
        </div>
      </div>

      {/* Asset pills */}
      <div className="flex gap-1 flex-wrap">
        {row.assets.map((a) => (
          <AssetPill key={a} asset={a} />
        ))}
      </div>

      {/* 30d return + sparkline */}
      <div className="flex items-center gap-2.5 justify-end">
        <span
          data-return-pct
          className={`font-mono text-[16px] font-semibold tracking-[-0.01em] ${positive ? "text-gold" : "text-danger"}`}
        >
          {row.return30dPct === 0 && row.sharpe === 0
            ? "—"
            : `${retSign}${row.return30dPct}%`}
        </span>
        <Sparkline seed={row.id} positive={positive} />
      </div>

      {/* Buyers: humans + agents */}
      <div className="flex items-center gap-2">
        <span className="font-mono text-[13px] text-text">{row.buyers.humans.toLocaleString()}</span>
        <span className="text-text-4 text-[10px]">·</span>
        <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-[3px] bg-gold/10 border border-gold/30">
          <AgentIcon size={10} className="text-gold" />
          <span className="font-mono text-[11px] text-gold font-semibold">{row.buyers.agents}</span>
        </span>
      </div>

      {/* Sharpe */}
      <div className="text-right">
        <span className="font-mono text-[12px] text-text-3">
          {row.return30dPct === 0 && row.sharpe === 0
            ? "—"
            : `${row.sharpe > 0 ? "+" : ""}${row.sharpe.toFixed(2)}`}
        </span>
      </div>

      {/* Price */}
      <div>
        {isFree ? (
          <span className="inline-flex items-center gap-1.5 px-2 py-1 border border-gold/30 bg-gold/10 rounded-[3px]">
            <span className="w-1.5 h-1.5 rounded-full bg-gold" />
            <span className="font-mono text-[10.5px] text-gold tracking-[0.14em] font-semibold">
              OPEN
            </span>
          </span>
        ) : (
          <span className="font-mono text-[13px] text-text">
            {row.priceUsdc} USDC
          </span>
        )}
      </div>

      {/* CTA */}
      <div className="flex flex-col items-start gap-1">
        <button
          type="button"
          aria-label={isFree ? "run free" : "buy"}
          onClick={(e) => {
            e.stopPropagation();
            onBuy(row.id);
          }}
          className="w-full px-3 py-1.5 rounded bg-gold text-[#001A0A] text-[12px] font-bold hover:opacity-90 transition-opacity motion-safe:active:scale-[0.96]"
        >
          {isFree ? "Run free" : "Buy"}
        </button>
        {/* [Testnet] badge — all chain-bound CTAs label the network */}
        <TestnetBadge />
      </div>
    </div>
  );
}
