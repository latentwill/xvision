// ApiMarketplaceData.ts — real indexer-backed reads with fixture fallback.
//
// Reads come from the backend marketplace indexer (`/api/marketplace/*`);
// everything the indexer can't answer yet (slices, creators, receipts,
// viewer, drafts, purchase intents) delegates to the wrapped fixture client.
// Metrics/social fields the chain doesn't carry are zeroed honestly rather
// than faked.
import { ApiError, apiFetch } from "@/api/client";
import {
  fetchListableStrategies,
  fetchPublishDraft,
} from "./listable";
import {
  activeNetworkSlug,
  approveUsdc,
  buyDirect,
  currentAddress,
  ensureMantleSepolia,
  getActiveNetworkConfigOrDefault,
  getContracts,
  signTransferAuthorization,
  usdcBalance,
  type RelayAuthorization,
} from "../lib/chain";
import {
  InsufficientUsdcError,
  WalletRequiredError,
} from "../lib/purchaseErrors";
import { decryptSealedBundle } from "../lib/sealed";
import { FixtureMarketplaceData, type MarketplaceData } from "./MarketplaceData";
import { applyFilter, defaultFilterState } from "./filter";
import { SLICES } from "./fixtures/slices";
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
  /// On-chain attestation count from EvalAttestationRegistry (0 when the
  /// registry isn't configured on the server).
  attestation_count: number;
  /// Licenses sold, derived from `Sold` event logs (0 when the marketplace
  /// address isn't configured).
  units_sold: number;
  /// Sum of seller proceeds across `Sold` events, in whole USDC.
  earned_usdc: number;
  /// Best trailing-30d return % from completed eval runs (Part B).
  /// Absent when agent_id is empty or no completed runs exist.
  return30d_pct?: number | null;
  /// Best Sharpe ratio from completed eval runs (Part B).
  /// Absent when agent_id is empty or no completed runs exist.
  sharpe?: number | null;
}

/** Mirrors `ReceiptOut` in marketplace_read.rs. */
export interface ReceiptOut {
  tx_hash: string;
  listing_id: number;
  agent_id: string;
  gen_art_seed: string;
  name: string;
  /// Listing `content_uri` joined from the snapshot — `ipfs://CID`,
  /// `xvn://strategy/<ulid>`, or "" when the listing isn't indexed.
  content_uri: string;
  buyer: string;
  price_usdc: number;
  seller_proceeds_usdc: number;
  protocol_proceeds_usdc: number;
  license_token_id: string;
  purchase_path: number;
  block_time_unix: number;
}

/**
 * The bundle CID behind a listing's `content_uri`, for receipt display.
 * `ipfs://CID` → `CID`; `xvn://strategy/<ulid>` (local-only, no IPFS pin)
 * and anything else → "" — the receipt shows an honest empty rather than
 * pretending a non-IPFS URI is a CID.
 */
export function bundleCidFromContentUri(contentUri: string | undefined): string {
  if (!contentUri) return "";
  return contentUri.startsWith("ipfs://") ? contentUri.slice("ipfs://".length) : "";
}

/**
 * PublicManifest fields the marketplace surfaces from an open-tier bundle.
 * All optional defensively — the bundle is author-supplied JSON.
 * asset_universe entries are "BASE/QUOTE" pair strings, e.g. "ETH/USD".
 */
export interface PublicManifest {
  display_name?: string;
  plain_summary?: string;
  asset_universe?: string[];
  risk_preset_or_config?: unknown;
  decision_cadence_minutes?: number;
  creator?: string;
  attested_with?: string[];
  required_tools?: string[];
}

/**
 * `GET /api/marketplace/listings/:id/bundle` response. For OPEN listings this
 * carries `{verified, manifest}`; for SEALED listings it carries
 * `{encrypted:true, ciphertext, content_hash}` instead — the manifest is
 * undecryptable without satisfying the Lit gate.
 *
 * `manifest` is the canonical Strategy JSON; the human-readable fields live
 * one level deeper at `manifest.manifest` (PublicManifest).
 */
export interface BundleOut {
  listing_id: number;
  content_uri: string;
  encrypted?: boolean;
  ciphertext?: string;
  content_hash?: string;
  verified?: boolean;
  /** Full canonical Strategy JSON. PublicManifest is at manifest.manifest. */
  manifest?: { manifest?: PublicManifest };
}

/** Fetch a listing's bundle (open manifest or sealed ciphertext). */
export async function fetchBundle(listingId: Id): Promise<BundleOut> {
  return apiFetch<BundleOut>(
    `/api/marketplace/listings/${encodeURIComponent(String(listingId))}/bundle`,
  );
}

/**
 * Standalone sealed-tier import: fetch bundle → Lit-gated decrypt → POST the
 * plaintext manifest to import-sealed. Exported so callers can drive it without
 * instantiating the data client (the LineageRoute buy flow finalizes through
 * `importSealed` below). See `importSealed` for the error contract; the
 * server's on-chain `content_hash` recheck (409) is the integrity authority.
 */
export async function importSealedListing(
  listingId: Id,
): Promise<{ agent_id: string }> {
  const bundle = await fetchBundle(listingId);
  if (!bundle.encrypted || !bundle.ciphertext) {
    throw new Error("Listing is not a sealed bundle.");
  }
  // decrypt also returns the server-issued challenge `message` + its
  // `signature` (lane cgz): the server re-recovers the signer, requires it to
  // equal `address`, and consumes the single-use nonce before granting import.
  const { manifest, message, signature } = await decryptSealedBundle({
    listingId,
    ciphertext: bundle.ciphertext,
  });
  const address = await currentAddress();
  if (!address) throw new WalletRequiredError();
  return apiFetch<{ agent_id: string }>(
    `/api/marketplace/listings/${encodeURIComponent(String(listingId))}/import-sealed`,
    { method: "POST", body: JSON.stringify({ address, manifest, message, signature }) },
  );
}

export interface MarketplaceIndexStatus {
  active: boolean;
  last_poll_unix: number;
  total_onchain: number;
  last_error: string | null;
}

function toRow(l: IndexedListing): ListingRow {
  return {
    // Part A (.7): prefer agent_id (ULID) for routing so detail-page URLs
    // are stable across re-mints. Fall back to numeric listing_id for
    // listings with empty agent_id (pre-ULID on-chain entries).
    id: l.agent_id || String(l.listing_id),
    lineageId: l.agent_id || String(l.listing_id),
    version: "v1",
    // QA9: populate name from the IndexedListing.name field so the browse
    // entry can display a human-readable title instead of the raw id.
    name: l.name || undefined,
    creator: { address: l.seller },
    model: "",
    style: l.symmetry,
    assets: [],
    // Part B: consume real perf metrics when the backend provides them.
    // buyers.agents and clones are BLOCKED (no on-chain data) — leave 0
    // with a code comment referencing bead xvision-ctkm.8.
    return30dPct: l.return30d_pct ?? 0,
    sharpe: l.sharpe ?? 0,
    // Honest approximation: the chain only tells us how many licenses sold,
    // not whether the buyer was a human or an agent — count them all as
    // humans rather than inventing an agent split.
    buyers: { humans: l.units_sold, agents: 0 },
    priceUsdc: l.price_usdc > 0 ? l.price_usdc : null,
    tier: l.tier === 1 ? "sealed" : "open",
    transferableLicense: l.transferable_license,
    // Positive-only badge: any on-chain eval attestation marks the listing
    // verified; zero attestations renders no badge (never a negative mark).
    verification: l.attestation_count > 0 ? "verified" : "unverified",
    acceptsX402: true,
    clones: 0,
    // QA11: fallback to String(listing_id) when gen_art_seed is absent so
    // the gen-art plate never renders with an empty seed.
    genArtSeed: l.gen_art_seed || String(l.listing_id),
  };
}

function toDetail(
  l: IndexedListing,
  networkSlug: string = activeNetworkSlug,
): ListingDetail {
  return {
    ...toRow(l),
    // Chain metadata name is the only human-readable copy we have; it renders
    // in the promise slot under the title.
    promise: l.name,
    // Part B: surface real perf metrics when available; leave other metric
    // slots zeroed (winRatePct, maxDrawdownPct, avgDurationDays are not yet
    // wired from eval_runs — tracked by bead xvision-ctkm.8).
    metrics: {
      return30dPct: l.return30d_pct ?? 0,
      sharpe: l.sharpe ?? 0,
      winRatePct: 0,
      maxDrawdownPct: 0,
      avgDurationDays: 0,
    },
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
        network: networkSlug,
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

/**
 * Convert an asset_universe entry ("ETH/USD", "BTC/USDT", …) to its base
 * ticker ("ETH", "BTC"). Falls back to the original string when the separator
 * is absent. Deduplication is handled at the call site.
 */
function assetTicker(pair: string): string {
  const slash = pair.indexOf("/");
  return slash > 0 ? pair.slice(0, slash) : pair;
}

export class ApiMarketplaceData implements MarketplaceData {
  // W2-DATA: required by the MarketplaceData interface (added by W1-FOUNDATION).
  readonly dataSource = "api" as const;

  /** Memoised PublicManifest per numeric listing id. Failures cache as null. */
  private readonly bundleCache = new Map<string, PublicManifest | null>();

  constructor(private fallback: MarketplaceData) {}

  /**
   * Fetch and memoize the PublicManifest for an OPEN-tier listing.
   * Returns null when the listing is sealed (no manifest pre-purchase), when
   * the fetch fails, or when the id is not numeric (fixture slug).
   * Never throws — callers degrade gracefully when null.
   */
  private async fetchPublicManifest(listingId: string): Promise<PublicManifest | null> {
    if (!/^\d+$/.test(listingId)) return null;
    if (this.bundleCache.has(listingId)) return this.bundleCache.get(listingId)!;
    try {
      const bundle = await fetchBundle(listingId);
      // Sealed bundles carry ciphertext, not a readable manifest.
      const manifest = (!bundle.encrypted && bundle.manifest?.manifest) || null;
      this.bundleCache.set(listingId, manifest);
      return manifest;
    } catch {
      this.bundleCache.set(listingId, null);
      return null;
    }
  }

  async listListings(f: FilterState) {
    const out = await apiFetch<{ items: IndexedListing[]; total: number }>(
      "/api/marketplace/listings",
    );
    const allRows = out.items.map(toRow);

    // "Mine" segment: restrict to listings created by the connected wallet.
    // If no wallet is connected, return an empty pool so the empty state renders
    // (never show all listings when the ownership check cannot be performed).
    if (f.segment === "mine") {
      const viewer = await this.getViewer();
      if (!viewer.isConnected || viewer.createdListingIds.length === 0) {
        return { rows: [], total: allRows.length, matched: 0 };
      }
      const owned = allRows.filter((r) => viewer.createdListingIds.includes(r.id));
      return applyFilter(owned, f);
    }

    return applyFilter(allRows, f);
  }

  async getListing(idOrName: string): Promise<ListingDetail> {
    try {
      const l = await apiFetch<IndexedListing>(
        `/api/marketplace/listings/${encodeURIComponent(idOrName)}`,
      );
      const detail = toDetail(l, (await getActiveNetworkConfigOrDefault()).slug);
      // For OPEN-tier listings, enrich the detail with the verified bundle
      // manifest. Tolerate failure — manifest unavailable must not throw the
      // detail page down.
      if (l.tier === 0) {
        const manifest = await this.fetchPublicManifest(String(l.listing_id));
        if (manifest) {
          if (manifest.plain_summary) detail.promise = manifest.plain_summary;
          if (manifest.display_name) detail.name = manifest.display_name;
          if (manifest.asset_universe && manifest.asset_universe.length > 0) {
            // Deduplicate base tickers ("ETH/USD","ETH/BTC" both → "ETH").
            detail.assets = [...new Set(manifest.asset_universe.map(assetTicker))];
          }
        }
      }
      return detail;
    } catch (e) {
      // QA11: for purely-numeric (on-chain) ids that 404, rethrow so the
      // caller surfaces the designed not-found state rather than silently
      // serving a wrong-seed fixture. Slug ids (non-numeric) may still fall
      // back to the fixture client for demo/dev use.
      //
      // Part A (.7): also rethrow for ULID-shaped ids. A ULID is exactly 26
      // chars of uppercase alphanumeric (digits + uppercase letters, Crockford
      // base32 — excludes I/L/O/U). A real ULID that 404s is a genuine
      // not-found, not a fixture slug. The fixture fallback must NOT serve
      // stale/wrong-seed data for real ULIDs.
      // We use a broader pattern (any 26-char uppercase alphanumeric) to
      // avoid subtle regex edge cases; all valid fixture slugs are shorter or
      // contain lowercase/hyphens.
      const isUlidShaped = /^[0-9A-Z]{26}$/.test(idOrName);
      if (/^\d+$/.test(idOrName) || isUlidShaped) throw e;
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

  // ——— overrides with live/honest implementations ———

  // QA1: return real slice counts by recomputing each slice's count from
  // actual listing rows. Slice definitions (id/label/hint/filter) come from
  // SLICES; counts are computed by applying each slice's filter to live rows.
  async getSlices(): Promise<Slice[]> {
    const out = await apiFetch<{ items: IndexedListing[]; total: number }>(
      "/api/marketplace/listings",
    );
    const rows = out.items.map(toRow);
    return SLICES.map((slice) => ({
      ...slice,
      count: applyFilter(rows, { ...defaultFilterState(), ...slice.filter } as FilterState).matched,
    }));
  }

  // QA1: return a real wallet-based viewer instead of delegating to the
  // fixture @ed viewer. When no wallet is connected, return isConnected:false.
  // The wallet→listing join is deferred; listing id arrays are empty for now.
  async getViewer(): Promise<Viewer> {
    const address = await currentAddress();
    if (!address) {
      return { isConnected: false, createdListingIds: [], ownedListingIds: [] };
    }
    return { isConnected: true, address, createdListingIds: [], ownedListingIds: [] };
  }

  // QA1: return a no-op cleanup instead of delegating to the fixture 5-second
  // fake purchase feed. No fake purchase events in the real client.
  subscribePurchases(_cb: (e: PurchaseEvent) => void): () => void {
    return () => {};
  }

  // ——— everything else delegates to the fixture client ———
  getCreator(handleOrAddress: string): Promise<CreatorProfile> {
    return this.fallback.getCreator(handleOrAddress);
  }
  getLeaderboard(sliceId: SliceId): Promise<{ slice: Slice; rows: ListingRow[] }> {
    return this.fallback.getLeaderboard(sliceId);
  }
  async getReceipt(txHash: string): Promise<Receipt> {
    // Only real 32-byte tx hashes hit the chain-backed route; fixture hashes
    // (e.g. "0xdemo-tx") keep resolving from the fixture client.
    if (!/^0x[0-9a-fA-F]{64}$/.test(txHash)) {
      return this.fallback.getReceipt(txHash);
    }
    const r = await apiFetch<ReceiptOut>(`/api/marketplace/receipts/${txHash}`);
    let licenseContract = "";
    try {
      licenseContract = (await getContracts()).license_token ?? "";
    } catch {
      // address book unavailable — honest empty, receipt still renders
    }
    const at = new Date(r.block_time_unix * 1000).toISOString();
    // Part A (.7): use agent_id (ULID) for the share URL when available so
    // the receipt deep-link routes to the ULID-keyed detail page.
    const routingId = r.agent_id || String(r.listing_id);
    const listingId = String(r.listing_id);
    const buyers = { humans: 0, agents: 0 };
    return {
      txHash: r.tx_hash,
      network: (await getActiveNetworkConfigOrDefault()).slug,
      at,
      buyer: r.buyer,
      listing: {
        id: routingId,
        version: "v1",
        creator: { address: "" },
        genArtSeed: r.gen_art_seed,
        return30dPct: 0,
        buyers,
      },
      license: {
        tokenId: String(r.license_token_id),
        contract: licenseContract,
        manifestHash: "", // not carried by the receipts route
        bundleCid: bundleCidFromContentUri(r.content_uri),
        pricePaidUsdc: r.price_usdc,
        feeUsdc: r.protocol_proceeds_usdc,
        netToCreatorUsdc: r.seller_proceeds_usdc,
        mintedAt: at,
      },
      // Honest empties: nothing detected/derived locally yet.
      install: { xvnDetected: false, xvnEndpoint: "", ingredients: [] },
      share: {
        ogCard: {
          id: routingId,
          version: "v1",
          creator: { address: "" },
          genArtSeed: r.gen_art_seed,
          return30dPct: 0,
          buyers,
          paidToCreatorUsd: 0,
          priceUsdc: r.price_usdc,
          verification: "unverified",
          acceptsX402: true,
          promise: r.name,
          url: `/marketplace/lineage/${routingId}`,
        },
        buyerStamp: `bought by ${r.buyer.slice(0, 6)}…${r.buyer.slice(-4)}`,
        caption: `I just bought ${r.name || listingId} for ${r.price_usdc} USDC on Mantle Sepolia.`,
        variants: [],
        notificationHint: "",
      },
    };
  }
  async listListableStrategies(): Promise<ListableStrategy[]> {
    return fetchListableStrategies();
  }
  async createPublishDraft(strategyId: string): Promise<PublishDraft> {
    return fetchPublishDraft(strategyId);
  }
  // Real in-UI purchase. Primary path is gasless x402: sign an EIP-3009
  // TransferWithAuthorization in the browser and let the backend relay
  // `buyWithAuthorization` (server pays gas; the signature is the buyer's
  // authority). When the relay is unavailable (503) fall back to the two-tx
  // approve+buy path from the user's wallet. Other errors propagate.
  async purchaseIntent(listingId: Id): Promise<TxRef> {
    // Fixture slug ids never hit the chain; on-chain listing ids are numeric.
    if (!/^\d+$/.test(listingId)) {
      return this.fallback.purchaseIntent(listingId);
    }
    const addr = await currentAddress();
    if (!addr) throw new WalletRequiredError();
    await ensureMantleSepolia();

    const listing = await apiFetch<IndexedListing>(
      `/api/marketplace/listings/${encodeURIComponent(listingId)}`,
    );
    const price6 = BigInt(Math.round(listing.price_usdc * 1e6));

    const balance = await usdcBalance(addr);
    if (balance < price6) throw new InsufficientUsdcError(price6, balance);

    const authorization: RelayAuthorization = await signTransferAuthorization({
      from: addr,
      valueUsdc6: price6,
    });
    try {
      const out = await apiFetch<{ tx_hash: string; license_token_id: string }>(
        "/api/marketplace/buy",
        {
          method: "POST",
          body: JSON.stringify({
            listing_id: Number(listingId),
            recipient: addr,
            authorization,
          }),
        },
      );
      return {
        txHash: out.tx_hash,
        network: (await getActiveNetworkConfigOrDefault()).slug,
      };
    } catch (e) {
      if (!(e instanceof ApiError) || e.status !== 503) throw e;
      // Relay unavailable — approve + buy directly from the wallet.
      await approveUsdc(price6);
      const txHash = await buyDirect(BigInt(listingId), addr);
      return {
        txHash,
        network: (await getActiveNetworkConfigOrDefault()).slug,
      };
    }
  }
  /**
   * SEALED-tier import: fetch the bundle, decrypt the ciphertext through the
   * Lit gate (license-gated), then POST the plaintext manifest to the
   * import-sealed route. The server re-checks `keccak256(canonical(manifest))`
   * against the on-chain `content_hash` (409 on mismatch) — that on-chain
   * recheck, not any browser-side hash, is the integrity authority.
   *
   * Resolves to the local `agent_id` the manifest landed as. Errors propagate:
   *   - WalletRequiredError / SealedNotConfiguredError / SealedGateError from
   *     decrypt (no wallet, no Lit config, gate rejection / no license),
   *   - ApiError 403 (no license at import), 409 (hash mismatch) from the POST.
   */
  async importSealed(listingId: Id): Promise<{ agent_id: string }> {
    return importSealedListing(listingId);
  }
  /**
   * OPEN/free-tier finalize: POST `{address}` to the plain import route, which
   * materializes the referenced agents server-side and returns the new local
   * strategy `agent_id`. Mirrors the legacy InstallSteps.runImport apiFetch
   * shape. Fixture slug ids delegate to the fixture client (deterministic fake).
   */
  async importListing(listingId: Id): Promise<{ agent_id: string }> {
    if (!/^\d+$/.test(listingId)) {
      return this.fallback.importListing(listingId);
    }
    const address = await currentAddress();
    if (!address) throw new WalletRequiredError();
    return apiFetch<{ agent_id: string }>(
      `/api/marketplace/listings/${encodeURIComponent(String(listingId))}/import`,
      { method: "POST", body: JSON.stringify({ address }) },
    );
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
