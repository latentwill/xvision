// Tests for useDeployDegenArena — the POST /api/live/deploy/degen-arena wrapper.
//
// Security assertion: the API key is sent in the request body to the server
// but is NEVER rendered to the DOM. Since this module returns a plain async
// function (not a React hook with DOM output), we verify the security property
// by asserting the key appears in the serialised fetch body and is NOT present
// in `document.body.textContent` after the call.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DeployDegenArenaError, useDeployDegenArena } from "./useDeployDegenArena";

// A valid 0x-prefixed 64-hex private key (66 chars: "0x" + 64 hex).
const VALID_KEY =
  "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
// A valid EVM account address (42 chars: "0x" + 40 hex).
const VALID_ADDR = "0xAbCdEf1234567890AbCdEf1234567890AbCdEf12";

const BASE_PAYLOAD = {
  apiKey: VALID_KEY,
  accountAddress: VALID_ADDR,
  network: "testnet" as const,
};

// --------------------------------------------------------------------------
// fetch mock helpers
// --------------------------------------------------------------------------

type FetchMock = ReturnType<typeof vi.fn>;

function mockFetchSuccess(): FetchMock {
  const mock = vi.fn().mockResolvedValue({
    ok: true,
    status: 200,
    json: async () => ({ ok: true }),
  });
  vi.stubGlobal("fetch", mock);
  return mock;
}

function mockFetchError(status: number, message: string): FetchMock {
  const mock = vi.fn().mockResolvedValue({
    ok: false,
    status,
    json: async () => ({ message }),
  });
  vi.stubGlobal("fetch", mock);
  return mock;
}

function mockFetchErrorNoJson(status: number): FetchMock {
  const mock = vi.fn().mockResolvedValue({
    ok: false,
    status,
    json: async () => { throw new SyntaxError("not json"); },
  });
  vi.stubGlobal("fetch", mock);
  return mock;
}

beforeEach(() => {
  vi.unstubAllGlobals();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

// --------------------------------------------------------------------------
// Success path
// --------------------------------------------------------------------------

describe("useDeployDegenArena — success path", () => {
  it("POSTs to /api/live/deploy/degen-arena", async () => {
    const fetchMock = mockFetchSuccess();
    const deploy = useDeployDegenArena();

    await deploy(BASE_PAYLOAD);

    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("/api/live/deploy/degen-arena");
    expect((init.method ?? "").toUpperCase()).toBe("POST");
  });

  it("sends apiKey, accountAddress, and network in the JSON body", async () => {
    const fetchMock = mockFetchSuccess();
    const deploy = useDeployDegenArena();

    await deploy(BASE_PAYLOAD);

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    const body = JSON.parse(init.body as string) as Record<string, unknown>;

    expect(body.apiKey).toBe(VALID_KEY);
    expect(body.accountAddress).toBe(VALID_ADDR);
    expect(body.network).toBe("testnet");
  });

  it("sends network: 'mainnet' when mainnet is selected", async () => {
    const fetchMock = mockFetchSuccess();
    const deploy = useDeployDegenArena();

    await deploy({ ...BASE_PAYLOAD, network: "mainnet" });

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    const body = JSON.parse(init.body as string) as Record<string, unknown>;
    expect(body.network).toBe("mainnet");
  });

  it("resolves with { ok: true } on success", async () => {
    mockFetchSuccess();
    const deploy = useDeployDegenArena();

    const result = await deploy(BASE_PAYLOAD);
    expect(result).toEqual({ ok: true });
  });

  it("does NOT render the API key to the DOM after a successful call", async () => {
    mockFetchSuccess();
    const deploy = useDeployDegenArena();

    await deploy(BASE_PAYLOAD);

    // The key must never appear in any DOM node's text content.
    const domText = document.body.textContent ?? "";
    expect(domText).not.toContain(VALID_KEY);
  });
});

// --------------------------------------------------------------------------
// Error path
// --------------------------------------------------------------------------

describe("useDeployDegenArena — error path", () => {
  it("throws DeployDegenArenaError with the server message on 4xx", async () => {
    mockFetchError(422, "accountAddress is invalid");
    const deploy = useDeployDegenArena();

    await expect(deploy(BASE_PAYLOAD)).rejects.toMatchObject({
      name: "DeployDegenArenaError",
      status: 422,
      message: "accountAddress is invalid",
    });
  });

  it("throws DeployDegenArenaError on 5xx with an HTTP fallback when body has no message", async () => {
    mockFetchErrorNoJson(500);
    const deploy = useDeployDegenArena();

    await expect(deploy(BASE_PAYLOAD)).rejects.toMatchObject({
      name: "DeployDegenArenaError",
      status: 500,
    });
    // Message falls back to the "HTTP 500" sentinel.
    await expect(deploy(BASE_PAYLOAD)).rejects.toHaveProperty(
      "message",
      "HTTP 500",
    );
  });

  it("is an instance of DeployDegenArenaError and Error", async () => {
    mockFetchError(403, "forbidden");
    const deploy = useDeployDegenArena();

    try {
      await deploy(BASE_PAYLOAD);
      expect.fail("should have thrown");
    } catch (err) {
      expect(err).toBeInstanceOf(DeployDegenArenaError);
      expect(err).toBeInstanceOf(Error);
    }
  });

  it("does NOT render the API key to the DOM after a failed call", async () => {
    mockFetchError(422, "bad payload");
    const deploy = useDeployDegenArena();

    try {
      await deploy(BASE_PAYLOAD);
    } catch {
      // Expected to throw.
    }

    const domText = document.body.textContent ?? "";
    expect(domText).not.toContain(VALID_KEY);
  });
});
