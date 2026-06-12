// src/features/marketplace/data/MarketplaceData.ts
import { applyFilter, defaultFilterState } from "./filter";
import { ALL_LISTINGS, LISTING_DETAILS } from "./fixtures/listings";
import { CREATORS } from "./fixtures/creators";
import { SLICES } from "./fixtures/slices";
import { RECEIPTS } from "./fixtures/receipts";
import { LISTABLE_STRATEGIES, buildPublishDraft } from "./fixtures/seller";
import { VIEWER } from "./fixtures/viewer";
import { publishListing } from "./publish";
import type {
  CreatorProfile, FilterState, Id, ListableStrategy, ListingDetail, ListingRow,
  MarketplaceStats, PublishDraft, PurchaseEvent, Receipt, Slice, SliceId, TxRef, Viewer,
} from "./types";

export interface MarketplaceData {
  // W1-FOUNDATION: `dataSource` is REQUIRED on the interface.
  // W2-DATA MUST add `readonly dataSource = "api"` to ApiMarketplaceData and
  // `readonly dataSource = "subgraph"` to SubgraphMarketplaceData, or tsc fails.
  readonly dataSource: "fixture" | "api" | "subgraph";
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
  // DEPLOY WALL (C7 / AM6 + signer): `purchaseIntent` is the seam for the real
  // on-chain EIP-3009 `buyWithAuthorization`. The fixture impl returns a fake
  // TxRef; the live impl (swapped in at MarketplaceLayout once contracts are
  // deployed and `useWallet` exposes a signer) signs the authorization and
  // submits it here. Callers MUST treat this as testnet/simulated until then.
  purchaseIntent(listingId: Id): Promise<TxRef>;
  cloneIntent(listingId: Id): Promise<TxRef>;
  subscribePurchases(cb: (e: PurchaseEvent) => void): () => void;
}

const fakeTx = (): TxRef => ({
  txHash: `0x${Math.random().toString(16).slice(2).padEnd(8, "0")}`,
  network: "mantle-sepolia",
});

export class FixtureMarketplaceData implements MarketplaceData {
  readonly dataSource = "fixture" as const;
  async getStats(): Promise<MarketplaceStats> {
    // ENTRIES must equal the number of rows actually in the catalogue — a
    // hard-coded 1,247 against ~206 visible rows reads as an internal
    // contradiction even inside a DEMO CATALOGUE (QA fix). Derive from the
    // same source the rows come from so the stat ledger and the row count agree.
    return { totalStrategies: ALL_LISTINGS.length, paidThisWeekUsd: 34820, agentPurchases: 218, mintedLast24h: 64 };
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
    const { rows } = applyFilter(ALL_LISTINGS, { ...defaultFilterState(), ...slice.filter } as FilterState);
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
  async submitListing(d: PublishDraft): Promise<TxRef> {
    return publishListing(d);
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

