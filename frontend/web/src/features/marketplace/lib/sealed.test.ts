import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WalletRequiredError } from "./purchaseErrors";

// chain.ts is mocked so we can drive currentAddress / walletClient without a
// real wallet. mantleSepolia is preserved (real) for the rpc url.
vi.mock("./chain", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./chain")>();
  return {
    ...actual,
    currentAddress: vi.fn(),
    walletClient: vi.fn(),
  };
});
vi.mock("@/api/client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/api/client")>();
  return { ...actual, apiFetch: vi.fn() };
});

import { apiFetch } from "@/api/client";
import { currentAddress, walletClient } from "./chain";
import {
  buildSealedMessage,
  decryptSealedBundle,
  invokeGateAction,
  SealedGateError,
  SealedNotConfiguredError,
  type LitConfig,
} from "./sealed";

const mockedAddress = vi.mocked(currentAddress);
const mockedWalletClient = vi.mocked(walletClient);
const mockedApiFetch = vi.mocked(apiFetch);

const ADDR = "0x7c2e000000000000000000000000000000000007" as const;
const LIT: LitConfig = {
  api_base: "https://lit.example/api",
  gate_action_cid: "QmGateCID",
  pkp_id: "0xPKP",
};
const LICENSE_TOKEN = "0x1155000000000000000000000000000000001155";

function stubStatus(over?: { lit?: LitConfig | null; license_token?: string | null }) {
  mockedApiFetch.mockResolvedValue({
    lit: over?.lit === undefined ? LIT : over.lit,
    contracts: {
      license_token:
        over?.license_token === undefined ? LICENSE_TOKEN : over.license_token,
    },
  });
}

function stubWalletSign(sig = ("0x" + "ab".repeat(65)) as `0x${string}`) {
  const signMessage = vi.fn().mockResolvedValue(sig);
  mockedWalletClient.mockReturnValue({ signMessage } as never);
  return signMessage;
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubEnv("VITE_LIT_CLIENT_KEY", "test-client-key");
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});

// ── buildSealedMessage parity ───────────────────────────────────────────────
describe("buildSealedMessage", () => {
  it("reproduces the gate's canonical message byte-for-byte", () => {
    // Mirrors `buildMessage` in contracts/lit-actions/sealed-gate.test.mjs.
    const expected = [
      "xvision sealed-bundle license request",
      "Listing: 42",
      "Nonce: 3f9a1c8e7b2d4056",
      "Expiry: 1760000000",
    ].join("\n");
    expect(
      buildSealedMessage({ listingId: 42, nonce: "3f9a1c8e7b2d4056", expirySec: 1760000000 }),
    ).toBe(expected);
  });

  it("accepts a string listingId identically", () => {
    expect(
      buildSealedMessage({ listingId: "7", nonce: "abcdef0123456789", expirySec: 1 }),
    ).toBe(
      "xvision sealed-bundle license request\nListing: 7\nNonce: abcdef0123456789\nExpiry: 1",
    );
  });
});

// ── invokeGateAction ────────────────────────────────────────────────────────
describe("invokeGateAction", () => {
  const jsParams = {
    pkpId: "0xPKP",
    ciphertext: "CT",
    address: ADDR,
    message: "m",
    signature: "0xsig",
    listingId: "42",
    nftAddress: LICENSE_TOKEN,
    rpcUrl: "https://rpc",
  };

  it("throws SealedNotConfiguredError when the client key is unset", async () => {
    vi.stubEnv("VITE_LIT_CLIENT_KEY", "");
    await expect(invokeGateAction(LIT, jsParams)).rejects.toBeInstanceOf(
      SealedNotConfiguredError,
    );
  });

  it("POSTs ipfs_id + js_params with X-Api-Key to /core/v1/lit_action (object form)", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      // OBJECT form: response is a JSON object.
      json: async () => ({ response: { plaintext: '{"k":1}' }, has_error: false }),
    });
    vi.stubGlobal("fetch", fetchMock);

    const out = await invokeGateAction(LIT, jsParams);
    expect(out.plaintext).toBe('{"k":1}');
    expect(fetchMock).toHaveBeenCalledWith(
      "https://lit.example/api/core/v1/lit_action",
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({ "x-api-key": "test-client-key" }),
        body: JSON.stringify({ ipfs_id: "QmGateCID", js_params: jsParams }),
      }),
    );
  });

  it("parses the json-string response form (response is a JSON string)", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        // JSON-STRING form: response is a string holding the payload.
        json: async () => ({ response: JSON.stringify({ plaintext: "pt" }), has_error: false }),
      }),
    );
    expect((await invokeGateAction(LIT, jsParams)).plaintext).toBe("pt");
  });

  it("throws SealedGateError when the action returns {error} (object form)", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ response: { error: "no license" }, has_error: false }),
      }),
    );
    await expect(invokeGateAction(LIT, jsParams)).rejects.toMatchObject({
      name: "SealedGateError",
      message: "no license",
    });
  });

  it("throws SealedGateError when the envelope has_error is true", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ response: "boom", logs: "trace", has_error: true }),
      }),
    );
    await expect(invokeGateAction(LIT, jsParams)).rejects.toMatchObject({
      name: "SealedGateError",
    });
  });

  it("throws on a non-2xx HTTP response", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({ ok: false, status: 502, text: async () => "bad gw" }),
    );
    await expect(invokeGateAction(LIT, jsParams)).rejects.toBeInstanceOf(SealedGateError);
  });
});

// ── decryptSealedBundle flow ────────────────────────────────────────────────
describe("decryptSealedBundle", () => {
  it("happy path: signs, invokes the gate, parses the manifest object", async () => {
    stubStatus();
    mockedAddress.mockResolvedValue(ADDR);
    const signMessage = stubWalletSign();
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        response: { plaintext: JSON.stringify({ name: "Strat", agents: [] }) },
        has_error: false,
      }),
    });
    vi.stubGlobal("fetch", fetchMock);

    const manifest = await decryptSealedBundle({ listingId: 42, ciphertext: "CT" });
    expect(manifest).toEqual({ name: "Strat", agents: [] });

    // signed message is listing-bound and uses the canonical header
    const signedMsg = signMessage.mock.calls[0][0].message as string;
    expect(signedMsg).toMatch(/^xvision sealed-bundle license request\nListing: 42\nNonce: /);
    expect(signedMsg).toMatch(/\nExpiry: \d+$/);

    // jsParams carry the ciphertext, license token, pkp, signature
    const body = JSON.parse(fetchMock.mock.calls[0][1].body as string);
    expect(body.ipfs_id).toBe("QmGateCID");
    expect(body.js_params).toMatchObject({
      pkpId: "0xPKP",
      ciphertext: "CT",
      address: ADDR,
      nftAddress: LICENSE_TOKEN,
      listingId: "42",
    });
  });

  it("throws WalletRequiredError when no wallet is connected", async () => {
    stubStatus();
    mockedAddress.mockResolvedValue(null);
    await expect(
      decryptSealedBundle({ listingId: 1, ciphertext: "CT" }),
    ).rejects.toBeInstanceOf(WalletRequiredError);
  });

  it("throws SealedNotConfiguredError when status.lit is null", async () => {
    stubStatus({ lit: null });
    mockedAddress.mockResolvedValue(ADDR);
    await expect(
      decryptSealedBundle({ listingId: 1, ciphertext: "CT" }),
    ).rejects.toBeInstanceOf(SealedNotConfiguredError);
  });

  it("throws SealedNotConfiguredError when the client key is unset", async () => {
    vi.stubEnv("VITE_LIT_CLIENT_KEY", "");
    stubStatus();
    mockedAddress.mockResolvedValue(ADDR);
    stubWalletSign();
    await expect(
      decryptSealedBundle({ listingId: 1, ciphertext: "CT" }),
    ).rejects.toBeInstanceOf(SealedNotConfiguredError);
  });

  it("propagates a gate {error} as SealedGateError", async () => {
    stubStatus();
    mockedAddress.mockResolvedValue(ADDR);
    stubWalletSign();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          response: { error: "caller does not hold the license NFT" },
          has_error: false,
        }),
      }),
    );
    await expect(
      decryptSealedBundle({ listingId: 1, ciphertext: "CT" }),
    ).rejects.toMatchObject({ name: "SealedGateError" });
  });

  it("rejects a decrypted payload that is not a JSON object", async () => {
    stubStatus();
    mockedAddress.mockResolvedValue(ADDR);
    stubWalletSign();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ response: { plaintext: "[1,2]" }, has_error: false }),
      }),
    );
    await expect(
      decryptSealedBundle({ listingId: 1, ciphertext: "CT" }),
    ).rejects.toBeInstanceOf(SealedGateError);
  });
});
