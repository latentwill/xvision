// publish.ts — real publish path: backend mints the identity NFT with its
// genart tokenURI, then creates the listing. Throws on failure; no fake TXs.
//
// The `apiFetch` wrapper throws `ApiError` on any non-2xx response, so a 503
// (chain env unset) propagates up as a thrown error — callers must not catch
// it silently.
import { apiFetch } from "@/api/client";
import { activeNetworkSlug } from "../lib/chain";
import type { PublishDraft, TxRef } from "./types";

export interface PublishOut {
  agent_id: string;
  manifest_hash: string;
  token_id: string;
  listing_id: string;
  token_uri_bytes: number;
}

export async function publishListing(d: PublishDraft): Promise<TxRef> {
  const out = await apiFetch<PublishOut>("/api/marketplace/publish", {
    method: "POST",
    body: JSON.stringify({
      strategy_id: d.strategyId,
      // Tier type in types.ts is "open" | "sealed" — same literals the backend expects.
      tier: d.tier,
      price_usdc: d.priceUsdc ?? 0,
      transferable_license: false,
      // Creator-chosen listing name (defaults to the strategy's display name).
      // The backend stores it on the publish receipt so the listing inherits a
      // real name instead of rendering a generic "Strategy #N".
      name: d.name,
    }),
  });
  // TxRef: { txHash: string; network: string }
  // listing_id is the closest stable on-chain handle available at submit time;
  // the real tx hash is attached by the confirmation path once mined.
  return { txHash: out.listing_id, network: activeNetworkSlug };
}
