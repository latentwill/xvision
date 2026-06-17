// src/features/marketplace/routes/WalletRoute.test.tsx
// Task 5 — wallet page: owned strategies, licenses, listing management.
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { WalletRoute } from "./WalletRoute";

// ── useWallet mock ───────────────────────────────────────────────────────────
const mockWallet = {
  address: null as string | null,
  connecting: false,
  connect: vi.fn(async () => {}),
  disconnect: vi.fn(),
};
vi.mock("@/features/marketplace/lib/wallet", () => ({
  useWallet: () => mockWallet,
}));

// ── chain.ts mock — jsdom has no wallet/RPC; balance + faucet are stubbed ────
const usdcBalanceMock = vi.fn<(addr: string) => Promise<bigint>>();
const faucetUsdcMock = vi.fn<(amount6: bigint) => Promise<string>>();
vi.mock("@/features/marketplace/lib/chain", () => ({
  activeNetworkSlug: "mantle-sepolia",
  usdcBalance: (addr: string) => usdcBalanceMock(addr),
  faucetUsdc: (amount6: bigint) => faucetUsdcMock(amount6),
}));

const ADDR = "0x1111222233334444555566667777888899990000";

const walletPayload = {
  address: ADDR,
  strategies: [
    {
      token_id: "12",
      agent_id: "01HZX4AGENTID",
      name: "btc-momentum-v3",
      gen_art_seed: "seed-strat-1",
      listed: true,
      listing_id: 7,
    },
    {
      token_id: "13",
      agent_id: "01HZX4OTHER",
      name: "unlisted-strat",
      gen_art_seed: "seed-strat-2",
      listed: false,
      listing_id: null,
    },
  ],
  licenses: [
    {
      listing_id: 3,
      agent_id: "01JLICENSE",
      name: "eth-swing-v1",
      gen_art_seed: "seed-lic-1",
      balance: 2,
    },
  ],
  listings: [
    {
      listing_id: 7,
      agent_nft_id: "12",
      agent_id: "01HZX4AGENTID",
      seller: ADDR,
      content_hash: "0xhash",
      content_uri: "ipfs://x",
      tier: 1,
      price_usdc: 49,
      transferable_license: true,
      revoked: false,
      gen_art_seed: "seed-strat-1",
      name: "btc-momentum-v3",
      symmetry: "radial",
      palette: "gold",
      attestation_count: 0,
      units_sold: 2,
      earned_usdc: 1.9,
    },
    {
      listing_id: 8,
      agent_nft_id: "14",
      agent_id: "01HZX4DEAD",
      seller: ADDR,
      content_hash: "0xhash2",
      content_uri: "ipfs://y",
      tier: 0,
      price_usdc: 0,
      transferable_license: true,
      revoked: true,
      gen_art_seed: "seed-strat-3",
      name: "old-revoked",
      symmetry: "grid",
      palette: "ice",
      attestation_count: 0,
      units_sold: 0,
      earned_usdc: 0,
    },
  ],
};

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  mockWallet.address = null;
  mockWallet.connecting = false;
  mockWallet.connect.mockClear();
  mockWallet.disconnect.mockClear();
  fetchMock = vi.fn();
  vi.stubGlobal("fetch", fetchMock);
  usdcBalanceMock.mockReset();
  usdcBalanceMock.mockResolvedValue(0n);
  faucetUsdcMock.mockReset();
  faucetUsdcMock.mockResolvedValue("0xfaucet-tx");
});

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
});

function renderRoute() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
        <MemoryRouter initialEntries={["/marketplace/wallet"]}>
          <WalletRoute />
        </MemoryRouter>
      </MarketplaceDataProvider>
    </QueryClientProvider>,
  );
}

describe("WalletRoute", () => {
  it("disconnected: renders connect button and fires no fetch", () => {
    renderRoute();
    expect(
      screen.getByRole("button", { name: /connect wallet/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/connect a wallet/i)).toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("connected: renders strategies, licenses, and listings from payload", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockResolvedValue(jsonResponse(walletPayload));
    renderRoute();

    // wallet endpoint hit with the connected address
    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        `/api/marketplace/wallet/${ADDR}`,
        expect.anything(),
      ),
    );

    // strip shows truncated address + disconnect
    expect(await screen.findByText(/0x1111…0000/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /disconnect/i }),
    ).toBeInTheDocument();

    // strategies section: name, token id chip, listed chip linking to listing
    // (name also appears in the listings table row — allow multiple)
    const names = await screen.findAllByText("btc-momentum-v3");
    expect(names.length).toBeGreaterThan(0);
    expect(screen.getByText("unlisted-strat")).toBeInTheDocument();
    expect(screen.getByText("#12")).toBeInTheDocument();
    const listedChip = screen.getByRole("link", { name: /listed/i });
    expect(listedChip).toHaveAttribute("href", "/marketplace/lineage/7");

    // licenses section: name + balance chip
    expect(screen.getByText("eth-swing-v1")).toBeInTheDocument();
    expect(screen.getByText("×2")).toBeInTheDocument();

    // listings section: price, tier, status
    expect(screen.getByText("49 USDC")).toBeInTheDocument();
    expect(screen.getByText(/sealed/i)).toBeInTheDocument();
    expect(screen.getByText("free")).toBeInTheDocument();
    expect(screen.getByText(/^revoked$/i)).toBeInTheDocument();
    expect(screen.getByText(/^active$/i)).toBeInTheDocument();
  });

  it("503 from wallet endpoint shows indexer-offline notice", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockResolvedValue(
      jsonResponse({ code: "unavailable", message: "indexer dormant" }, 503),
    );
    renderRoute();
    expect(
      await screen.findByText(/marketplace indexer offline/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/XVN_RPC_URL/)).toBeInTheDocument();
  });

  it("revoke flow: inline confirm, POSTs revoke, then refetches wallet", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockImplementation(
      async (url: string, init?: RequestInit) => {
        if (
          url === "/api/marketplace/listings/7/revoke" &&
          init?.method === "POST"
        ) {
          return jsonResponse({ listing_id: 7, tx_hash: "0xrevoked" });
        }
        return jsonResponse(walletPayload);
      },
    );
    const user = userEvent.setup();
    renderRoute();

    const revokeBtn = await screen.findByRole("button", { name: /^revoke$/i });
    await user.click(revokeBtn);

    // inline two-step confirm appears — no dialog
    expect(screen.getByText(/confirm revoke\?/i)).toBeInTheDocument();
    const yes = screen.getByRole("button", { name: /yes/i });
    expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument();

    await user.click(yes);

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        "/api/marketplace/listings/7/revoke",
        expect.objectContaining({ method: "POST" }),
      ),
    );

    // refetch: wallet GET fired at least twice (initial + post-revoke)
    await waitFor(() => {
      const walletGets = fetchMock.mock.calls.filter(
        ([url]) => url === `/api/marketplace/wallet/${ADDR}`,
      );
      expect(walletGets.length).toBeGreaterThanOrEqual(2);
    });
  });

  it("revoke cancel restores the plain revoke button without POSTing", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockResolvedValue(jsonResponse(walletPayload));
    const user = userEvent.setup();
    renderRoute();

    await user.click(await screen.findByRole("button", { name: /^revoke$/i }));
    await user.click(screen.getByRole("button", { name: /cancel/i }));

    expect(screen.queryByText(/confirm revoke\?/i)).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /^revoke$/i }),
    ).toBeInTheDocument();
    expect(
      fetchMock.mock.calls.filter(([, init]) => init?.method === "POST"),
    ).toHaveLength(0);
  });

  it("connected: shows the USDC balance line with a faucet button", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockResolvedValue(jsonResponse(walletPayload));
    usdcBalanceMock.mockResolvedValue(12_340_000n); // 12.34 USDC

    renderRoute();

    expect(await screen.findByText("12.34 USDC")).toBeInTheDocument();
    expect(usdcBalanceMock).toHaveBeenCalledWith(ADDR);
    expect(
      screen.getByRole("button", { name: /get test usdc/i }),
    ).toBeInTheDocument();
  });

  it("faucet click mints test USDC and refetches the balance", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockResolvedValue(jsonResponse(walletPayload));
    usdcBalanceMock.mockResolvedValue(0n);

    const user = userEvent.setup();
    renderRoute();

    await user.click(
      await screen.findByRole("button", { name: /get test usdc/i }),
    );

    await waitFor(() =>
      expect(faucetUsdcMock).toHaveBeenCalledWith(10_000_000n),
    );
    // invalidation refetches the balance (initial + post-faucet)
    await waitFor(() =>
      expect(usdcBalanceMock.mock.calls.length).toBeGreaterThanOrEqual(2),
    );
  });

  it("disconnected: no USDC balance line or faucet button", () => {
    renderRoute();
    expect(screen.queryByTestId("usdc-balance")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /get test usdc/i }),
    ).not.toBeInTheDocument();
  });

  it("earnings chip shows units sold and earned USDC only when units_sold > 0", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockResolvedValue(jsonResponse(walletPayload));
    renderRoute();

    // listing 7 sold 2 for 1.90 USDC; revoked listing 8 sold 0 → single chip
    expect(await screen.findByText("sold ×2 · $1.90 earned")).toBeInTheDocument();
    expect(screen.getAllByText(/earned/)).toHaveLength(1);
  });

  it("republish flow: inline confirm, POSTs update, shows new content_uri, refetches", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockImplementation(async (url: string, init?: RequestInit) => {
      if (
        url === "/api/marketplace/listings/7/update" &&
        init?.method === "POST"
      ) {
        return jsonResponse({
          listing_id: 7,
          content_hash: "0xnewhash",
          content_uri: "ipfs://bafy-new",
          tx_hash: "0xupdated",
        });
      }
      return jsonResponse(walletPayload);
    });
    const user = userEvent.setup();
    renderRoute();

    await user.click(
      await screen.findByRole("button", { name: /republish content/i }),
    );

    // inline two-step confirm — no dialog
    expect(screen.getByText(/confirm republish\?/i)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^yes$/i }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        "/api/marketplace/listings/7/update",
        expect.objectContaining({ method: "POST" }),
      ),
    );

    // success: new content_uri rendered inline
    expect(await screen.findByText(/ipfs:\/\/bafy-new/)).toBeInTheDocument();

    // refetch: wallet GET fired at least twice (initial + post-update)
    await waitFor(() => {
      const walletGets = fetchMock.mock.calls.filter(
        ([url]) => url === `/api/marketplace/wallet/${ADDR}`,
      );
      expect(walletGets.length).toBeGreaterThanOrEqual(2);
    });
  });

  it("republish cancel restores the button without POSTing", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockResolvedValue(jsonResponse(walletPayload));
    const user = userEvent.setup();
    renderRoute();

    await user.click(
      await screen.findByRole("button", { name: /republish content/i }),
    );
    await user.click(screen.getByRole("button", { name: /cancel/i }));

    expect(screen.queryByText(/confirm republish\?/i)).not.toBeInTheDocument();
    expect(
      fetchMock.mock.calls.filter(([, init]) => init?.method === "POST"),
    ).toHaveLength(0);
  });

  it("republish failure renders an inline error", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockImplementation(async (url: string, init?: RequestInit) => {
      if (
        url === "/api/marketplace/listings/7/update" &&
        init?.method === "POST"
      ) {
        return jsonResponse(
          { code: "validation", message: "NotSeller" },
          400,
        );
      }
      return jsonResponse(walletPayload);
    });
    const user = userEvent.setup();
    renderRoute();

    await user.click(
      await screen.findByRole("button", { name: /republish content/i }),
    );
    await user.click(screen.getByRole("button", { name: /^yes$/i }));

    expect(
      await screen.findByText(/republish failed.*NotSeller/i),
    ).toBeInTheDocument();
  });

  it("attest flow: inline expander POSTs cycles + sharpe, shows truncated tx", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockImplementation(async (url: string, init?: RequestInit) => {
      if (
        url === "/api/marketplace/listings/7/attest" &&
        init?.method === "POST"
      ) {
        return jsonResponse({ tx_hash: "0xattesttxhash1234567890" }, 201);
      }
      return jsonResponse(walletPayload);
    });
    const user = userEvent.setup();
    renderRoute();

    await user.click(
      await screen.findByRole("button", { name: /post attestation/i }),
    );

    // inline mini-form, submit disabled until both fields are valid
    const submit = screen.getByRole("button", { name: /^attest$/i });
    expect(submit).toBeDisabled();
    await user.type(screen.getByRole("spinbutton", { name: /cycles/i }), "20");
    await user.type(
      screen.getByRole("spinbutton", { name: /sharpe/i }),
      "1.5",
    );
    expect(submit).not.toBeDisabled();
    await user.click(submit);

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        "/api/marketplace/listings/7/attest",
        expect.objectContaining({
          method: "POST",
          body: JSON.stringify({ cycles: 20, sharpe: 1.5 }),
        }),
      ),
    );

    // success: truncated tx hash inline
    expect(await screen.findByText(/0xatte…7890/)).toBeInTheDocument();
  });

  it("attest failure renders an inline error", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockImplementation(async (url: string, init?: RequestInit) => {
      if (
        url === "/api/marketplace/listings/7/attest" &&
        init?.method === "POST"
      ) {
        return jsonResponse(
          { code: "unavailable", message: "signer not configured" },
          503,
        );
      }
      return jsonResponse(walletPayload);
    });
    const user = userEvent.setup();
    renderRoute();

    await user.click(
      await screen.findByRole("button", { name: /post attestation/i }),
    );
    await user.type(screen.getByRole("spinbutton", { name: /cycles/i }), "20");
    await user.type(
      screen.getByRole("spinbutton", { name: /sharpe/i }),
      "1.5",
    );
    await user.click(screen.getByRole("button", { name: /^attest$/i }));

    expect(
      await screen.findByText(/attestation failed.*signer not configured/i),
    ).toBeInTheDocument();
  });

  it("empty sections render muted empty states", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockResolvedValue(
      jsonResponse({ address: ADDR, strategies: [], licenses: [], listings: [] }),
    );
    renderRoute();
    expect(
      await screen.findByText(/no strategies owned/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/no licenses/i)).toBeInTheDocument();
    expect(screen.getByText(/no listings/i)).toBeInTheDocument();
  });
});
