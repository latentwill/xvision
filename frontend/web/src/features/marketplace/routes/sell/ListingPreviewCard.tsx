// src/features/marketplace/routes/sell/ListingPreviewCard.tsx
// Catalogue-style preview — seller sees exactly the entry they are minting.
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { X402Badge } from "@/features/marketplace/components/X402Badge";
import type { ListingRow } from "@/features/marketplace/data/types";

/** Humanize a listing id for display — mirrors the logic in CatalogueEntry. */
function humanize(id: string | number): string {
  const s = String(id);
  // Numeric ids → "Strategy #<id>"
  if (/^\d+$/.test(s)) return `Strategy #${s}`;
  // Slug ids → Title Case, replacing hyphens/underscores with spaces
  return s
    .replace(/[-_]/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

export function ListingPreviewCard({ listing }: { listing: ListingRow }) {
  const displayTitle = (listing as ListingRow & { name?: string }).name ?? humanize(listing.id);
  const tierLabel = listing.tier === "open" ? "Open edition" : "Sealed";

  return (
    <div
      data-preview="listing"
      className="grid gap-6 py-5 border border-[var(--ink-rule)] rounded-[2px] px-5"
      style={{ gridTemplateColumns: "120px 1fr auto" }}
    >
      {/* Zone A: plate */}
      <div className="flex flex-col items-start gap-1.5">
        <div
          className="p-[3px] border-2 border-[var(--ink-rule)] ring-1 ring-gilt/15"
          style={{ display: "inline-block" }}
        >
          <GenArtPlaceholder seed={listing.genArtSeed} size={104} />
        </div>
        <span className="text-[12px] font-mono tracking-[0.1em] text-gilt">
          №&nbsp;<span data-listing-id>{String(listing.id).padStart(4, "0")}</span>
        </span>
      </div>

      {/* Zone B: caption */}
      <div className="flex flex-col gap-1 min-w-0">
        {/* Line 1 — title */}
        <div className="flex items-center gap-1.5 flex-wrap">
          <span
            className="text-[17px] font-display font-medium leading-[1.15] tracking-[-0.01em] line-clamp-2"
            title={displayTitle}
          >
            {displayTitle}
          </span>
          {listing.verification === "verified" && <VerifiedBadge />}
          {listing.acceptsX402 && <X402Badge />}
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
          <span className="text-[11px] font-mono uppercase tracking-wide border border-gilt/40 text-gilt px-2 py-0.5 rounded-[2px]">
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
