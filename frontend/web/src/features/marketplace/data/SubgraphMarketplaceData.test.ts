// src/features/marketplace/data/SubgraphMarketplaceData.test.ts
import { describe, expect, it, vi } from "vitest";
import { SubgraphMarketplaceData } from "./SubgraphMarketplaceData";
import type { MarketplaceData } from "./MarketplaceData";
import type { SubgraphClient } from "./subgraph/client";
import { defaultFilterState } from "./filter";

// Mock chain so getViewer tests don't need real wallet state.
vi.mock("../lib/chain", () => ({
  currentAddress: vi.fn(async () => null),
}));

import * as chain from "../lib/chain";
const mockedChain = vi.mocked(chain);

const listing = (id: string, agentId: string) => ({
  id,
  seller: "0x00000000000000000000000000000000000000ab",
  contentHash: "0xdead",
  tier: 0,
  priceUSDC: "1000000",
  protocolFeeBps: 500,
  revoked: false,
  agent: {
    id: agentId,
    owner: "0x00000000000000000000000000000000000000ab",
    manifestCid: "ipfs://cid",
    validations: [],
  },
  sales: [],
  attestations: [],
});

// A stub client that routes by query content.
function stubClient(map: {
  listings?: unknown;
  listing?: unknown;
  stats?: unknown;
}): SubgraphClient {
  return {
    query: vi.fn(async (q: string) => {
      if (q.includes("query Listings")) return { listings: map.listings ?? [] };
      if (q.includes("query Listing(")) return { listing: map.listing ?? null };
      if (q.includes("query Stats")) return map.stats;
      throw new Error(`unexpected query: ${q.slice(0, 30)}`);
    }),
  } as unknown as SubgraphClient;
}

// A fallback that records which methods were delegated.
function spyFallback(): MarketplaceData {
  const tx = { txHash: "0xfake", network: "mantle-sepolia" };
  return {
    dataSource: "fixture" as const,
    getStats: vi.fn(),
    listListings: vi.fn(),
    getSlices: vi.fn(async () => []),
    getListing: vi.fn(),
    getCreator: vi.fn(async () => ({}) as never),
    getLeaderboard: vi.fn(),
    getReceipt: vi.fn(async () => ({}) as never),
    getViewer: vi.fn(async () => ({ isConnected: false, createdListingIds: [], ownedListingIds: [] })),
    listListableStrategies: vi.fn(async () => []),
    createPublishDraft: vi.fn(async () => ({}) as never),
    submitListing: vi.fn(async () => tx),
    purchaseIntent: vi.fn(async () => tx),
    importSealed: vi.fn(async () => ({ agent_id: "demo-agent-7" })),
    importListing: vi.fn(async () => ({ agent_id: "demo-agent-7" })),
    subscribePurchases: vi.fn(() => () => {}),
  } as unknown as MarketplaceData;
}

describe("SubgraphMarketplaceData", () => {
  it("listListings maps subgraph listings to rows and applies the filter", async () => {
    const mp = new SubgraphMarketplaceData({
      client: stubClient({ listings: [listing("1", "10"), listing("2", "20")] }),
    });
    const { rows, total, matched } = await mp.listListings(defaultFilterState());
    expect(total).toBe(2);
    expect(matched).toBe(2);
    expect(rows.map((r) => r.id).sort()).toEqual(["1", "2"]);
    expect(rows[0].priceUsdc).toBe(1.0);
  });

  it("getListing maps a detail; throws on a missing listing", async () => {
    const present = new SubgraphMarketplaceData({
      client: stubClient({ listing: { ...listing("7", "42"), agent: { ...listing("7", "42").agent, reputation: [] } } }),
    });
    const d = await present.getListing("7");
    expect(d.id).toBe("7");
    expect(d.onChain.nft.tokenId).toBe("42");

    const absent = new SubgraphMarketplaceData({ client: stubClient({ listing: null }) });
    await expect(absent.getListing("nope")).rejects.toThrow(/not found/);
  });

  it("getStats projects counts", async () => {
    const mp = new SubgraphMarketplaceData({
      client: stubClient({
        stats: { agents: [{ id: "1" }], listings: [{ id: "7" }], sales: [] },
      }),
      nowSecs: () => 1700000000,
    });
    const s = await mp.getStats();
    expect(s.totalStrategies).toBe(1);
  });

  it("uses the manifest resolver for off-chain metadata", async () => {
    const mp = new SubgraphMarketplaceData({
      client: stubClient({ listings: [listing("1", "10")] }),
      manifest: { resolve: async () => ({ name: "BTC Momentum", model: "kimi-k2", assets: ["ETH/USD"] }) },
    });
    const { rows } = await mp.listListings(defaultFilterState());
    expect(rows[0].model).toBe("kimi-k2");
    expect(rows[0].assets).toEqual(["ETH/USD"]);
    // QA9: name field from manifest
    expect(rows[0].name).toBe("BTC Momentum");
  });

  it("getListing: promise comes from meta.description (no fabrication), empty string when absent", async () => {
    const fullListing = {
      ...listing("7", "42"),
      agent: { ...listing("7", "42").agent, reputation: [] },
    };
    // With description in meta
    const withDesc = new SubgraphMarketplaceData({
      client: stubClient({ listing: fullListing }),
      manifest: { resolve: async () => ({ description: "Trades ETH on 15m breakouts." }) },
    });
    const d1 = await withDesc.getListing("7");
    expect(d1.promise).toBe("Trades ETH on 15m breakouts.");

    // Without description in meta (null resolver)
    const withoutDesc = new SubgraphMarketplaceData({
      client: stubClient({ listing: fullListing }),
      manifest: { resolve: async () => null },
    });
    const d2 = await withoutDesc.getListing("7");
    expect(d2.promise).toBe(""); // honest empty, no fabrication
  });

  it("delegates off-chain / write methods to the fallback", async () => {
    const fallback = spyFallback();
    const mp = new SubgraphMarketplaceData({
      client: stubClient({}),
      fallback,
    });
    await mp.getReceipt("0xabc");
    await mp.listListableStrategies();
    await mp.purchaseIntent("7");
    await mp.submitListing({} as never);
    await mp.importSealed("7");
    await mp.importListing("7");
    expect(fallback.getReceipt).toHaveBeenCalledWith("0xabc");
    expect(fallback.listListableStrategies).toHaveBeenCalled();
    expect(fallback.purchaseIntent).toHaveBeenCalledWith("7");
    expect(fallback.submitListing).toHaveBeenCalled();
    expect(fallback.importSealed).toHaveBeenCalledWith("7");
    expect(fallback.importListing).toHaveBeenCalledWith("7");
  });

  it("QA1: getSlices does NOT delegate to fallback — computes live counts", async () => {
    const fallback = spyFallback();
    const mp = new SubgraphMarketplaceData({
      client: stubClient({ listings: [listing("1", "10")] }),
      fallback,
    });
    const slices = await mp.getSlices();
    // getSlices is overridden; fallback.getSlices should NOT be called
    expect(fallback.getSlices).not.toHaveBeenCalled();
    expect(slices.length).toBeGreaterThan(0);
    for (const s of slices) {
      expect(typeof s.count).toBe("number");
    }
  });

  it("QA1: getViewer does NOT delegate to fallback", async () => {
    mockedChain.currentAddress.mockResolvedValue(null);
    const fallback = spyFallback();
    const mp = new SubgraphMarketplaceData({
      client: stubClient({}),
      fallback,
    });
    const viewer = await mp.getViewer();
    expect(fallback.getViewer).not.toHaveBeenCalled();
    expect(viewer.isConnected).toBe(false);
    // Must NOT assert @ed is connected
    expect(viewer.handle).toBeUndefined();
  });

  it("QA1: getViewer returns isConnected:true when wallet is connected", async () => {
    const ADDR = "0x1234567890abcdef1234567890abcdef12345678" as `0x${string}`;
    mockedChain.currentAddress.mockResolvedValue(ADDR);
    const mp = new SubgraphMarketplaceData({ client: stubClient({}) });
    const viewer = await mp.getViewer();
    expect(viewer.isConnected).toBe(true);
    expect(viewer.address).toBe(ADDR);
  });

  it("QA1: subscribePurchases returns a no-op cleanup, not the fixture feed", () => {
    const fallback = spyFallback();
    const mp = new SubgraphMarketplaceData({
      client: stubClient({}),
      fallback,
    });
    const events: unknown[] = [];
    const cleanup = mp.subscribePurchases((e) => events.push(e));
    expect(typeof cleanup).toBe("function");
    expect(events).toHaveLength(0);
    expect(fallback.subscribePurchases).not.toHaveBeenCalled();
    expect(() => cleanup()).not.toThrow();
  });

  it("exposes dataSource = 'subgraph'", () => {
    const mp = new SubgraphMarketplaceData({ client: stubClient({}) });
    expect(mp.dataSource).toBe("subgraph");
  });
});
