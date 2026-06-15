// src/features/marketplace/routes/MyListingsRoute.test.tsx
import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { MyListingsRoute } from "./MyListingsRoute";

vi.mock("@/features/marketplace/lib/wallet", () => ({
  useWallet: () => mockWallet,
}));

const mockWallet = {
  address: null as string | null,
};

const ADDR = "0x1111222233334444555566667777888899990000";

const walletPayload = {
  address: ADDR,
  strategies: [],
  licenses: [],
  listings: [
    {
      listing_id: 7,
      agent_nft_id: "12",
      agent_id: "01HXAGENT",
      seller: ADDR,
      content_hash: "0xhash",
      content_uri: "ipfs://x",
      tier: 1,
      price_usdc: 49,
      transferable_license: true,
      revoked: false,
      gen_art_seed: "seed-1",
      name: "My Strategy",
      symmetry: "radial",
      palette: "gold",
      attestation_count: 0,
      units_sold: 0,
      earned_usdc: 0,
    },
    {
      listing_id: 8,
      agent_nft_id: "13",
      agent_id: "01HXAGENT2",
      seller: ADDR,
      content_hash: "0xhash2",
      content_uri: "ipfs://y",
      tier: 0,
      price_usdc: 0,
      transferable_license: false,
      revoked: true,
      gen_art_seed: "seed-2",
      name: "Old Revoked",
      symmetry: "grid",
      palette: "ice",
      attestation_count: 0,
      units_sold: 0,
      earned_usdc: 0,
    },
  ],
};

function renderRoute() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
        <MemoryRouter initialEntries={["/marketplace/mine"]}>
          <MyListingsRoute />
        </MemoryRouter>
      </MarketplaceDataProvider>
    </QueryClientProvider>,
  );
}

describe("MyListingsRoute", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    mockWallet.address = null;
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("disconnected: shows connect prompt, no fetch", () => {
    renderRoute();
    expect(screen.getByText(/connect a wallet/i)).toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("connected: renders all listings (active + revoked) with OwnerListingCard", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(walletPayload), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    renderRoute();
    expect(await screen.findByText("My Strategy")).toBeInTheDocument();
    expect(screen.getByText("Old Revoked")).toBeInTheDocument();
    // Active listing: edit price button visible
    expect(screen.getByRole("button", { name: /edit price/i })).toBeInTheDocument();
    // Revoked listing: shows revoked badge, no revoke button for that one
    expect(screen.getByText(/^revoked$/i)).toBeInTheDocument();
    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        `/api/marketplace/wallet/${ADDR}`,
        expect.anything(),
      ),
    );
  });

  it("shows empty state when wallet has no listings", async () => {
    mockWallet.address = ADDR;
    fetchMock.mockResolvedValue(
      new Response(
        JSON.stringify({ address: ADDR, strategies: [], licenses: [], listings: [] }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );
    renderRoute();
    expect(await screen.findByText(/no listings published/i)).toBeInTheDocument();
  });

  it("shows the My Listings title", () => {
    mockWallet.address = ADDR;
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(walletPayload), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    renderRoute();
    expect(screen.getByRole("heading", { name: /my listings/i })).toBeInTheDocument();
  });
});
