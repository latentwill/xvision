// ApiMarketplaceData.ts — real indexer-backed reads with fixture fallback.
//
// Reads come from the backend marketplace indexer (`/api/marketplace/*`);
// everything the indexer can't answer yet (slices, creators, receipts,
// viewer, drafts, purchase intents) delegates to the wrapped fixture client.
// Metrics/social fields the chain doesn't carry are zeroed honestly rather
// than faked.
import { apiFetch } from "@/api/client";
import { FixtureMarketplaceData, type MarketplaceData } from "./MarketplaceData";
import { applyFilter } from "./filter";
import { publishListing } from "./publish";
import type {
  CreatorProfile, FilterState, Id, ListableStrategy, ListingDetail, ListingRow,
  MarketplaceStats, PublishDraft, PurchaseEvent, Receipt, Slice, SliceId, TxRef, Viewer,
} from "./types";

// Backend shapes (see crates' marketplace indexer routes).
export interface IndexedListing {
  listing_id: number;
  agent_nft_id: string;
  agent_id: string;
  seller: string;
  content_hash: string;
  content_uri: string;
  tier: number; // 0 open | 1 sealed
  price_usdc: number;
  transferable_license: boolean;
  revoked: boolean;
  gen_art_seed: string;
  name: string;
  symmetry: string;
  palette: string;
}

export interface MarketplaceIndexStatus {
  active: boolean;
  last_poll_unix: number;
  total_onchain: number;
  last_error: string | null;
}

function toRow(l: IndexedListing): ListingRow {
  return {
    id: String(l.listing_id),
    lineageId: l.agent_id || String(l.listing_id),
    version: "v1",
    creator: { address: l.seller },
    model: "",
    style: l.symmetry,
    assets: [],
    return30dPct: 0,
    sharpe: 0,
    buyers: { humans: 0, agents: 0 },
    priceUsdc: l.price_usdc > 0 ? l.price_usdc : null,
    tier: l.tier === 1 ? "sealed" : "open",
    transferableLicense: l.transferable_license,
    verification: "unverified",
    acceptsX402: true,
    clones: 0,
    genArtSeed: l.gen_art_seed,
  };
}

function toDetail(l: IndexedListing): ListingDetail {
  return {
    ...toRow(l),
    // Chain metadata name is the only human-readable copy we have; it renders
    // in the promise slot under the title.
    promise: l.name,
    metrics: { return30dPct: 0, sharpe: 0, winRatePct: 0, maxDrawdownPct: 0, avgDurationDays: 0 },
    paidToCreatorUsd: 0,
    platformFeeBps: 0,
    ingredients: [],
    variants: [],
    recentBuyers: [],
    creatorOther: [],
    equityCurve: { base: 100, points: [] },
    whatYouGet: [],
    whatYouDont: [],
    onChain: {
      nft: {
        tokenId: l.agent_nft_id,
        lineageId: l.agent_id || String(l.listing_id),
        agentURI: l.content_uri,
        manifestHash: l.content_hash,
        parentLineage: null,
        bornAt: "",
        operatorSig: "",
        contract: "",
        network: "mantle-sepolia",
      },
      attestations: [],
      anchors: [],
      trades: [],
      tradesMeta: {
        totalOnChain: 0,
        lastAnchorAt: "",
        receiptKind: "",
        netPnlUsd: 0,
        window: "",
        anchorTx: "",
      },
    },
  };
}

export class ApiMarketplaceData implements MarketplaceData {
  constructor(private fallback: MarketplaceData) {}

  async listListings(f: FilterState) {
    const out = await apiFetch<{ items: IndexedListing[]; total: number }>(
      "/api/marketplace/listings",
    );
    return applyFilter(out.items.map(toRow), f);
  }

  async getListing(idOrName: string): Promise<ListingDetail> {
    try {
      const l = await apiFetch<IndexedListing>(
        `/api/marketplace/listings/${encodeURIComponent(idOrName)}`,
      );
      return toDetail(l);
    } catch {
      // Unknown on-chain id (404) or indexer unreachable — fixture detail
      // pages (slug ids) keep working.
      return this.fallback.getListing(idOrName);
    }
  }

  async getStats(): Promise<MarketplaceStats> {
    const out = await apiFetch<{ items: IndexedListing[]; total: number }>(
      "/api/marketplace/listings",
    );
    return { totalStrategies: out.total, paidThisWeekUsd: 0, agentPurchases: 0, mintedLast24h: 0 };
  }

  async submitListing(d: PublishDraft): Promise<TxRef> {
    return publishListing(d);
  }

  // ——— everything else delegates to the fixture client ———
  getSlices(): Promise<Slice[]> {
    return this.fallback.getSlices();
  }
  getCreator(handleOrAddress: string): Promise<CreatorProfile> {
    return this.fallback.getCreator(handleOrAddress);
  }
  getLeaderboard(sliceId: SliceId): Promise<{ slice: Slice; rows: ListingRow[] }> {
    return this.fallback.getLeaderboard(sliceId);
  }
  getReceipt(txHash: string): Promise<Receipt> {
    return this.fallback.getReceipt(txHash);
  }
  getViewer(): Promise<Viewer> {
    return this.fallback.getViewer();
  }
  listListableStrategies(): Promise<ListableStrategy[]> {
    return this.fallback.listListableStrategies();
  }
  createPublishDraft(strategyId: string): Promise<PublishDraft> {
    return this.fallback.createPublishDraft(strategyId);
  }
  purchaseIntent(listingId: Id): Promise<TxRef> {
    return this.fallback.purchaseIntent(listingId);
  }
  cloneIntent(listingId: Id): Promise<TxRef> {
    return this.fallback.cloneIntent(listingId);
  }
  subscribePurchases(cb: (e: PurchaseEvent) => void): () => void {
    return this.fallback.subscribePurchases(cb);
  }
}

/// Pick the marketplace client based on the indexer status endpoint.
/// Never rejects: any fetch failure (indexer absent, jsdom, network down)
/// resolves to the fixture fallback so callers can `.then(setClient)` safely.
export async function chooseMarketplaceData(
  fallback: MarketplaceData = new FixtureMarketplaceData(),
): Promise<MarketplaceData> {
  try {
    const status = await apiFetch<MarketplaceIndexStatus>("/api/marketplace/status");
    if (status.active === true) return new ApiMarketplaceData(fallback);
  } catch {
    // indexer not running / not reachable → fixtures
  }
  return fallback;
}
