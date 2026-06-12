// ApiMarketplaceData.test.ts — TDD first: real indexer-backed reads with
// fixture fallback. Mocks `globalThis.fetch` like publish.test.ts does.
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ApiMarketplaceData,
  chooseMarketplaceData,
} from "./ApiMarketplaceData";
import { FixtureMarketplaceData, type MarketplaceData } from "./MarketplaceData";
import { defaultFilterState } from "./filter";
import type { ListingDetail } from "./types";

// Mock chain so getViewer tests don't need real wallet state.
vi.mock("../lib/chain", () => ({
  currentAddress: vi.fn(async () => null),
  ensureMantleSepolia: vi.fn(),
  usdcBalance: vi.fn(),
  signTransferAuthorization: vi.fn(),
  approveUsdc: vi.fn(),
  buyDirect: vi.fn(),
  getContracts: vi.fn(),
  faucetUsdc: vi.fn(),
}));

import * as chain from "../lib/chain";
const mockedChain = vi.mocked(chain);

// One on-chain listing as the backend indexer serves it.
const indexedListing = {
  listing_id: 3,
  agent_nft_id: "7",
  agent_id: "01HXAGENT",
  seller: "0xa83e000000000000000000000000000000000001",
  content_hash: "ab".repeat(32),
  content_uri: "ipfs://bafy123",
  tier: 1,
  price_usdc: 49, // whole USDC, same unit publish.ts sends

  transferable_license: true,
  revoked: false,
  gen_art_seed: "seed-xyz",
  name: "BTC Momentum",
  symmetry: "radial-6",
  palette: "ember",
  attestation_count: 2,
  units_sold: 3,
  earned_usdc: 12.5,
};

const freeListing = {
  ...indexedListing,
  listing_id: 4,
  agent_id: "",
  tier: 0,
  price_usdc: 0,
  transferable_license: false,
  gen_art_seed: "seed-free",
  name: "Open One",
  attestation_count: 0,
  units_sold: 0,
  earned_usdc: 0,
};

// Listing with an empty gen_art_seed — should fall back to String(listing_id).
const noSeedListing = {
  ...indexedListing,
  listing_id: 5,
  gen_art_seed: "",
  name: "No Seed",
};

function mockOkJson(body: unknown) {
  return Promise.resolve({
    ok: true,
    status: 200,
    json: () => Promise.resolve(body),
  } as Response);
}

function mockErrorJson(status: number, body: unknown) {
  return Promise.resolve({
    ok: false,
    status,
    statusText: `HTTP ${status}`,
    json: () => Promise.resolve(body),
  } as Response);
}

function makeFallback(): MarketplaceData {
  return new FixtureMarketplaceData();
}

describe("ApiMarketplaceData.listListings", () => {
  afterEach(() => vi.restoreAllMocks());

  it("maps IndexedListing → ListingRow and keeps all rows under the default filter", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() =>
        mockOkJson({ items: [indexedListing, freeListing], total: 2 }),
      );

    const api = new ApiMarketplaceData(makeFallback());
    const { rows, total, matched } = await api.listListings(defaultFilterState());

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/marketplace/listings",
      expect.anything(),
    );
    expect(total).toBe(2);
    expect(matched).toBe(2);
    expect(rows).toHaveLength(2);

    const sealed = rows.find((r) => r.id === "3")!;
    expect(sealed).toBeDefined();
    expect(sealed.lineageId).toBe("01HXAGENT");
    expect(sealed.version).toBe("v1");
    expect(sealed.creator.address).toBe(indexedListing.seller);
    expect(sealed.tier).toBe("sealed");
    expect(sealed.priceUsdc).toBe(49);
    expect(sealed.transferableLicense).toBe(true);
    // attestation_count > 0 → verified (badge stays positive-only)
    expect(sealed.verification).toBe("verified");
    expect(sealed.acceptsX402).toBe(true);
    expect(sealed.clones).toBe(0);
    // units_sold approximated as human buyers (agents not distinguished on-chain)
    expect(sealed.buyers).toEqual({ humans: 3, agents: 0 });
    expect(sealed.return30dPct).toBe(0);
    expect(sealed.sharpe).toBe(0);
    expect(sealed.assets).toEqual([]);
    expect(sealed.genArtSeed).toBe("seed-xyz");
    // QA9: name field populated from IndexedListing.name
    expect(sealed.name).toBe("BTC Momentum");

    const open = rows.find((r) => r.id === "4")!;
    expect(open.tier).toBe("open");
    expect(open.priceUsdc).toBeNull(); // price 0 → null (open/free)
    expect(open.lineageId).toBe("4"); // empty agent_id falls back to listing id
    expect(open.verification).toBe("unverified"); // zero attestations
    expect(open.buyers).toEqual({ humans: 0, agents: 0 });
    expect(open.name).toBe("Open One");
  });

  it("QA11: genArtSeed falls back to String(listing_id) when gen_art_seed is empty", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockOkJson({ items: [noSeedListing], total: 1 }),
    );
    const api = new ApiMarketplaceData(makeFallback());
    const { rows } = await api.listListings(defaultFilterState());
    expect(rows[0].genArtSeed).toBe("5");
  });
});

describe("ApiMarketplaceData.getListing", () => {
  afterEach(() => vi.restoreAllMocks());

  it("maps the detail honestly: real chain fields, zeroed metrics, empty activity", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockOkJson(indexedListing),
    );

    const api = new ApiMarketplaceData(makeFallback());
    const d: ListingDetail = await api.getListing("3");

    expect(d.id).toBe("3");
    expect(d.genArtSeed).toBe("seed-xyz");
    expect(d.promise).toBe("BTC Momentum"); // chain metadata name
    expect(d.verification).toBe("verified"); // attestation_count 2
    expect(d.buyers).toEqual({ humans: 3, agents: 0 }); // units_sold approximation
    expect(d.metrics).toEqual({
      return30dPct: 0,
      sharpe: 0,
      winRatePct: 0,
      maxDrawdownPct: 0,
      avgDurationDays: 0,
    });
    expect(d.variants).toEqual([]);
    expect(d.recentBuyers).toEqual([]);
    expect(d.creatorOther).toEqual([]);
    expect(d.ingredients).toEqual([]);
    expect(d.equityCurve.points).toEqual([]);
    expect(d.onChain.nft.tokenId).toBe("7");
    expect(d.onChain.nft.manifestHash).toBe(indexedListing.content_hash);
    expect(d.onChain.nft.agentURI).toBe("ipfs://bafy123");
    expect(d.onChain.trades).toEqual([]);
  });

  it("falls back to the fixture client on 404 for slug (non-numeric) ids", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockErrorJson(404, { code: "not_found", message: "listing not found" }),
    );

    const fallback = makeFallback();
    const spy = vi.spyOn(fallback, "getListing");
    const api = new ApiMarketplaceData(fallback);

    const d = await api.getListing("btc-momentum-v3");
    expect(spy).toHaveBeenCalledWith("btc-momentum-v3");
    expect(d.id).toBe("btc-momentum-v3");
  });

  it("QA11: throws (no fixture fallback) for numeric on-chain id that 404s", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockErrorJson(404, { code: "not_found", message: "listing not found" }),
    );

    const fallback = makeFallback();
    const spy = vi.spyOn(fallback, "getListing");
    const api = new ApiMarketplaceData(fallback);

    await expect(api.getListing("999")).rejects.toThrow();
    expect(spy).not.toHaveBeenCalled();
  });
});

describe("ApiMarketplaceData.getStats", () => {
  afterEach(() => vi.restoreAllMocks());

  it("reports real total, zeroed money/activity counters", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockOkJson({ items: [indexedListing], total: 1 }),
    );

    const api = new ApiMarketplaceData(makeFallback());
    expect(await api.getStats()).toEqual({
      totalStrategies: 1,
      paidThisWeekUsd: 0,
      agentPurchases: 0,
      mintedLast24h: 0,
    });
  });
});

describe("ApiMarketplaceData.dataSource", () => {
  it("exposes dataSource = 'api'", () => {
    const api = new ApiMarketplaceData(makeFallback());
    expect(api.dataSource).toBe("api");
  });
});

describe("ApiMarketplaceData.getViewer", () => {
  afterEach(() => vi.restoreAllMocks());

  it("QA1: returns isConnected:false when no wallet is connected", async () => {
    mockedChain.currentAddress.mockResolvedValue(null);
    const api = new ApiMarketplaceData(makeFallback());
    const viewer = await api.getViewer();
    expect(viewer.isConnected).toBe(false);
    expect(viewer.createdListingIds).toEqual([]);
    expect(viewer.ownedListingIds).toEqual([]);
    expect(viewer.address).toBeUndefined();
  });

  it("QA1: returns isConnected:true with address when wallet is connected", async () => {
    const ADDR = "0x1234567890abcdef1234567890abcdef12345678" as `0x${string}`;
    mockedChain.currentAddress.mockResolvedValue(ADDR);
    const api = new ApiMarketplaceData(makeFallback());
    const viewer = await api.getViewer();
    expect(viewer.isConnected).toBe(true);
    expect(viewer.address).toBe(ADDR);
    expect(viewer.createdListingIds).toEqual([]);
    expect(viewer.ownedListingIds).toEqual([]);
  });

  it("QA1: does NOT return fixture @ed viewer", async () => {
    mockedChain.currentAddress.mockResolvedValue(null);
    const api = new ApiMarketplaceData(makeFallback());
    const viewer = await api.getViewer();
    // The fixture viewer has @ed handle — the real client must not return it
    expect(viewer.handle).toBeUndefined();
  });
});

describe("ApiMarketplaceData.subscribePurchases", () => {
  it("QA1: returns a no-op cleanup function — no fake purchase events", () => {
    const api = new ApiMarketplaceData(makeFallback());
    const events: unknown[] = [];
    const cleanup = api.subscribePurchases((e) => events.push(e));
    expect(typeof cleanup).toBe("function");
    // No events emitted synchronously
    expect(events).toHaveLength(0);
    // Cleanup is callable without error
    expect(() => cleanup()).not.toThrow();
  });
});

describe("ApiMarketplaceData.getSlices", () => {
  afterEach(() => vi.restoreAllMocks());

  it("QA1: computes live slice counts from real listing rows", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockOkJson({ items: [indexedListing, freeListing], total: 2 }),
    );
    const api = new ApiMarketplaceData(makeFallback());
    const slices = await api.getSlices();
    // Slices should have real (computed) counts, not fixture literals like 1247
    expect(slices.length).toBeGreaterThan(0);
    // Every count is a number
    for (const s of slices) {
      expect(typeof s.count).toBe("number");
    }
    // The "free" slice should match the open-tier row
    const freeSlice = slices.find((s) => s.id === "free");
    expect(freeSlice).toBeDefined();
    // freeListing has tier=0 (open) and return30dPct=0; sealed listing doesn't match "free"
    // The count is the real computed value from our two test rows
    expect(freeSlice!.count).toBe(1); // only freeListing matches tier=["open"]
  });
});

describe("ApiMarketplaceData delegation", () => {
  afterEach(() => vi.restoreAllMocks());

  it("delegates getSlices to compute live counts (not fallback)", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockOkJson({ items: [indexedListing], total: 1 }),
    );
    const fallback = makeFallback();
    const slicesSpy = vi.spyOn(fallback, "getSlices");
    const api = new ApiMarketplaceData(fallback);

    const slices = await api.getSlices();
    // getSlices is now overridden; fallback.getSlices should NOT be called
    expect(slicesSpy).not.toHaveBeenCalled();
    expect(slices.length).toBeGreaterThan(0);
  });
});

describe("chooseMarketplaceData", () => {
  afterEach(() => vi.restoreAllMocks());

  it("returns ApiMarketplaceData when the indexer reports active", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockOkJson({ active: true, last_poll_unix: 1, total_onchain: 2, last_error: null }),
    );
    const fallback = makeFallback();
    const client = await chooseMarketplaceData(fallback);
    expect(client).toBeInstanceOf(ApiMarketplaceData);
  });

  it("returns the fallback when the indexer is inactive", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockOkJson({ active: false, last_poll_unix: 0, total_onchain: 0, last_error: null }),
    );
    const fallback = makeFallback();
    expect(await chooseMarketplaceData(fallback)).toBe(fallback);
  });

  it("returns the fallback when the status fetch fails (jsdom-safe, no rejection)", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      Promise.reject(new TypeError("fetch failed")),
    );
    const fallback = makeFallback();
    expect(await chooseMarketplaceData(fallback)).toBe(fallback);
  });
});
