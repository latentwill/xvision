// ApiMarketplaceData.purchase.test.ts — TDD for the real purchase flow
// (gasless relay → approve+buy fallback) and real receipt mapping.
// chain.ts is fully mocked at the module boundary; HTTP via fetch spy.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiMarketplaceData } from "./ApiMarketplaceData";
import { FixtureMarketplaceData, type MarketplaceData } from "./MarketplaceData";
import {
  InsufficientUsdcError,
  WalletRequiredError,
} from "../lib/purchaseErrors";
import * as chain from "../lib/chain";

vi.mock("../lib/chain", () => ({
  currentAddress: vi.fn(),
  ensureMantleSepolia: vi.fn(),
  usdcBalance: vi.fn(),
  signTransferAuthorization: vi.fn(),
  approveUsdc: vi.fn(),
  buyDirect: vi.fn(),
  getContracts: vi.fn(),
  faucetUsdc: vi.fn(),
}));

const mocked = vi.mocked(chain);

const ADDR = "0x3333333333333333333333333333333333333333" as const;

const indexedListing = {
  listing_id: 3,
  agent_nft_id: "7",
  agent_id: "01HXAGENT",
  seller: "0xa83e000000000000000000000000000000000001",
  content_hash: "ab".repeat(32),
  content_uri: "ipfs://bafy123",
  tier: 1,
  price_usdc: 49,
  transferable_license: true,
  revoked: false,
  gen_art_seed: "seed-xyz",
  name: "BTC Momentum",
  symmetry: "radial-6",
  palette: "ember",
  attestation_count: 0,
  units_sold: 0,
  earned_usdc: 0,
};

const AUTH = {
  from: ADDR,
  to: "0x2222222222222222222222222222222222222222" as `0x${string}`,
  value: "49000000",
  valid_after: 0,
  valid_before: 1750000000,
  nonce: ("0x" + "ab".repeat(32)) as `0x${string}`,
  v: 27,
  r: ("0x" + "11".repeat(32)) as `0x${string}`,
  s: ("0x" + "22".repeat(32)) as `0x${string}`,
};

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function makeFallback(): MarketplaceData {
  return new FixtureMarketplaceData();
}

let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  vi.clearAllMocks();
  fetchMock = vi.fn();
  vi.stubGlobal("fetch", fetchMock);
  // Happy-path defaults; individual tests override.
  mocked.currentAddress.mockResolvedValue(ADDR);
  mocked.ensureMantleSepolia.mockResolvedValue(undefined);
  mocked.usdcBalance.mockResolvedValue(100_000_000n); // 100 USDC
  mocked.signTransferAuthorization.mockResolvedValue(AUTH);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("ApiMarketplaceData.purchaseIntent", () => {
  it("relay happy path: ensures chain, gates balance, signs, POSTs the relay body", async () => {
    fetchMock.mockImplementation(async (url: string, init?: RequestInit) => {
      if (url === "/api/marketplace/listings/3") return jsonResponse(indexedListing);
      if (url === "/api/marketplace/buy" && init?.method === "POST") {
        return jsonResponse({ tx_hash: "0x" + "cd".repeat(32), license_token_id: "3" });
      }
      throw new Error(`unexpected fetch: ${url}`);
    });

    const api = new ApiMarketplaceData(makeFallback());
    const ref = await api.purchaseIntent("3");

    expect(mocked.ensureMantleSepolia).toHaveBeenCalled();
    expect(mocked.usdcBalance).toHaveBeenCalledWith(ADDR);
    expect(mocked.signTransferAuthorization).toHaveBeenCalledWith({
      from: ADDR,
      valueUsdc6: 49_000_000n,
    });

    const post = fetchMock.mock.calls.find(([u]) => u === "/api/marketplace/buy");
    expect(post).toBeDefined();
    expect(JSON.parse(post![1].body as string)).toEqual({
      listing_id: 3,
      recipient: ADDR,
      authorization: AUTH,
    });

    expect(ref).toEqual({ txHash: "0x" + "cd".repeat(32), network: "mantle-sepolia" });
    // No direct-path calls on the happy path.
    expect(mocked.approveUsdc).not.toHaveBeenCalled();
    expect(mocked.buyDirect).not.toHaveBeenCalled();
  });

  it("relay 503 → falls back to approve + buyDirect from the wallet", async () => {
    fetchMock.mockImplementation(async (url: string, init?: RequestInit) => {
      if (url === "/api/marketplace/listings/3") return jsonResponse(indexedListing);
      if (url === "/api/marketplace/buy" && init?.method === "POST") {
        return jsonResponse({ code: "unavailable", message: "relay not configured" }, 503);
      }
      throw new Error(`unexpected fetch: ${url}`);
    });
    mocked.approveUsdc.mockResolvedValue("0xapprove" as `0x${string}`);
    mocked.buyDirect.mockResolvedValue(("0x" + "ee".repeat(32)) as `0x${string}`);

    const api = new ApiMarketplaceData(makeFallback());
    const ref = await api.purchaseIntent("3");

    expect(mocked.approveUsdc).toHaveBeenCalledWith(49_000_000n);
    expect(mocked.buyDirect).toHaveBeenCalledWith(3n, ADDR);
    expect(ref).toEqual({ txHash: "0x" + "ee".repeat(32), network: "mantle-sepolia" });
  });

  it("non-503 relay errors propagate without the direct fallback", async () => {
    fetchMock.mockImplementation(async (url: string, init?: RequestInit) => {
      if (url === "/api/marketplace/listings/3") return jsonResponse(indexedListing);
      if (url === "/api/marketplace/buy" && init?.method === "POST") {
        return jsonResponse({ code: "validation", message: "recipient mismatch" }, 400);
      }
      throw new Error(`unexpected fetch: ${url}`);
    });

    const api = new ApiMarketplaceData(makeFallback());
    await expect(api.purchaseIntent("3")).rejects.toThrow("recipient mismatch");
    expect(mocked.approveUsdc).not.toHaveBeenCalled();
    expect(mocked.buyDirect).not.toHaveBeenCalled();
  });

  it("insufficient balance → typed InsufficientUsdcError carrying the needed amount", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url === "/api/marketplace/listings/3") return jsonResponse(indexedListing);
      throw new Error(`unexpected fetch: ${url}`);
    });
    mocked.usdcBalance.mockResolvedValue(1_000_000n); // 1 USDC < 49

    const api = new ApiMarketplaceData(makeFallback());
    const err = await api.purchaseIntent("3").catch((e) => e);
    expect(err).toBeInstanceOf(InsufficientUsdcError);
    expect((err as InsufficientUsdcError).neededUsdc6).toBe(49_000_000n);
    expect((err as InsufficientUsdcError).balanceUsdc6).toBe(1_000_000n);
    expect(mocked.signTransferAuthorization).not.toHaveBeenCalled();
  });

  it("no connected wallet → typed WalletRequiredError before any network work", async () => {
    mocked.currentAddress.mockResolvedValue(null);

    const api = new ApiMarketplaceData(makeFallback());
    await expect(api.purchaseIntent("3")).rejects.toBeInstanceOf(WalletRequiredError);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("fixture (non-numeric) listing ids delegate to the fallback client", async () => {
    const fallback = makeFallback();
    const spy = vi.spyOn(fallback, "purchaseIntent");
    const api = new ApiMarketplaceData(fallback);

    const ref = await api.purchaseIntent("btc-momentum-v3");
    expect(spy).toHaveBeenCalledWith("btc-momentum-v3");
    expect(ref.network).toBe("mantle-sepolia");
    expect(mocked.currentAddress).not.toHaveBeenCalled();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("ApiMarketplaceData.getReceipt", () => {
  const TX = ("0x" + "ab".repeat(32)) as `0x${string}`;
  const receiptOut = {
    tx_hash: TX,
    listing_id: 3,
    agent_id: "01HXAGENT",
    gen_art_seed: "seed-xyz",
    name: "BTC Momentum",
    content_uri: "ipfs://bafybeibundlecid123",
    buyer: "0x7c2e000000000000000000000000000000000007",
    price_usdc: 49,
    seller_proceeds_usdc: 46.55,
    protocol_proceeds_usdc: 2.45,
    license_token_id: "3",
    purchase_path: 1,
    block_time_unix: 1_750_000_000,
  };

  it("maps the backend receipt into the rich Receipt type with honest empties", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url === `/api/marketplace/receipts/${TX}`) return jsonResponse(receiptOut);
      throw new Error(`unexpected fetch: ${url}`);
    });
    mocked.getContracts.mockResolvedValue({
      marketplace: "0x2222222222222222222222222222222222222222",
      usdc: "0x1111111111111111111111111111111111111111",
      license_token: "0x4444444444444444444444444444444444444444",
      listing_registry: null,
      identity_registry: null,
    });

    const api = new ApiMarketplaceData(makeFallback());
    const r = await api.getReceipt(TX);

    const iso = new Date(1_750_000_000 * 1000).toISOString();
    expect(r.txHash).toBe(TX);
    expect(r.network).toBe("mantle-sepolia");
    expect(r.at).toBe(iso);
    expect(r.buyer).toBe(receiptOut.buyer);

    // Part A (.7): listing.id uses agent_id (ULID) when non-empty.
    expect(r.listing.id).toBe("01HXAGENT");
    expect(r.listing.version).toBe("v1");
    expect(r.listing.creator).toEqual({ address: "" });
    expect(r.listing.genArtSeed).toBe("seed-xyz");
    expect(r.listing.return30dPct).toBe(0);
    expect(r.listing.buyers).toEqual({ humans: 0, agents: 0 });

    expect(r.license.tokenId).toBe("3");
    expect(r.license.contract).toBe("0x4444444444444444444444444444444444444444");
    expect(r.license.manifestHash).toBe("");
    expect(r.license.bundleCid).toBe("bafybeibundlecid123");
    expect(r.license.pricePaidUsdc).toBe(49);
    expect(r.license.feeUsdc).toBe(2.45);
    expect(r.license.netToCreatorUsdc).toBe(46.55);
    expect(r.license.mintedAt).toBe(iso);

    expect(r.install).toEqual({ xvnDetected: false, xvnEndpoint: "", ingredients: [] });
    expect(r.share.variants).toEqual([]);
    // Part A (.7): ogCard.id also uses agent_id (ULID) when non-empty.
    expect(r.share.ogCard.id).toBe("01HXAGENT");
    expect(r.share.ogCard.genArtSeed).toBe("seed-xyz");
    expect(r.share.ogCard.priceUsdc).toBe(49);
  });

  it("maps an xvn:// content_uri to an honest empty bundleCid", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url === `/api/marketplace/receipts/${TX}`)
        return jsonResponse({ ...receiptOut, content_uri: "xvn://strategy/01HXAGENT" });
      throw new Error(`unexpected fetch: ${url}`);
    });
    mocked.getContracts.mockRejectedValue(new Error("not configured"));

    const api = new ApiMarketplaceData(makeFallback());
    const r = await api.getReceipt(TX);
    expect(r.license.bundleCid).toBe("");
  });

  it("keeps an empty license contract when the address book is unavailable", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url === `/api/marketplace/receipts/${TX}`) return jsonResponse(receiptOut);
      throw new Error(`unexpected fetch: ${url}`);
    });
    mocked.getContracts.mockRejectedValue(new Error("not configured"));

    const api = new ApiMarketplaceData(makeFallback());
    const r = await api.getReceipt(TX);
    expect(r.license.contract).toBe("");
  });

  it("non-hex tx ids (fixture hashes) delegate to the fallback client", async () => {
    const fallback = makeFallback();
    const spy = vi.spyOn(fallback, "getReceipt");
    const api = new ApiMarketplaceData(fallback);

    const r = await api.getReceipt("0xdemo-tx");
    expect(spy).toHaveBeenCalledWith("0xdemo-tx");
    expect(r.listing.id).toBe("btc-momentum-v3");
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
