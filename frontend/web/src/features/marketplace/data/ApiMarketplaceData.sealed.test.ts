// ApiMarketplaceData.sealed.test.ts — TDD for the sealed-tier import flow
// (fetch bundle → Lit-gated decrypt → import-sealed POST). decryptSealedBundle
// and chain are mocked at the module boundary; apiFetch is mocked directly.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/client";
import { ApiMarketplaceData } from "./ApiMarketplaceData";
import { FixtureMarketplaceData } from "./MarketplaceData";

vi.mock("../lib/chain", () => ({
  activeNetworkSlug: "mantle-sepolia",
  currentAddress: vi.fn(),
}));
vi.mock("../lib/sealed", () => ({
  decryptSealedBundle: vi.fn(),
}));
vi.mock("@/api/client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/api/client")>();
  return { ...actual, apiFetch: vi.fn() };
});

import { apiFetch } from "@/api/client";
import { currentAddress } from "../lib/chain";
import { decryptSealedBundle } from "../lib/sealed";

const mockedApiFetch = vi.mocked(apiFetch);
const mockedAddress = vi.mocked(currentAddress);
const mockedDecrypt = vi.mocked(decryptSealedBundle);

const ADDR = "0x7c2e000000000000000000000000000000000007" as const;
const MANIFEST = { name: "Sealed Strat", agents: [] };
// The server-issued proof decrypt now returns alongside the manifest (lane cgz).
const MESSAGE =
  "xvision sealed-bundle license request\nListing: 9\nNonce: 3f9a1c8e7b2d40563f9a1c8e7b2d4056\nExpiry: 1760000000";
const SIGNATURE = "0x" + "ab".repeat(65);
const DECRYPTED = { manifest: MANIFEST, message: MESSAGE, signature: SIGNATURE };

function client() {
  return new ApiMarketplaceData(new FixtureMarketplaceData());
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("ApiMarketplaceData.importSealed", () => {
  it("fetches the bundle, decrypts, and POSTs the manifest with the address", async () => {
    mockedApiFetch
      .mockResolvedValueOnce({
        listing_id: 9,
        content_uri: "ipfs://x",
        encrypted: true,
        ciphertext: "CIPHER",
        content_hash: "ab".repeat(32),
      }) // bundle
      .mockResolvedValueOnce({ agent_id: "01HSEALEDULID" }); // import-sealed
    mockedDecrypt.mockResolvedValue(DECRYPTED);
    mockedAddress.mockResolvedValue(ADDR);

    const out = await client().importSealed("9");
    expect(out).toEqual({ agent_id: "01HSEALEDULID" });

    expect(mockedApiFetch).toHaveBeenNthCalledWith(
      1,
      "/api/marketplace/listings/9/bundle",
    );
    expect(mockedDecrypt).toHaveBeenCalledWith({ listingId: "9", ciphertext: "CIPHER" });
    // The import POST now carries the server-issued proof (message + signature)
    // so the server can recover the signer and consume the single-use nonce
    // before granting import (lane cgz).
    expect(mockedApiFetch).toHaveBeenNthCalledWith(
      2,
      "/api/marketplace/listings/9/import-sealed",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          address: ADDR,
          manifest: MANIFEST,
          message: MESSAGE,
          signature: SIGNATURE,
        }),
      }),
    );
  });

  it("rejects when the bundle is not encrypted (open tier)", async () => {
    mockedApiFetch.mockResolvedValueOnce({
      listing_id: 9,
      content_uri: "ipfs://x",
      verified: true,
      manifest: {},
    });
    await expect(client().importSealed("9")).rejects.toThrow(/not a sealed bundle/i);
    expect(mockedDecrypt).not.toHaveBeenCalled();
  });

  it("propagates a 403 (no license) from the import-sealed POST", async () => {
    mockedApiFetch
      .mockResolvedValueOnce({ listing_id: 9, content_uri: "", encrypted: true, ciphertext: "C" })
      .mockRejectedValueOnce(new ApiError(403, "forbidden", "no license"));
    mockedDecrypt.mockResolvedValue(DECRYPTED);
    mockedAddress.mockResolvedValue(ADDR);
    await expect(client().importSealed("9")).rejects.toMatchObject({ status: 403 });
  });

  it("propagates a 409 (hash mismatch) from the import-sealed POST", async () => {
    mockedApiFetch
      .mockResolvedValueOnce({ listing_id: 9, content_uri: "", encrypted: true, ciphertext: "C" })
      .mockRejectedValueOnce(new ApiError(409, "conflict", "content hash mismatch"));
    mockedDecrypt.mockResolvedValue(DECRYPTED);
    mockedAddress.mockResolvedValue(ADDR);
    await expect(client().importSealed("9")).rejects.toMatchObject({ status: 409 });
  });

  it("propagates decrypt errors (gate rejection / no wallet) without POSTing", async () => {
    mockedApiFetch.mockResolvedValueOnce({
      listing_id: 9,
      content_uri: "",
      encrypted: true,
      ciphertext: "C",
    });
    mockedDecrypt.mockRejectedValue(new Error("caller does not hold the license NFT"));
    await expect(client().importSealed("9")).rejects.toThrow(/does not hold/i);
    expect(mockedApiFetch).toHaveBeenCalledTimes(1); // only the bundle fetch
  });
});
