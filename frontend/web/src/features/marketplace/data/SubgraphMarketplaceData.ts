// src/features/marketplace/data/SubgraphMarketplaceData.ts
//
// Real MarketplaceData backed by the marketplace subgraph (Goldsky — runbook
// §3.2 / C7), composed with a fallback delegate for everything the subgraph
// does not own.
//
// Subgraph-backed (on-chain truth — renders REAL listings):
//   listListings · getListing · getLeaderboard · getStats
//
// Delegated to `fallback` (off-chain / operator-local / write surface — not in
// the index, and several still deploy-gated):
//   getSlices            curated slice defs (static)
//   getCreator           rich profile (forest/earnings) needs eval+manifest
//   getReceipt           needs licenseTokenId/bundleCid (post-purchase; §3.4)
//   getViewer            wallet state (§3.4, wallet signer deferred)
//   listListableStrategies / createPublishDraft   operator-local (dashboard/CLI)
//   submitListing / purchaseIntent / cloneIntent  on-chain WRITES (driver; §3.4)
//   subscribePurchases   live feed (could poll sales later)
//
// Display metrics (sharpe/return/equity) and name/model come from the eval and
// manifest seams; until those land the mappers emit honest zero/empty values
// (see ./subgraph/map.ts), so real listings render with real price/tier/owner/
// provenance but placeholder analytics.

import { applyFilter, defaultFilterState } from "./filter";
import { FixtureMarketplaceData } from "./MarketplaceData";
import type { MarketplaceData } from "./MarketplaceData";
import { SLICES } from "./fixtures/slices";
import {
  Q_LISTING,
  Q_LISTINGS,
  Q_STATS,
  type SgListing,
  type SubgraphClient,
} from "./subgraph/client";
import {
  mapListingDetail,
  mapListingRow,
  mapStats,
  nullManifestResolver,
  type ManifestResolver,
  type SgListingFull,
} from "./subgraph/map";
import type {
  CreatorProfile,
  FilterState,
  Id,
  ListableStrategy,
  ListingDetail,
  ListingRow,
  MarketplaceStats,
  PublishDraft,
  PurchaseEvent,
  Receipt,
  Slice,
  SliceId,
  TxRef,
  Viewer,
} from "./types";

const LISTINGS_CAP = 1000;

export interface SubgraphMarketplaceDataOpts {
  client: SubgraphClient;
  /** Delegate for non-subgraph methods. Defaults to the fixture client. */
  fallback?: MarketplaceData;
  /** Off-chain metadata resolver (manifest CID → name/model/...). */
  manifest?: ManifestResolver;
  /** Injectable clock (seconds) for stats windows; defaults to Date.now. */
  nowSecs?: () => number;
}

export class SubgraphMarketplaceData implements MarketplaceData {
  private readonly client: SubgraphClient;
  private readonly fallback: MarketplaceData;
  private readonly manifest: ManifestResolver;
  private readonly nowSecs: () => number;

  constructor(opts: SubgraphMarketplaceDataOpts) {
    this.client = opts.client;
    this.fallback = opts.fallback ?? new FixtureMarketplaceData();
    this.manifest = opts.manifest ?? nullManifestResolver;
    this.nowSecs = opts.nowSecs ?? (() => Math.floor(Date.now() / 1000));
  }

  // --- subgraph-backed reads ---------------------------------------------

  async listListings(f: FilterState) {
    const { listings } = await this.client.query<{ listings: SgListing[] }>(
      Q_LISTINGS,
      { first: LISTINGS_CAP },
    );
    const rows = await this.rowsFrom(listings);
    return applyFilter(rows, f);
  }

  async getListing(idOrName: string): Promise<ListingDetail> {
    const { listing } = await this.client.query<{
      listing: SgListingFull | null;
    }>(Q_LISTING, { id: idOrName });
    if (!listing) throw new Error(`listing not found: ${idOrName}`);
    const meta = await this.manifest.resolve(listing.agent.manifestCid);
    return mapListingDetail(listing, meta);
  }

  async getLeaderboard(sliceId: SliceId): Promise<{ slice: Slice; rows: ListingRow[] }> {
    const slice = SLICES.find((s) => s.id === sliceId);
    if (!slice) throw new Error(`slice not found: ${sliceId}`);
    const { listings } = await this.client.query<{ listings: SgListing[] }>(
      Q_LISTINGS,
      { first: LISTINGS_CAP },
    );
    const rows = await this.rowsFrom(listings);
    const filtered = applyFilter(rows, {
      ...defaultFilterState(),
      ...slice.filter,
    } as FilterState);
    return { slice, rows: filtered.rows };
  }

  async getStats(): Promise<MarketplaceStats> {
    const r = await this.client.query<Parameters<typeof mapStats>[0]>(Q_STATS, {
      cap: LISTINGS_CAP,
    });
    return mapStats(r, this.nowSecs());
  }

  // --- delegated (off-chain / operator-local / write) --------------------

  getSlices(): Promise<Slice[]> {
    return this.fallback.getSlices();
  }
  getCreator(handleOrAddress: string): Promise<CreatorProfile> {
    return this.fallback.getCreator(handleOrAddress);
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
  submitListing(d: PublishDraft): Promise<TxRef> {
    return this.fallback.submitListing(d);
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

  // --- helpers -----------------------------------------------------------

  private async rowsFrom(listings: SgListing[]): Promise<ListingRow[]> {
    return Promise.all(
      listings.map(async (l) => {
        const meta = await this.manifest.resolve(l.agent.manifestCid);
        return mapListingRow(l, meta);
      }),
    );
  }
}
