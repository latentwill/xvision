// src/features/marketplace/data/MarketplaceData.ts
import { applyFilter } from "./filter";
import { ALL_LISTINGS, LISTING_DETAILS } from "./fixtures/listings";
import { CREATORS } from "./fixtures/creators";
import { SLICES } from "./fixtures/slices";
import { RECEIPTS } from "./fixtures/receipts";
import { LISTABLE_STRATEGIES, buildPublishDraft } from "./fixtures/seller";
import { VIEWER } from "./fixtures/viewer";
import type {
  CreatorProfile, FilterState, Id, ListableStrategy, ListingDetail, ListingRow,
  MarketplaceStats, PublishDraft, PurchaseEvent, Receipt, Slice, SliceId, TxRef, Viewer,
} from "./types";

export interface MarketplaceData {
  getStats(): Promise<MarketplaceStats>;
  listListings(f: FilterState): Promise<{ rows: ListingRow[]; total: number; matched: number }>;
  getSlices(): Promise<Slice[]>;
  getListing(idOrName: string): Promise<ListingDetail>;
  getCreator(handleOrAddress: string): Promise<CreatorProfile>;
  getLeaderboard(sliceId: SliceId): Promise<{ slice: Slice; rows: ListingRow[] }>;
  getReceipt(txHash: string): Promise<Receipt>;
  getViewer(): Promise<Viewer>;
  listListableStrategies(): Promise<ListableStrategy[]>;
  createPublishDraft(strategyId: string): Promise<PublishDraft>;
  submitListing(d: PublishDraft): Promise<TxRef>;
  purchaseIntent(listingId: Id): Promise<TxRef>;
  cloneIntent(listingId: Id): Promise<TxRef>;
  subscribePurchases(cb: (e: PurchaseEvent) => void): () => void;
}

const fakeTx = (): TxRef => ({
  txHash: `0x${Math.random().toString(16).slice(2).padEnd(8, "0")}`,
  network: "mantle-sepolia",
});

export class FixtureMarketplaceData implements MarketplaceData {
  async getStats(): Promise<MarketplaceStats> {
    return { totalStrategies: 1247, paidThisWeekUsd: 34820, agentPurchases: 218, mintedLast24h: 64 };
  }
  async listListings(f: FilterState) {
    const pool =
      f.segment === "mine"
        ? ALL_LISTINGS.filter((r) => VIEWER.createdListingIds.includes(r.id))
        : ALL_LISTINGS;
    return applyFilter(pool, f);
  }
  async getSlices() {
    return SLICES;
  }
  async getListing(idOrName: string): Promise<ListingDetail> {
    const d = LISTING_DETAILS[idOrName];
    if (!d) throw new Error(`listing not found: ${idOrName}`);
    return d;
  }
  async getCreator(handleOrAddress: string): Promise<CreatorProfile> {
    const c = CREATORS[handleOrAddress];
    if (!c) throw new Error(`creator not found: ${handleOrAddress}`);
    return c;
  }
  async getLeaderboard(sliceId: SliceId) {
    const slice = SLICES.find((s) => s.id === sliceId);
    if (!slice) throw new Error(`slice not found: ${sliceId}`);
    const { rows } = applyFilter(ALL_LISTINGS, { ...baseFilter(), ...slice.filter } as FilterState);
    return { slice, rows };
  }
  async getReceipt(txHash: string): Promise<Receipt> {
    return RECEIPTS[txHash] ?? RECEIPTS["0xdemo-tx"];
  }
  async getViewer(): Promise<Viewer> {
    return VIEWER;
  }
  async listListableStrategies() {
    return LISTABLE_STRATEGIES;
  }
  async createPublishDraft(strategyId: string) {
    return buildPublishDraft(strategyId);
  }
  async submitListing(_d: PublishDraft): Promise<TxRef> {
    return fakeTx();
  }
  async purchaseIntent(_listingId: Id): Promise<TxRef> {
    return fakeTx();
  }
  async cloneIntent(_listingId: Id): Promise<TxRef> {
    return fakeTx();
  }
  subscribePurchases(cb: (e: PurchaseEvent) => void): () => void {
    const id = setInterval(() => {
      cb({
        listingId: "btc-momentum-v3", version: "v3.0", buyer: "0x7c2e…aa07",
        payerKind: Math.random() > 0.5 ? "agent" : "human", amountUsdc: 49, netToCreatorUsdc: 46.55,
        at: new Date().toISOString(),
      });
    }, 5000);
    return () => clearInterval(id);
  }
}

function baseFilter(): FilterState {
  return {
    segment: "trending", search: "", sort: "return30d", assets: [], models: [], styles: [],
    trust: { verifiedOnly: false, acceptsAgents: false, auditedOnly: false },
    priceUsdc: { from: 0, to: 500 }, minBuyers: 0,
  };
}
