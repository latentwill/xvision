// src/features/marketplace/data/FixtureMarketplaceData.test.ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FixtureMarketplaceData } from "./MarketplaceData";
import { defaultFilterState } from "./filter";
import { ED_CREATOR } from "./fixtures/creators";
import { LISTABLE_STRATEGIES } from "./fixtures/seller";
import { fetchListableStrategies, fetchPublishDraft } from "./listable";

// submitListing now calls the real publish endpoint; mock it so this unit test
// doesn't require a live server. The real publish path is tested in publish.test.ts.
vi.mock("./publish", () => ({
  publishListing: vi.fn().mockResolvedValue({ txHash: "0xmocked-listing-id", network: "mantle-sepolia" }),
}));

// The sell flow (list-your-strategy picker + draft) is operator-local and now
// hits the real `/api/strategies` via ./listable. Mock that module so these
// unit tests don't touch a backend; the DEFAULT behaviour rejects so the
// documented offline fixture fallback is exercised deterministically. The
// real-API delegation path is asserted in its own test below.
vi.mock("./listable", () => ({
  fetchListableStrategies: vi.fn(),
  fetchPublishDraft: vi.fn(),
}));

const mockedFetchListable = vi.mocked(fetchListableStrategies);
const mockedFetchDraft = vi.mocked(fetchPublishDraft);

const mp = new FixtureMarketplaceData();

beforeEach(() => {
  // Default: backend unreachable → FixtureMarketplaceData falls back to the
  // offline demo fixtures (the behaviour the rest of this suite asserts).
  mockedFetchListable.mockRejectedValue(new Error("no backend in unit test"));
  mockedFetchDraft.mockRejectedValue(new Error("no backend in unit test"));
});

describe("FixtureMarketplaceData", () => {
  it("lists with totals (curated collection only — no at-scale wall fixtures)", async () => {
    const { rows, total, matched } = await mp.listListings(defaultFilterState());
    // The demo client serves only the curated NAMED_LISTINGS so every browse
    // entry is inspectable; the 200 wall-strat at-scale fixtures are excluded.
    expect(total).toBe(6);
    expect(rows.length).toBe(matched);
  });

  it("getListing synthesizes a detail for a curated row without an explicit detail", async () => {
    // sol-strategist-pro has no hand-authored LISTING_DETAILS entry — the
    // fixture client synthesizes one so every entry is inspectable.
    const d = await mp.getListing("sol-strategist-pro");
    expect(d.id).toBe("sol-strategist-pro");
    expect(d.onChain.nft.network).toBe("mantle-sepolia");
    // Performance is a designed empty record (no fabricated curve/trades).
    expect(d.equityCurve.points).toEqual([]);
    expect(d.onChain.trades).toEqual([]);
  });

  it("getSlices computes live counts from the curated pool (no stale hardcoded figures)", async () => {
    const slices = await mp.getSlices();
    const trending = slices.find((s) => s.id === "trending");
    // Trending matches the whole curated collection (6), never the old 1,247.
    expect(trending?.count).toBe(6);
    // Every shown slice has a real, small count.
    expect(slices.every((s) => s.count <= 6)).toBe(true);
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
  it("publish draft does not fabricate placeholder bundle ingredients", async () => {
    const draft = await mp.createPublishDraft("local-btc-momentum");
    expect(draft.ingredients).toEqual([]);
  });
  it("publish draft uses the selected strategy as the listing preview identity art seed", async () => {
    const draft = await mp.createPublishDraft("local-btc-momentum");
    expect(draft.preview.genArtSeed).toBe("local-btc-momentum");
  });

  // QA fix: the "list your strategy" picker must show the operator's REAL
  // strategy library — never the hardcoded placeholder fixtures — even when the
  // on-chain marketplace indexer is inactive and FixtureMarketplaceData is the
  // active client.
  it("listListableStrategies serves the operator's real strategies (not placeholder fixtures)", async () => {
    const real = [
      { id: "orb-breakout-15m-ollama-fino1", name: "ORB Breakout 15m", version: "evaluated 2026-06-10", assets: ["BTC"] },
      { id: "rsi-bb-meanrev-1h-ollama-qwen3", name: "RSI-BB MeanRev 1h", version: "evaluated 2026-06-11", assets: ["ETH"] },
    ];
    mockedFetchListable.mockResolvedValue(real);
    const got = await mp.listListableStrategies();
    expect(got).toEqual(real);
    // Must NOT be the placeholder fixtures the QA flagged.
    expect(got.map((s) => s.id)).not.toContain("local-btc-momentum");
  });

  it("createPublishDraft builds from the operator's real strategy when the backend is reachable", async () => {
    const realDraft = { strategyId: "orb-breakout-15m-ollama-fino1", name: "ORB Breakout 15m" } as Awaited<
      ReturnType<typeof fetchPublishDraft>
    >;
    mockedFetchDraft.mockResolvedValue(realDraft);
    const draft = await mp.createPublishDraft("orb-breakout-15m-ollama-fino1");
    expect(draft).toBe(realDraft);
  });

  it("listListableStrategies falls back to demo fixtures only when the strategies API is unreachable", async () => {
    // beforeEach already makes the fetch reject (no backend).
    const got = await mp.listListableStrategies();
    expect(got).toEqual(LISTABLE_STRATEGIES);
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
