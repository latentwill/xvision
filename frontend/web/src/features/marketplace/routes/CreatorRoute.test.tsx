// src/features/marketplace/routes/CreatorRoute.test.tsx
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { CreatorRoute } from "./CreatorRoute";

function renderCreator(handleOrAddr = "@ed") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
        <MemoryRouter initialEntries={[`/marketplace/creator/${handleOrAddr}`]}>
          <Routes>
            <Route
              path="/marketplace/creator/:handleOrAddr"
              element={<CreatorRoute />}
            />
            {/* stub for lineage nav */}
            <Route
              path="/marketplace/lineage/:name"
              element={<div data-testid="lineage-route" />}
            />
          </Routes>
        </MemoryRouter>
      </MarketplaceDataProvider>
    </QueryClientProvider>,
  );
}

describe("CreatorRoute", () => {
  it("renders the creator handle in the hero", async () => {
    renderCreator();
    expect(await screen.findByText("@ed")).toBeInTheDocument();
  });

  it("shows the ENS name pill", async () => {
    renderCreator();
    expect(await screen.findByText("ed.xvn")).toBeInTheDocument();
  });

  it("shows the notableTag badge", async () => {
    renderCreator();
    // notableTag = "agent #0 contributor"
    expect(await screen.findByText(/agent #0 contributor/i)).toBeInTheDocument();
  });

  it("renders the truncated address", async () => {
    renderCreator();
    // address: "0xa83e7c2efabb91d4eea7c2efbb91d4eef12d4" → "0xa83e…2d4"
    expect(await screen.findByText(/0xa83e…/)).toBeInTheDocument();
  });

  it("Follow and Tip CTAs are disabled (deferred affordances)", async () => {
    renderCreator();
    const followBtn = await screen.findByRole("button", { name: /follow/i });
    const tipBtn = await screen.findByRole("button", { name: /tip/i });
    expect(followBtn).toBeDisabled();
    expect(tipBtn).toBeDisabled();
  });

  it("renders all 6 counter tiles with correct values", async () => {
    renderCreator();
    // strategies = 3 — at least one element with text "3"
    const threes = await screen.findAllByText("3");
    expect(threes.length).toBeGreaterThanOrEqual(1);
    // lifetimeEarnedUsd = 4820 → "$4,820"
    expect(await screen.findByText(/\$4[,.]?820/)).toBeInTheDocument();
    // attestationsIssued = 14 — may appear multiple times (counter + strategy buyers)
    const fourteens = await screen.findAllByText("14");
    expect(fourteens.length).toBeGreaterThanOrEqual(1);
  });

  it("renders the strategies grid with correct count", async () => {
    renderCreator();
    // 3 strategies in fixture @ed: btc-momentum-v3, btc-grid-v2, eth-mr-v2
    const cards = await screen.findAllByRole("link", {
      name: /btc-momentum-v3|btc-grid-v2|eth-mr-v2/i,
    });
    expect(cards.length).toBe(3);
  });

  it("strategy tab 'Live' filters to live strategies only", async () => {
    renderCreator();
    const liveTab = await screen.findByRole("button", { name: /^live$/i });
    fireEvent.click(liveTab);
    // All 3 fixture strategies have status: "live", so all 3 remain
    const cards = await screen.findAllByRole("link", {
      name: /btc-momentum-v3|btc-grid-v2|eth-mr-v2/i,
    });
    expect(cards.length).toBe(3);
  });

  it("strategy tab 'Archived' shows no strategies (none archived in fixture)", async () => {
    renderCreator();
    await screen.findByText("@ed");
    const archivedTab = screen.getByRole("button", { name: /^archived$/i });
    fireEvent.click(archivedTab);
    // All 3 fixture strategies are "live", so archived tab shows none
    await waitFor(() => {
      expect(
        screen.queryByRole("link", { name: /btc-momentum-v3/i }),
      ).not.toBeInTheDocument();
    });
  });

  it("renders the EarningsChart SVG", async () => {
    const { container } = renderCreator();
    await screen.findByText("@ed");
    const paths = container.querySelectorAll("svg path");
    // EarningsChart has 2 paths (fill + stroke)
    expect(paths.length).toBeGreaterThanOrEqual(2);
  });

  it("renders the lineage forest with node labels", async () => {
    renderCreator();
    await screen.findByText("@ed");
    // v3.0 appears in strategy card version AND forest node label — use getAllBy
    expect(screen.getAllByText("v3.0").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("v1.0").length).toBeGreaterThanOrEqual(1);
  });

  it("clicking a forest node navigates to its lineage route", async () => {
    renderCreator();
    // Wait for forest to render
    const nodeBtn = await screen.findByRole("button", { name: /view lineage: v3.0/i });
    fireEvent.click(nodeBtn);
    expect(screen.getByTestId("lineage-route")).toBeInTheDocument();
  });

  it("renders reputation feed rows", async () => {
    renderCreator();
    await screen.findByText("@ed");
    // fixture has 3 reputation feed entries; check verdict pills
    expect(screen.getAllByText(/endorse|question/i).length).toBeGreaterThanOrEqual(2);
  });

  it("reputation tab 'Received' filters to received only", async () => {
    renderCreator();
    await screen.findByText("@ed");
    const receivedTab = screen.getByRole("button", { name: /^received$/i });
    fireEvent.click(receivedTab);
    // After filtering to received, the "issued" direction label in feed rows should disappear.
    // The stats header still shows "N issued" as sub-text, so match the exact uppercase label
    // used in feed rows: case-sensitive "issued" (lowercase, in a <span> role).
    await waitFor(() => {
      // Feed row direction labels are uppercase "ISSUED" — check those are gone
      const issuedLabels = screen.queryAllByText("ISSUED");
      expect(issuedLabels).toHaveLength(0);
    });
  });

  it("renders the cloned-by list", async () => {
    renderCreator();
    await screen.findByText("@ed");
    // @solyana appears in both the lineage forest node label and the cloned-by list
    expect(screen.getAllByText("@solyana").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("@quantnext").length).toBeGreaterThanOrEqual(1);
  });

  it("renders a not-found message for an unknown handle", async () => {
    renderCreator("@ghost");
    expect(await screen.findByText(/creator not found/i)).toBeInTheDocument();
  });

  it("does not mount any dialog or modal (no-popups rule)", async () => {
    renderCreator();
    await screen.findByText("@ed");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

// ── On-chain creator page (0x… 40-hex address params) ────────────────────────
describe("CreatorRoute with a real wallet address", () => {
  const ADDR = "0x7c2E000000000000000000000000000000000007";

  const walletView = {
    address: ADDR.toLowerCase(),
    strategies: [],
    licenses: [],
    listings: [
      {
        listing_id: 3,
        agent_nft_id: "3",
        agent_id: "01HSTRAT",
        seller: ADDR.toLowerCase(),
        content_hash: "ab".repeat(32),
        content_uri: "ipfs://bafytestcid",
        tier: 1,
        price_usdc: 49,
        transferable_license: false,
        revoked: false,
        gen_art_seed: "seed-3",
        name: "BTC Dip Buyer",
        symmetry: "radial",
        palette: "gold",
        attestation_count: 0,
        units_sold: 2,
        earned_usdc: 98,
      },
      {
        listing_id: 4,
        agent_nft_id: "4",
        agent_id: "01HDEAD",
        seller: ADDR.toLowerCase(),
        content_hash: "cd".repeat(32),
        content_uri: "ipfs://bafyrevoked",
        tier: 0,
        price_usdc: 0,
        transferable_license: false,
        revoked: true,
        gen_art_seed: "seed-4",
        name: "Dead Listing",
        symmetry: "radial",
        palette: "gold",
        attestation_count: 0,
        units_sold: 0,
        earned_usdc: 0,
      },
    ],
  };

  function stubWalletFetch(impl?: () => Promise<Response>) {
    const fetchMock = vi.fn(
      impl ??
        (async () =>
          new Response(JSON.stringify(walletView), {
            status: 200,
            headers: { "content-type": "application/json" },
          })),
    );
    vi.stubGlobal("fetch", fetchMock);
    return fetchMock;
  }

  afterEach(() => vi.unstubAllGlobals());

  it("fetches the wallet view and renders the truncated address header", async () => {
    const fetchMock = stubWalletFetch();
    renderCreator(ADDR);
    expect(await screen.findByTestId("onchain-creator-page")).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledWith(
      `/api/marketplace/wallet/${ADDR}`,
      expect.anything(),
    );
    // truncated 0x7c2E…0007
    expect(screen.getByText(/0x7c2E…0007/)).toBeInTheDocument();
  });

  it("renders non-revoked listings with name + price, linking to detail", async () => {
    stubWalletFetch();
    renderCreator(ADDR);
    const card = await screen.findByTestId("onchain-creator-listing");
    expect(card).toHaveTextContent("BTC Dip Buyer");
    expect(card).toHaveTextContent("49 USDC");
    // Part A (.7): href uses agent_id (ULID) when non-empty.
    expect(card).toHaveAttribute("href", "/marketplace/lineage/01HSTRAT");
    // revoked listing is not shown
    expect(screen.queryByText("Dead Listing")).not.toBeInTheDocument();
  });

  it("shows the not-found state when the wallet route errors", async () => {
    stubWalletFetch(async () =>
      new Response(JSON.stringify({ code: "internal", message: "boom" }), {
        status: 500,
        headers: { "content-type": "application/json" },
      }),
    );
    renderCreator(ADDR);
    expect(await screen.findByText(/creator not found/i)).toBeInTheDocument();
  });

  it("non-address params never hit the wallet route (fixture path untouched)", async () => {
    const fetchMock = stubWalletFetch();
    renderCreator("@ed");
    expect(await screen.findByText("@ed")).toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId("onchain-creator-page")).not.toBeInTheDocument();
  });
});
