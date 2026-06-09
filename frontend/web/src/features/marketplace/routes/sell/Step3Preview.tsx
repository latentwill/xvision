// src/features/marketplace/routes/sell/Step3Preview.tsx
import type { PublishDraft } from "@/features/marketplace/data/types";
import { TestnetBadge } from "@/features/marketplace/components/TestnetBadge";
import { ListingPreviewCard } from "./ListingPreviewCard";

export function Step3Preview({
  draft,
  onMint,
  minting,
}: {
  draft: PublishDraft;
  onMint: () => void;
  minting: boolean;
}) {
  const allPass = draft.listable.every((c) => c.ok);
  const mintDisabled = !allPass || minting;

  return (
    <div data-testid="sell-step-3-body" className="flex flex-col gap-5">

      {/* Ingredients summary */}
      <div>
        <p className="text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2">
          Ingredients in bundle
        </p>
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
      </div>

      {/* Preview card */}
      <div>
        <p className="text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2">
          Listing preview
        </p>
        <ListingPreviewCard listing={draft.preview} />
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
      <div className="flex items-center gap-4">
        <button
          onClick={onMint}
          disabled={mintDisabled}
          className={`px-4 py-2 rounded-md text-[13px] font-medium flex items-center gap-2 motion-safe:active:scale-[0.96] ${
            mintDisabled
              ? "bg-surface-elev border border-border text-text-3 cursor-not-allowed"
              : "bg-gold text-black hover:bg-gold/90"
          }`}
          aria-label={minting ? "Minting…" : "Mint on testnet"}
        >
          {minting ? "Minting…" : "Mint"}
          {!minting ? <TestnetBadge /> : null}
        </button>
        <p className="text-[11px] text-text-3">
          Submits listing to the Mantle Sepolia testnet · one-time fee
        </p>
      </div>
    </div>
  );
}
