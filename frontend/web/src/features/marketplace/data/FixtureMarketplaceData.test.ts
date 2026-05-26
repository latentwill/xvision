// src/features/marketplace/data/FixtureMarketplaceData.test.ts
import { describe, expect, it, vi } from "vitest";
import { FixtureMarketplaceData } from "./MarketplaceData";
import { defaultFilterState } from "./filter";
import { ED_CREATOR } from "./fixtures/creators";

const mp = new FixtureMarketplaceData();

describe("FixtureMarketplaceData", () => {
  it("lists with totals", async () => {
    const { rows, total, matched } = await mp.listListings(defaultFilterState());
    expect(total).toBeGreaterThan(200);
    expect(rows.length).toBe(matched);
  });
  it("gets a known listing detail", async () => {
    const d = await mp.getListing("btc-momentum-v3");
    expect(d.metrics.winRatePct).toBe(62);
    expect(d.onChain.nft.network).toBe("mantle-sepolia");
  });
  it("rejects unknown listing", async () => {
    await expect(mp.getListing("nope")).rejects.toThrow();
  });
  it("gets a creator by handle", async () => {
    const c = await mp.getCreator("@ed");
    expect(c.counters.strategies).toBe(3);
  });
  it("leaderboard returns slice + rows", async () => {
    const { slice, rows } = await mp.getLeaderboard("sol-7d");
    expect(slice.id).toBe("sol-7d");
    expect(rows.every((r) => r.assets.includes("SOL"))).toBe(true);
  });
  it("publish draft + submit returns a tx", async () => {
    const draft = await mp.createPublishDraft("local-btc-momentum");
    const { txHash } = await mp.submitListing(draft);
    expect(txHash).toMatch(/^0x/);
  });
  it("purchaseIntent returns a TxRef with network (testnet label source)", async () => {
    const ref = await mp.purchaseIntent("btc-momentum-v3");
    expect(ref.txHash).toMatch(/^0x/);
    expect(ref.network).toBe("mantle-sepolia");
  });
  it("exposes a fixture viewer (Mine + clone-gate source)", async () => {
    const v = await mp.getViewer();
    expect(v.isConnected).toBe(true);
    expect(v.createdListingIds).toContain("btc-momentum-v3");
  });
  it("Mine segment filters to the viewer's created listings", async () => {
    const v = await mp.getViewer();
    const { rows } = await mp.listListings({ ...defaultFilterState(), segment: "mine" });
    expect(rows.map((r) => r.id).sort()).toEqual([...v.createdListingIds].sort());
  });
  it("getLeaderboard('free') rows are all tier === 'open'", async () => {
    const { rows } = await mp.getLeaderboard("free");
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.every((r) => r.tier === "open")).toBe(true);
  });
  it("getCreator by address resolves to same profile as by handle", async () => {
    const byHandle = await mp.getCreator("@ed");
    const byAddress = await mp.getCreator(ED_CREATOR.address);
    expect(byAddress).toBe(byHandle);
  });
  it("getCreator by ENS resolves to same profile as by handle", async () => {
    const byHandle = await mp.getCreator("@ed");
    const byEns = await mp.getCreator(ED_CREATOR.ens);
    expect(byEns).toBe(byHandle);
  });
  it("subscribePurchases emits and unsubscribes", () => {
    vi.useFakeTimers();
    const cb = vi.fn();
    const off = mp.subscribePurchases(cb);
    vi.advanceTimersByTime(6000);
    expect(cb).toHaveBeenCalled();
    off();
    cb.mockClear();
    vi.advanceTimersByTime(6000);
    expect(cb).not.toHaveBeenCalled();
    vi.useRealTimers();
  });
});
