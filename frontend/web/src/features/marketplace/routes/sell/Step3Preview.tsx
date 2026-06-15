// src/features/marketplace/routes/sell/Step3Preview.tsx
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { PublishDraft } from "@/features/marketplace/data/types";
import { getStrategy } from "@/api/strategies";
import { TestnetBadge } from "@/features/marketplace/components/TestnetBadge";
import { useMarketplaceNetwork } from "@/features/marketplace/lib/useMarketplaceNetwork";
import { ListingPreviewCard } from "./ListingPreviewCard";

/** What the operator wants published as the listing's public description.
 *  `dirty` is true only when the textarea diverges from the stored
 *  `manifest.plain_summary` — the caller PATCHes the strategy before
 *  submitting the listing so the server-side manifest hash includes it. */
export interface PublicDescription {
  value: string;
  dirty: boolean;
}

export function Step3Preview({
  draft,
  onMint,
  onBack,
  minting,
}: {
  draft: PublishDraft;
  onMint: (description: PublicDescription) => void;
  onBack?: () => void;
  minting: boolean;
}) {
  const allPass = draft.listable.every((c) => c.ok);
  const mintDisabled = !allPass || minting;
  const { isMainnet: mainnet } = useMarketplaceNetwork();
  // Bind the preview to the LIVE draft (name/price/tier the seller edited in
  // step 2) rather than the snapshot captured when the draft was created — so
  // the card never shows a stale name or the default 49 USDC.
  const livePreview = {
    ...draft.preview,
    name: draft.name,
    priceUsdc: draft.priceUsdc,
    tier: draft.tier,
  };
  const networkLabel = mainnet ? "Mantle mainnet" : "the Mantle Sepolia testnet";

  // Prefill from the stored strategy's manifest.plain_summary. Errors (local
  // engine unreachable, unknown id) leave the textarea empty but editable.
  const { data: strategy } = useQuery({
    queryKey: ["strategy", draft.strategyId, "publish-preview"],
    queryFn: () => getStrategy(draft.strategyId),
    retry: false,
  });
  const storedSummary = strategy?.manifest.plain_summary ?? "";
  // null until the operator touches the field — so a late-arriving fetch
  // can still prefill without clobbering edits.
  const [edited, setEdited] = useState<string | null>(null);
  const description = edited ?? storedSummary;
  const dirty = edited !== null && edited !== storedSummary;

  return (
    <div data-testid="sell-step-3-body" className="flex flex-col gap-5">

      {/* Ingredients summary */}
      <div>
        <p className="text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2">
          Ingredients in bundle
        </p>
        {draft.ingredients.length === 0 ? (
          <p className="text-[12px] text-text-3">
            No bundle ingredients detected for this strategy.
          </p>
        ) : (
          <ul className="flex flex-col gap-1">
            {draft.ingredients.map((ing, i) => (
              <li key={i} className="flex items-center gap-2 text-[12px]">
                <span
                  className={`w-1.5 h-1.5 rounded-full ${ing.installed ? "bg-gold" : "bg-warn"}`}
                  aria-hidden="true"
                />
                <span className={ing.installed ? "text-text-2" : "text-warn"}>{ing.name}</span>
                <span className="text-text-3 text-[10px] font-mono uppercase ml-1">{ing.kind}</span>
              </li>
            ))}
          </ul>
        )}
      </div>

      {/* Public description — saved to the strategy before the listing is
          submitted so the published manifest carries it */}
      <div>
        <label
          htmlFor="public-description"
          className="block text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2"
        >
          Public description
        </label>
        <textarea
          id="public-description"
          data-testid="public-description"
          value={description}
          onChange={(e) => setEdited(e.target.value)}
          rows={3}
          placeholder="What this strategy does, in plain English."
          className="w-full px-3 py-2 bg-surface-elev border border-border rounded-md text-[13px] text-text leading-[1.45] focus:border-gold/60 focus:outline-none resize-y"
        />
        <p className="mt-1 text-[11px] text-text-3">
          Published publicly to IPFS with your strategy — buyers and
          non-buyers can read it.
        </p>
      </div>

      {/* Preview card */}
      <div>
        <p className="text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2">
          Listing preview
        </p>
        <ListingPreviewCard listing={livePreview} />
      </div>

      {/* Failed checks warning (when minting is blocked) */}
      {!allPass && (
        <div className="flex items-start gap-2 px-4 py-3 rounded-md bg-danger/10 border border-danger/30">
          <svg
            width="14"
            height="14"
            viewBox="0 0 14 14"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.6"
            className="text-danger mt-0.5 shrink-0"
            aria-hidden="true"
          >
            <circle cx="7" cy="7" r="5.5" />
            <path d="M7 4.5v3M7 9.5v.2" strokeLinecap="round" />
          </svg>
          <p className="text-[12px] text-danger">
            Mint is disabled — resolve listability failures in step 2 before minting.
          </p>
        </div>
      )}

      {/* Mint action */}
      <div className="flex flex-wrap items-center gap-4">
        <button
          onClick={() => onMint({ value: description, dirty })}
          disabled={mintDisabled}
          className={`px-4 py-2 rounded-md text-[13px] font-medium flex items-center gap-2 motion-safe:active:scale-[0.96] ${
            mintDisabled
              ? "bg-surface-elev border border-border text-text-3 cursor-not-allowed"
              : "bg-gold text-black hover:bg-gold/90"
          }`}
          aria-label={
            minting
              ? "Minting…"
              : mainnet
                ? "Mint on Mantle mainnet"
                : "Mint on testnet"
          }
        >
          {minting ? "Minting…" : "Mint"}
          {!minting ? <TestnetBadge /> : null}
        </button>
        <button
          type="button"
          onClick={onBack}
          disabled={minting}
          className="text-[12px] text-text-3 hover:text-text-2 disabled:opacity-50"
        >
          ← Back
        </button>
        <p className="text-[11px] text-text-3">
          Submits listing to {networkLabel} · one-time fee
        </p>
      </div>
    </div>
  );
}
