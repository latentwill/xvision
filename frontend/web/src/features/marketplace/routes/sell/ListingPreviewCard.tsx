// src/features/marketplace/routes/sell/ListingPreviewCard.tsx
// Plain app-native listing preview — seller sees exactly the entry they are minting.
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { humanize } from "@/features/marketplace/routes/browse/ListingEntry";
import type { ListingRow } from "@/features/marketplace/data/types";

export function ListingPreviewCard({ listing }: { listing: ListingRow }) {
  const displayTitle = (listing as ListingRow & { name?: string }).name ?? humanize(listing.id);
  const tierLabel = listing.tier === "open" ? "Open edition" : "Sealed";

  return (
    <div
      data-preview="listing"
      className="grid gap-6 py-5 border border-border rounded-md px-5"
      style={{ gridTemplateColumns: "120px 1fr auto" }}
    >
      {/* Zone A: art */}
      <div className="flex flex-col items-start gap-1.5">
        <GenArtPlaceholder seed={listing.genArtSeed} size={104} />
        <span className="text-[11px] font-mono text-text-3">
          <span data-listing-id>{String(listing.id)}</span>
        </span>
      </div>

      {/* Zone B: info */}
      <div className="flex flex-col gap-1 min-w-0">
        {/* Line 1 — title */}
        <div className="flex items-center gap-1.5 flex-wrap">
          <span
            className="text-[15px] font-medium leading-[1.15] tracking-[-0.01em] line-clamp-2 text-text"
            title={displayTitle}
          >
            {displayTitle}
          </span>
          {listing.verification === "verified" && <VerifiedBadge />}
          <span className="text-[11px] font-mono text-text-3">{listing.version}</span>
        </div>

        {/* Line 2 — provenance caption */}
        <p className="text-[11.5px] font-mono text-text-2 leading-[1.4]">
          {listing.creator.handle ?? listing.creator.address.slice(0, 10) + "…"}
          {" · "}
          {tierLabel}
          {listing.assets.length > 0 && (
            <>
              {" · "}
              <span className="inline-flex gap-1 flex-wrap">
                {listing.assets.map((a) => (
                  <AssetPill key={a} asset={a} />
                ))}
              </span>
            </>
          )}
        </p>

        {/* Assets warning when empty */}
        {listing.assets.length === 0 && (
          <p className="text-[11px] text-danger italic">No assets configured</p>
        )}
      </div>

      {/* Zone C: acquisition */}
      <div className="shrink-0 flex flex-col items-end justify-start gap-1.5 text-right">
        {listing.priceUsdc === null ? (
          <span className="text-[11px] font-mono uppercase tracking-wide border border-border text-text-3 px-2 py-0.5 rounded-sm">
            Open Edition
          </span>
        ) : (
          <>
            <p className="text-[10px] font-mono uppercase tracking-[0.15em] text-text-3">PRICE</p>
            <p className="text-[13px] font-mono font-medium text-text tabular-nums">
              {listing.priceUsdc}{" "}
              <span className="text-text-3">USDC</span>
            </p>
          </>
        )}
      </div>
    </div>
  );
}
