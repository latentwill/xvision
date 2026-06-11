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
    expect(sealed.verification).toBe("unverified");
    expect(sealed.acceptsX402).toBe(true);
    expect(sealed.clones).toBe(0);
    expect(sealed.buyers).toEqual({ humans: 0, agents: 0 });
    expect(sealed.return30dPct).toBe(0);
    expect(sealed.sharpe).toBe(0);
    expect(sealed.assets).toEqual([]);
    expect(sealed.genArtSeed).toBe("seed-xyz");

    const open = rows.find((r) => r.id === "4")!;
    expect(open.tier).toBe("open");
    expect(open.priceUsdc).toBeNull(); // price 0 → null (open/free)
    expect(open.lineageId).toBe("4"); // empty agent_id falls back to listing id
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

  it("falls back to the fixture client on 404", async () => {
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

describe("ApiMarketplaceData delegation", () => {
  afterEach(() => vi.restoreAllMocks());

  it("delegates getSlices / getViewer (and the rest) to the fallback", async () => {
    const fallback = makeFallback();
    const slicesSpy = vi.spyOn(fallback, "getSlices");
    const viewerSpy = vi.spyOn(fallback, "getViewer");
    const api = new ApiMarketplaceData(fallback);

    const slices = await api.getSlices();
    const viewer = await api.getViewer();
    expect(slicesSpy).toHaveBeenCalled();
    expect(viewerSpy).toHaveBeenCalled();
    expect(slices.length).toBeGreaterThan(0);
    expect(viewer).toEqual(await fallback.getViewer());
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
