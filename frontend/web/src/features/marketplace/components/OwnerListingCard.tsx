// src/features/marketplace/components/OwnerListingCard.tsx
// Shared owner management card for a listing. Renders inline (no popups):
//   - status chip + price + tier label
//   - inline Edit Price control (number input + Save)
//   - Post attestation mini-form (inline expand)
//   - Republish content (inline confirm)
//   - Revoke (inline confirm)
// Used by WalletRoute's listing section and MyListingsRoute.
import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { apiFetch } from "@/api/client";
import { ApiError } from "@/api/client";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import type { IndexedListing } from "@/features/marketplace/data/ApiMarketplaceData";

function truncAddr(s: string): string {
  if (s.length <= 12) return s;
  return `${s.slice(0, 6)}…${s.slice(-4)}`;
}

export interface OwnerListingCardProps {
  listing: IndexedListing;
  /** Call this after any successful mutation to refetch the parent query. */
  onChanged: () => void;
  /**
   * When true, suppresses the redundant name/price/tier chips. Use this
   * when the card is embedded in a detail page that already shows those
   * fields prominently (e.g. LineageRoute owner strip).
   */
  omitMeta?: boolean;
}

export function OwnerListingCard({ listing, onChanged, omitMeta = false }: OwnerListingCardProps) {
  const mp = useMarketplaceData();

  // Revoke
  const [confirming, setConfirming] = useState(false);
  // Republish
  const [confirmingUpdate, setConfirmingUpdate] = useState(false);
  // Attestation
  const [attestOpen, setAttestOpen] = useState(false);
  const [cycles, setCycles] = useState("");
  const [sharpe, setSharpe] = useState("");
  // Edit price
  const [editingPrice, setEditingPrice] = useState(false);
  const [priceInput, setPriceInput] = useState(String(listing.price_usdc ?? 0));

  const revoke = useMutation({
    mutationFn: () =>
      apiFetch<{ listing_id: number; tx_hash: string }>(
        `/api/marketplace/listings/${listing.listing_id}/revoke`,
        { method: "POST" },
      ),
    onSuccess: () => { setConfirming(false); onChanged(); },
  });

  const update = useMutation({
    mutationFn: () =>
      apiFetch<{ listing_id: number; content_hash: string; content_uri: string; tx_hash: string }>(
        `/api/marketplace/listings/${listing.listing_id}/update`,
        { method: "POST" },
      ),
    onSuccess: () => { setConfirmingUpdate(false); onChanged(); },
  });

  const attest = useMutation({
    mutationFn: (body: { cycles: number; sharpe: number }) =>
      apiFetch<{ tx_hash: string }>(
        `/api/marketplace/listings/${listing.listing_id}/attest`,
        { method: "POST", body: JSON.stringify(body) },
      ),
    onSuccess: () => onChanged(),
  });

  const setPrice = useMutation({
    mutationFn: (priceUsdc: number) =>
      mp.setListingPrice(String(listing.listing_id), priceUsdc),
    onSuccess: () => { setEditingPrice(false); onChanged(); },
  });

  const cyclesNum = Number(cycles);
  const sharpeNum = Number(sharpe);
  const attestValid =
    cycles.trim() !== "" &&
    Number.isInteger(cyclesNum) &&
    cyclesNum > 0 &&
    sharpe.trim() !== "" &&
    Number.isFinite(sharpeNum);

  const priceNum = parseFloat(priceInput);
  const priceValid = priceInput.trim() !== "" && Number.isFinite(priceNum) && priceNum >= 0;

  const tierLabel = listing.tier === 1 ? "sealed" : "open";
  const price = listing.price_usdc > 0 ? `${listing.price_usdc} USDC` : "free";

  return (
    <div className="px-4 py-2.5 border-b border-border last:border-b-0">
      <div className="flex items-center gap-3 flex-wrap">
        {!omitMeta && <GenArtPlaceholder seed={listing.gen_art_seed} size={28} />}
        {!omitMeta && (
          <span className="font-mono text-[12px] text-text font-semibold min-w-0 truncate">
            {listing.name || listing.agent_id}
          </span>
        )}
        {!omitMeta && <span className="font-mono text-[11px] text-gold">{price}</span>}
        {!omitMeta && (
          <span className="font-mono text-[10px] px-1.5 py-0.5 border border-border rounded-[3px] text-text-2">
            {tierLabel}
          </span>
        )}
        {listing.units_sold > 0 && (
          <span className="font-mono text-[10px] px-1.5 py-0.5 border border-gold/40 rounded-[3px] text-gold">
            sold ×{listing.units_sold} · ${listing.earned_usdc.toFixed(2)} earned
          </span>
        )}
        {listing.revoked ? (
          <span className="font-mono text-[10px] px-1.5 py-0.5 border border-danger/40 rounded-[3px] text-danger ml-auto">
            revoked
          </span>
        ) : (
          <span className="ml-auto flex items-center gap-2 flex-wrap">
            <span className="font-mono text-[10px] px-1.5 py-0.5 border border-gold/40 rounded-[3px] text-gold">
              active
            </span>
            {/* Edit price inline control */}
            {editingPrice ? (
              <span className="inline-flex items-center gap-1.5">
                <input
                  type="number"
                  aria-label="price USDC"
                  placeholder="0"
                  min="0"
                  step="any"
                  value={priceInput}
                  onChange={(e) => setPriceInput(e.target.value)}
                  disabled={setPrice.isPending}
                  className="w-24 bg-transparent font-mono text-[11px] px-2 py-1 border border-border rounded-[3px] text-text focus:border-gold/50 outline-none disabled:opacity-50"
                />
                <span className="font-mono text-[10.5px] text-text-3">USDC</span>
                <button
                  type="button"
                  disabled={!priceValid || setPrice.isPending}
                  onClick={() => setPrice.mutate(priceNum)}
                  className="font-mono text-[11px] px-2 py-1 border border-gold/50 rounded-[3px] text-gold hover:bg-gold/10 transition-colors disabled:opacity-50"
                >
                  {setPrice.isPending ? "Saving…" : "Save"}
                </button>
                <button
                  type="button"
                  disabled={setPrice.isPending}
                  onClick={() => { setEditingPrice(false); setPrice.reset(); }}
                  className="font-mono text-[11px] px-2 py-1 border border-border rounded-[3px] text-text-2 hover:text-text transition-colors disabled:opacity-50"
                >
                  Cancel
                </button>
              </span>
            ) : (
              <button
                type="button"
                onClick={() => { setPriceInput(String(listing.price_usdc ?? 0)); setEditingPrice(true); }}
                className="font-mono text-[11px] px-2 py-1 border border-border rounded-[3px] text-text-2 hover:border-gold/50 hover:text-gold transition-colors"
              >
                Edit price
              </button>
            )}
            <button
              type="button"
              onClick={() => setAttestOpen((v) => !v)}
              className="font-mono text-[11px] px-2 py-1 border border-border rounded-[3px] text-text-2 hover:border-gold/50 hover:text-gold transition-colors"
            >
              Post attestation
            </button>
            {confirmingUpdate ? (
              <span className="inline-flex items-center gap-1.5">
                <span className="font-mono text-[11px] text-text-2">Confirm republish?</span>
                <button
                  type="button"
                  disabled={update.isPending}
                  onClick={() => update.mutate()}
                  className="font-mono text-[11px] px-2 py-1 border border-gold/50 rounded-[3px] text-gold hover:bg-gold/10 transition-colors disabled:opacity-50"
                >
                  {update.isPending ? "Republishing…" : "Yes"}
                </button>
                <button
                  type="button"
                  disabled={update.isPending}
                  onClick={() => { setConfirmingUpdate(false); update.reset(); }}
                  className="font-mono text-[11px] px-2 py-1 border border-border rounded-[3px] text-text-2 hover:text-text transition-colors disabled:opacity-50"
                >
                  Cancel
                </button>
              </span>
            ) : (
              <button
                type="button"
                onClick={() => setConfirmingUpdate(true)}
                className="font-mono text-[11px] px-2 py-1 border border-border rounded-[3px] text-text-2 hover:border-gold/50 hover:text-gold transition-colors"
              >
                Republish content
              </button>
            )}
            {confirming ? (
              <span className="inline-flex items-center gap-1.5">
                <span className="font-mono text-[11px] text-text-2">Confirm revoke?</span>
                <button
                  type="button"
                  disabled={revoke.isPending}
                  onClick={() => revoke.mutate()}
                  className="font-mono text-[11px] px-2 py-1 border border-danger/50 rounded-[3px] text-danger hover:bg-danger/10 transition-colors disabled:opacity-50"
                >
                  {revoke.isPending ? "Revoking…" : "Yes"}
                </button>
                <button
                  type="button"
                  disabled={revoke.isPending}
                  onClick={() => { setConfirming(false); revoke.reset(); }}
                  className="font-mono text-[11px] px-2 py-1 border border-border rounded-[3px] text-text-2 hover:text-text transition-colors disabled:opacity-50"
                >
                  Cancel
                </button>
              </span>
            ) : (
              <button
                type="button"
                onClick={() => setConfirming(true)}
                className="font-mono text-[11px] px-2 py-1 border border-border rounded-[3px] text-text-2 hover:border-danger/50 hover:text-danger transition-colors"
              >
                Revoke
              </button>
            )}
          </span>
        )}
      </div>
      {/* Inline errors */}
      {revoke.isError && (
        <div className="font-mono text-[11px] text-danger mt-1.5">
          Revoke failed: {revoke.error instanceof Error ? revoke.error.message : "unknown error"}
        </div>
      )}
      {update.isError && (
        <div className="font-mono text-[11px] text-danger mt-1.5">
          Republish failed: {update.error instanceof Error ? update.error.message : "unknown error"}
        </div>
      )}
      {update.isSuccess && (
        <div className="font-mono text-[11px] text-gold mt-1.5">
          republished → {update.data.content_uri} · tx {truncAddr(update.data.tx_hash)}
        </div>
      )}
      {setPrice.isError && (
        <div className="font-mono text-[11px] text-danger mt-1.5">
          {setPrice.error instanceof ApiError && setPrice.error.status === 400
            ? "Only the listing owner can change the price."
            : setPrice.error instanceof Error
              ? setPrice.error.message
              : "Price update failed."}
        </div>
      )}
      {setPrice.isSuccess && (
        <div className="font-mono text-[11px] text-gold mt-1.5">
          Price updated · tx {truncAddr(setPrice.data.txHash)}
        </div>
      )}
      {/* Inline attestation mini-form */}
      {attestOpen && !listing.revoked && (
        <div className="mt-2 border border-border rounded-[4px] px-3 py-2 flex items-center gap-2 flex-wrap">
          <span className="font-mono text-[10.5px] text-text-3">eval attestation</span>
          <input
            type="number"
            aria-label="cycles"
            placeholder="cycles"
            value={cycles}
            onChange={(e) => setCycles(e.target.value)}
            disabled={attest.isPending}
            className="w-20 bg-transparent font-mono text-[11px] px-2 py-1 border border-border rounded-[3px] text-text focus:border-gold/50 outline-none disabled:opacity-50"
          />
          <input
            type="number"
            step="any"
            aria-label="sharpe"
            placeholder="sharpe"
            value={sharpe}
            onChange={(e) => setSharpe(e.target.value)}
            disabled={attest.isPending}
            className="w-20 bg-transparent font-mono text-[11px] px-2 py-1 border border-border rounded-[3px] text-text focus:border-gold/50 outline-none disabled:opacity-50"
          />
          <button
            type="button"
            disabled={!attestValid || attest.isPending}
            onClick={() => attest.mutate({ cycles: cyclesNum, sharpe: sharpeNum })}
            className="font-mono text-[11px] px-2 py-1 border border-gold/50 rounded-[3px] text-gold hover:bg-gold/10 transition-colors disabled:opacity-50"
          >
            {attest.isPending ? "Posting…" : "Attest"}
          </button>
          {attest.isSuccess && (
            <span className="font-mono text-[10.5px] text-gold">
              attested · tx {truncAddr(attest.data.tx_hash)}
            </span>
          )}
          {attest.isError && (
            <span className="font-mono text-[10.5px] text-danger">
              Attestation failed: {attest.error instanceof Error ? attest.error.message : "unknown error"}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
