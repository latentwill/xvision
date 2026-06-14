// src/features/marketplace/data/MarketplaceData.ts
import { applyFilter, defaultFilterState } from "./filter";
import { DEMO_LISTINGS, getDemoDetail } from "./fixtures/listings";
import { CREATORS } from "./fixtures/creators";
import { SLICES } from "./fixtures/slices";
import { RECEIPTS } from "./fixtures/receipts";
import { LISTABLE_STRATEGIES, buildPublishDraft } from "./fixtures/seller";
import { VIEWER } from "./fixtures/viewer";
import { fetchListableStrategies, fetchPublishDraft } from "./listable";
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
  // SEALED-tier finalize: decrypt the bundle (Lit-gated) and materialize the
  // referenced agents server-side, resolving to the new local strategy ULID.
  importSealed(listingId: Id): Promise<{ agent_id: string }>;
  // OPEN/free-tier finalize: import + materialize the referenced agents
  // server-side (no decrypt), resolving to the new local strategy ULID. The
  // open/free buy path imports for real — there is no fake-tx clone receipt.
  importListing(listingId: Id): Promise<{ agent_id: string }>;
  subscribePurchases(cb: (e: PurchaseEvent) => void): () => void;
}

const fakeTx = (): TxRef => ({
  txHash: `0x${Math.random().toString(16).slice(2).padEnd(8, "0")}`,
  network: "mantle-sepolia",
});

// Deterministic fake local strategy ULID for fixture imports — same listing id
// always yields the same agent_id so callers (and tests) can rely on a stable
// landing target without an on-chain materialize.
const fakeAgentId = (listingId: Id): string => `demo-agent-${listingId}`;

export class FixtureMarketplaceData implements MarketplaceData {
  readonly dataSource = "fixture" as const;
  async getStats(): Promise<MarketplaceStats> {
    // ENTRIES must equal the number of rows actually served. The demo serves
    // only the curated DEMO_LISTINGS (the small, deliberately-small collection)
    // — NOT the 200 at-scale wall-strat fixtures, which have no detail page and
    // would link to the designed not-found state. Deriving from the same source
    // the rows come from keeps the stat ledger and the row count in agreement
    // and the collection inspectable end-to-end.
    return { totalStrategies: DEMO_LISTINGS.length, paidThisWeekUsd: 34820, agentPurchases: 218, mintedLast24h: 64 };
  }
  async listListings(f: FilterState) {
    const pool =
      f.segment === "mine"
        ? DEMO_LISTINGS.filter((r) => VIEWER.createdListingIds.includes(r.id))
        : DEMO_LISTINGS;
    return applyFilter(pool, f);
  }
  async getSlices() {
    // Compute each slice's count live from the curated pool so the chip counts
    // are factually honest for the demo collection (no stale hardcoded 1,247).
    return SLICES.map((slice) => ({
      ...slice,
      count: applyFilter(
        DEMO_LISTINGS,
        { ...defaultFilterState(), ...slice.filter } as FilterState,
      ).matched,
    }));
  }
  async getListing(idOrName: string): Promise<ListingDetail> {
    // Resolve a demo detail for any curated listing — a hand-authored detail
    // when present, otherwise a synthesized one from the row so EVERY entry is
    // inspectable. Unknown ids surface the designed not-found state.
    const d = getDemoDetail(idOrName);
    if (!d) throw new Error(`listing not found: ${idOrName}`);
    return d;
  }
  async getCreator(handleOrAddress: string): Promise<CreatorProfile> {
    const c = CREATORS[handleOrAddress];
    if (!c) throw new Error(`creator not found: ${handleOrAddress}`);
    return c;
  }
  async getLeaderboard(sliceId: SliceId) {
    const def = SLICES.find((s) => s.id === sliceId);
    if (!def) throw new Error(`slice not found: ${sliceId}`);
    const { rows, matched } = applyFilter(DEMO_LISTINGS, { ...defaultFilterState(), ...def.filter } as FilterState);
    // Live count so the leaderboard header matches the rows it actually shows
    // (no stale hardcoded figure against the curated pool).
    const slice = { ...def, count: matched };
    return { slice, rows };
  }
  async getReceipt(txHash: string): Promise<Receipt> {
    return RECEIPTS[txHash] ?? RECEIPTS["0xdemo-tx"];
  }
  async getViewer(): Promise<Viewer> {
    return VIEWER;
  }
  async listListableStrategies() {
    // Listing your OWN strategies is operator-local — always served by the real
    // `/api/strategies`, never placeholder rows, even when the on-chain
    // marketplace indexer is inactive (so the sell picker is never gated behind
    // it). Falls back to the offline demo fixtures only when the strategies API
    // is genuinely unreachable (storybook / unit tests with no backend).
    try {
      return await fetchListableStrategies();
    } catch {
      return LISTABLE_STRATEGIES;
    }
  }
  async createPublishDraft(strategyId: string) {
    // Mirror listListableStrategies: build the draft from the operator's real
    // stored strategy, falling back to the fixture draft only when offline.
    try {
      return await fetchPublishDraft(strategyId);
    } catch {
      return buildPublishDraft(strategyId);
    }
  }
  async submitListing(d: PublishDraft): Promise<TxRef> {
    return publishListing(d);
  }
  async purchaseIntent(_listingId: Id): Promise<TxRef> {
    return fakeTx();
  }
  async importSealed(listingId: Id): Promise<{ agent_id: string }> {
    return { agent_id: fakeAgentId(listingId) };
  }
  async importListing(listingId: Id): Promise<{ agent_id: string }> {
    return { agent_id: fakeAgentId(listingId) };
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

