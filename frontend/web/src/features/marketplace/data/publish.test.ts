// publish.test.ts — TDD first: write failing tests, then implement publish.ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { publishListing } from "./publish";
import type { PublishDraft } from "./types";

const draftFixture: PublishDraft = {
  strategyId: "local-btc-momentum",
  name: "BTC Momentum",
  listable: [
    { ok: true, label: "Strategy exists in your XVN" },
    { ok: true, label: "Declares an asset universe" },
    { ok: true, label: "Has a backtest on record" },
  ],
  tier: "sealed",
  priceUsdc: 49,
  acceptedPayers: { humans: true, agents: true },
  ingredients: [
    { name: "Claude Haiku 4.5", kind: "model", installed: true },
  ],
  preview: {
    id: "btc-momentum", lineageId: "btc-momentum", version: "v3.0",
    creator: { address: "0xa83e", handle: "@ed" }, model: "Claude · Haiku 4.5", style: "Day",
    assets: ["BTC"], return30dPct: 47.2, sharpe: 1.31, buyers: { humans: 0, agents: 0 },
    priceUsdc: 49, tier: "sealed", verification: "unverified", acceptsX402: true,
    transferableLicense: false, genArtSeed: "btc-momentum-preview",
  },
};

function mockOkJson(body: unknown) {
  return Promise.resolve({
    ok: true,
    status: 201,
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

describe("publishListing", () => {
  afterEach(() => vi.restoreAllMocks());

  it("POSTs the draft to /api/marketplace/publish and maps the TxRef", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockOkJson({
        agent_id: "01HX",
        manifest_hash: "ab".repeat(32),
        token_id: "7",
        listing_id: "3",
        token_uri_bytes: 4200,
      }),
    );

    const tx = await publishListing(draftFixture);

    // Verify the URL
    const [calledUrl, calledInit] = fetchMock.mock.calls[0];
    expect(String(calledUrl)).toContain("/api/marketplace/publish");

    // Verify method
    expect(calledInit?.method).toBe("POST");

    // Verify body fields
    const body = JSON.parse(calledInit?.body as string);
    expect(body.strategy_id).toBe("local-btc-momentum");
    expect(body.tier).toBe("sealed");
    expect(body.price_usdc).toBe(49);
    expect(body.transferable_license).toBe(false);
    // The creator-chosen listing name is forwarded so the listing inherits a
    // real name instead of "Strategy #N".
    expect(body.name).toBe("BTC Momentum");

    // Verify TxRef mapping
    expect(tx.network).toBe("mantle-sepolia");
    expect(tx.txHash).toBe("3"); // mapped from listing_id
  });

  it("throws on a 503 (chain env unset) instead of faking success", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockErrorJson(503, { code: "service_unavailable", message: "Chain env not configured" }),
    );

    await expect(publishListing(draftFixture)).rejects.toThrow();
  });

  it("sends price_usdc as 0 when draft.priceUsdc is null (open tier)", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      mockOkJson({
        agent_id: "01HY",
        manifest_hash: "cd".repeat(32),
        token_id: "8",
        listing_id: "4",
        token_uri_bytes: 1000,
      }),
    );

    await publishListing({ ...draftFixture, tier: "open", priceUsdc: null });

    const body = JSON.parse(fetchMock.mock.calls[0][1]?.body as string);
    expect(body.price_usdc).toBe(0);
    expect(body.tier).toBe("open");
  });
});
