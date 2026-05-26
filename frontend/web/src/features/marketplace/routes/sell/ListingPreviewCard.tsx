// src/features/marketplace/routes/sell/ListingPreviewCard.tsx
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { X402Badge } from "@/features/marketplace/components/X402Badge";
import type { ListingRow } from "@/features/marketplace/data/types";

export function ListingPreviewCard({ listing }: { listing: ListingRow }) {
  return (
    <div
      data-preview="listing"
      className="flex gap-4 p-4 rounded-md bg-surface-elev border border-border"
    >
      <GenArtPlaceholder seed={listing.genArtSeed} size={56} className="shrink-0 rounded-md" />

      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 flex-wrap mb-0.5">
          <span className="text-[14px] font-medium font-mono truncate">{listing.id}</span>
          <span className="text-[11px] text-text-3 font-mono">{listing.version}</span>
          {listing.verification === "verified" && <VerifiedBadge />}
          {listing.acceptsX402 && <X402Badge />}
        </div>

        <p className="text-[12px] text-text-3 mb-1.5">
          {listing.creator.handle ?? listing.creator.address} · {listing.model} · {listing.style}
        </p>

        <div className="flex items-center gap-2 flex-wrap">
          {listing.assets.map((a) => (
            <AssetPill key={a} asset={a} />
          ))}
          {listing.assets.length === 0 && (
            <span className="text-[11px] text-danger italic">No assets configured</span>
          )}
        </div>
      </div>

      <div className="shrink-0 text-right">
        {listing.priceUsdc === null ? (
          <span className="text-[12px] font-medium text-gold">● OPEN</span>
        ) : (
          <span className="text-[13px] font-mono font-medium text-text">
            {listing.priceUsdc} <span className="text-text-3">USDC</span>
          </span>
        )}
        <p className="text-[11px] text-text-3 mt-0.5">
          {listing.tier === "open" ? "Tier A" : "Tier B"}
        </p>
      </div>
    </div>
  );
}
