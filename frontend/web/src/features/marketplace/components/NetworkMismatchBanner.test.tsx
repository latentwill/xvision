import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NetworkMismatchBanner } from "./NetworkMismatchBanner";
import { __resetNetworkCacheForTest } from "../lib/chain";

// The build (CI) defaults to sepolia (chain 5003). The banner fires when the
// backend reports a DIFFERENT chain.
function stubBackendChain(chainId: number) {
  __resetNetworkCacheForTest();
  // Fresh Response per call (Response bodies are single-read; concurrent
  // mounts may both fetch before the cache is populated).
  const fetchMock = vi.fn(() =>
    Promise.resolve(
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
          network: {
            chain_id: chainId,
            network: chainId === 5000 ? "mantle" : "mantle-sepolia",
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    ),
  );
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

describe("NetworkMismatchBanner", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    __resetNetworkCacheForTest();
  });

  it("warns + renders when the backend chain differs from the build (build 5003 vs backend 5000)", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    stubBackendChain(5000);
    render(<NetworkMismatchBanner />);
    await waitFor(() =>
      expect(screen.getByTestId("network-mismatch-banner")).toBeInTheDocument(),
    );
    expect(screen.getByText(/5000/)).toBeInTheDocument();
    expect(warn).toHaveBeenCalled();
  });

  it("fires even when the backend chain is one the SPA can't resolve (build 5003 vs backend 1)", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    stubBackendChain(1);
    render(<NetworkMismatchBanner />);
    await waitFor(() =>
      expect(screen.getByTestId("network-mismatch-banner")).toBeInTheDocument(),
    );
    expect(screen.getByText(/\b1\b/)).toBeInTheDocument();
    expect(warn).toHaveBeenCalled();
  });

  it("renders nothing when build and backend agree (both 5003)", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const fetchMock = stubBackendChain(5003);
    render(<NetworkMismatchBanner />);
    // Let the effect's status fetch resolve, then confirm no banner / no warn.
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    expect(
      screen.queryByTestId("network-mismatch-banner"),
    ).not.toBeInTheDocument();
    expect(warn).not.toHaveBeenCalled();
  });
});
