import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  buildTransferAuthTypedData,
  relayBodyFromSignature,
  getContracts,
  getActiveNetworkConfig,
  getActiveNetworkConfigOrDefault,
  getBackendChainId,
  ensureMantleSepolia,
  networkConfig,
  activeNetwork,
  __resetContractsCacheForTest,
  __resetNetworkCacheForTest,
} from "./chain";

const USDC = "0x1111111111111111111111111111111111111111" as const;
const MARKETPLACE = "0x2222222222222222222222222222222222222222" as const;
const FROM = "0x3333333333333333333333333333333333333333" as const;

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  __resetContractsCacheForTest();
});

describe("networkConfig (network selection)", () => {
  it("mainnet → Mantle 5000 with the real USDC.e EIP-712 domain", () => {
    const c = networkConfig("mainnet");
    expect(c.chain.id).toBe(5000);
    expect(c.hex).toBe("0x1388");
    expect(c.usdcDomain).toEqual({ name: "USD Coin", version: "2" });
  });

  it("sepolia → Mantle Sepolia 5003 with the test-USDC domain", () => {
    const c = networkConfig("sepolia");
    expect(c.chain.id).toBe(5003);
    expect(c.hex).toBe("0x138b");
    expect(c.usdcDomain).toEqual({ name: "USD Coin (xvn test)", version: "1" });
  });

  it("defaults to sepolia when VITE_MARKETPLACE_NETWORK is unset", () => {
    // Guards the existing testnet behavior + tests (no mainnet env in CI).
    expect(activeNetwork).toBe("sepolia");
  });
});

describe("buildTransferAuthTypedData", () => {
  // The signing network (chainId + USDC EIP-712 domain) is now supplied by the
  // caller (resolved at runtime from the backend), not read from module consts.
  // These cases pin the testnet domain explicitly.
  const params = {
    from: FROM,
    to: MARKETPLACE,
    usdc: USDC,
    value: 5_000_000n,
    validAfter: 0n,
    validBefore: 1750000000n,
    nonce: ("0x" + "ab".repeat(32)) as `0x${string}`,
    chainId: 5003,
    usdcDomain: { name: "USD Coin (xvn test)", version: "1" },
  };

  it("uses the exact EIP-712 domain for the xvn test USDC", () => {
    const td = buildTransferAuthTypedData(params);
    expect(td.domain).toEqual({
      name: "USD Coin (xvn test)",
      version: "1",
      chainId: 5003,
      verifyingContract: USDC,
    });
  });

  it("declares TransferWithAuthorization fields in the normative typehash order", () => {
    const td = buildTransferAuthTypedData(params);
    expect(td.primaryType).toBe("TransferWithAuthorization");
    expect(td.types.TransferWithAuthorization).toEqual([
      { name: "from", type: "address" },
      { name: "to", type: "address" },
      { name: "value", type: "uint256" },
      { name: "validAfter", type: "uint256" },
      { name: "validBefore", type: "uint256" },
      { name: "nonce", type: "bytes32" },
    ]);
  });

  it("carries the message values verbatim (field names + order)", () => {
    const td = buildTransferAuthTypedData(params);
    expect(Object.keys(td.message)).toEqual([
      "from",
      "to",
      "value",
      "validAfter",
      "validBefore",
      "nonce",
    ]);
    expect(td.message).toEqual({
      from: FROM,
      to: MARKETPLACE,
      value: 5_000_000n,
      validAfter: 0n,
      validBefore: 1750000000n,
      nonce: "0x" + "ab".repeat(32),
    });
  });

  it("uses the MAINNET domain when the caller supplies mainnet config", () => {
    // The signing network is resolved at runtime; on a mainnet backend the
    // EIP-712 domain must be the real USDC.e "USD Coin"/"2"/5000 or the relayed
    // tx reverts.
    const td = buildTransferAuthTypedData({
      ...params,
      chainId: 5000,
      usdcDomain: { name: "USD Coin", version: "2" },
    });
    expect(td.domain).toEqual({
      name: "USD Coin",
      version: "2",
      chainId: 5000,
      verifyingContract: USDC,
    });
  });
});

describe("relayBodyFromSignature", () => {
  const message = {
    from: FROM,
    to: MARKETPLACE,
    value: 5_000_000n,
    validAfter: 0n,
    validBefore: 1750000000n,
    nonce: ("0x" + "ab".repeat(32)) as `0x${string}`,
  };
  const r = "0x" + "11".repeat(32);
  const s = "0x" + "22".repeat(32);

  it("splits a 65-byte signature with trailing 0x1b into r/s/v=27", () => {
    const sig = (r + s.slice(2) + "1b") as `0x${string}`;
    const body = relayBodyFromSignature(message, sig);
    expect(body).toEqual({
      from: FROM,
      to: MARKETPLACE,
      value: "5000000",
      valid_after: 0,
      valid_before: 1750000000,
      nonce: "0x" + "ab".repeat(32),
      v: 27,
      r,
      s,
    });
  });

  it("normalizes a yParity-style trailing byte 0x01 to v=28", () => {
    const sig = (r + s.slice(2) + "01") as `0x${string}`;
    const body = relayBodyFromSignature(message, sig);
    expect(body.v).toBe(28);
  });

  it("normalizes a yParity-style trailing byte 0x00 to v=27", () => {
    const sig = (r + s.slice(2) + "00") as `0x${string}`;
    const body = relayBodyFromSignature(message, sig);
    expect(body.v).toBe(27);
  });

  it("keeps v=28 (0x1c) as-is", () => {
    const sig = (r + s.slice(2) + "1c") as `0x${string}`;
    expect(relayBodyFromSignature(message, sig).v).toBe(28);
  });

  it("serializes value as a decimal 6dp string", () => {
    const sig = (r + s.slice(2) + "1b") as `0x${string}`;
    const body = relayBodyFromSignature(
      { ...message, value: 123_456_789n },
      sig,
    );
    expect(body.value).toBe("123456789");
  });
});

describe("nonce generation (via buildTransferAuthTypedData defaults)", () => {
  it("crypto.getRandomValues nonces are 32-byte hex and unique across calls", async () => {
    // The nonce helper is exercised through signTransferAuthorization's
    // typed-data construction; here we test the exported builder indirectly
    // by importing the internal helper.
    const { randomNonce } = await import("./chain");
    const a = randomNonce();
    const b = randomNonce();
    expect(a).toMatch(/^0x[0-9a-f]{64}$/);
    expect(b).toMatch(/^0x[0-9a-f]{64}$/);
    expect(a).not.toBe(b);
  });
});

describe("getContracts", () => {
  const statusBody = {
    active: true,
    last_poll_unix: 0,
    total_onchain: 0,
    last_error: null,
    contracts: {
      marketplace: MARKETPLACE,
      usdc: USDC,
      license_token: null,
      listing_registry: null,
      identity_registry: null,
    },
  };

  beforeEach(() => {
    __resetContractsCacheForTest();
  });

  it("fetches /api/marketplace/status and returns contracts", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(statusBody), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const contracts = await getContracts();
    expect(contracts.marketplace).toBe(MARKETPLACE);
    expect(contracts.usdc).toBe(USDC);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toBe("/api/marketplace/status");
  });

  it("caches: second call does not refetch", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(statusBody), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    await getContracts();
    await getContracts();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("throws when marketplace/usdc are unset", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          ...statusBody,
          contracts: { ...statusBody.contracts, marketplace: null, usdc: null },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);
    await expect(getContracts()).rejects.toThrow(/not configured/i);
  });
});

describe("getActiveNetworkConfig (runtime network from backend)", () => {
  function stubStatus(network: unknown | undefined) {
    const body: Record<string, unknown> = {
      active: true,
      last_poll_unix: 0,
      total_onchain: 0,
      last_error: null,
      contracts: {
        marketplace: null,
        usdc: null,
        license_token: null,
        listing_registry: null,
        identity_registry: null,
      },
    };
    if (network !== undefined) body.network = network;
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify(body), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
      ),
    );
  }

  beforeEach(() => {
    __resetNetworkCacheForTest();
  });

  it("backend mainnet (chain_id 5000) wins → mainnet chain + USDC domain + slug", async () => {
    stubStatus({ chain_id: 5000, network: "mantle" });
    const net = await getActiveNetworkConfig();
    expect(net.chain.id).toBe(5000);
    expect(net.usdcDomain).toEqual({ name: "USD Coin", version: "2" });
    expect(net.slug).toBe("mantle");
  });

  it("backend testnet (chain_id 5003) → sepolia chain + test-USDC domain", async () => {
    stubStatus({ chain_id: 5003, network: "mantle-sepolia" });
    const net = await getActiveNetworkConfig();
    expect(net.chain.id).toBe(5003);
    expect(net.usdcDomain).toEqual({ name: "USD Coin (xvn test)", version: "1" });
    expect(net.slug).toBe("mantle-sepolia");
  });

  it("backend network null → build-time default (sepolia in CI)", async () => {
    stubStatus(null);
    const net = await getActiveNetworkConfig();
    expect(net.chain.id).toBe(5003);
    expect(net.slug).toBe("mantle-sepolia");
  });

  it("backend network field absent → build-time default", async () => {
    stubStatus(undefined);
    const net = await getActiveNetworkConfig();
    expect(net.chain.id).toBe(5003);
  });

  it("unknown configured chain_id → STRICT throws (never signs a guessed domain)", async () => {
    stubStatus({ chain_id: 1, network: "ethereum" });
    await expect(getActiveNetworkConfig()).rejects.toThrow(
      /unsupported marketplace chain id 1/i,
    );
  });

  it("unknown configured chain_id → LENIENT returns the build-time default", async () => {
    stubStatus({ chain_id: 1, network: "ethereum" });
    const net = await getActiveNetworkConfigOrDefault();
    expect(net.chain.id).toBe(5003);
  });

  it("getBackendChainId returns the raw backend chain id (even when unsupported)", async () => {
    stubStatus({ chain_id: 1, network: "ethereum" });
    expect(await getBackendChainId()).toBe(1);
  });

  it("getBackendChainId returns null when the backend reports no chain", async () => {
    stubStatus(null);
    expect(await getBackendChainId()).toBeNull();
  });

  it("STRICT resolver propagates a status-fetch error (never guesses a domain)", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("offline")));
    await expect(getActiveNetworkConfig()).rejects.toThrow();
  });

  it("LENIENT resolver returns the build-time default on a status-fetch error", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("offline")));
    const net = await getActiveNetworkConfigOrDefault();
    expect(net.chain.id).toBe(5003);
    expect(net.slug).toBe("mantle-sepolia");
  });
});

describe("ensureMantleSepolia (runtime wallet-switch target)", () => {
  beforeEach(() => __resetNetworkCacheForTest());
  afterEach(() => {
    delete (window as unknown as { ethereum?: unknown }).ethereum;
  });

  it("switches the wallet to the BACKEND chain (0x1388 when backend = mainnet 5000)", async () => {
    // Backend reports mainnet; a prebuilt sepolia bundle must NOT switch to 5003.
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            active: true,
            last_poll_unix: 0,
            total_onchain: 0,
            last_error: null,
            contracts: {
              marketplace: null,
              usdc: null,
              license_token: null,
              listing_registry: null,
              identity_registry: null,
            },
            network: { chain_id: 5000, network: "mantle" },
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        ),
      ),
    );
    const request = vi.fn(async (args: { method: string }) =>
      args.method === "eth_chainId" ? "0x1" : null,
    );
    (window as unknown as { ethereum: unknown }).ethereum = { request };

    await ensureMantleSepolia();

    expect(request).toHaveBeenCalledWith({
      method: "wallet_switchEthereumChain",
      params: [{ chainId: "0x1388" }],
    });
  });
});
