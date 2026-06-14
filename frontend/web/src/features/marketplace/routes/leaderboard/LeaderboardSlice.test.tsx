// src/features/marketplace/routes/leaderboard/LeaderboardSlice.test.tsx
import { render as rtlRender, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { renderMarketplace } from "@/features/marketplace/test-utils";
import { LeaderboardSlice } from "./LeaderboardSlice";

// Fixture slice: sol-7d = { label: "Top on SOL · 7d", hint: "asset=SOL · 7d" }
// FixtureMarketplaceData.getLeaderboard filters the curated DEMO_LISTINGS by
// slice.filter ({ assets: ["SOL"] }) and reports a LIVE count of the matched
// rows. The curated pool has two SOL listings (sol-strategist-pro, meme-radar).

function render(sliceId = "sol-7d") {
  return renderMarketplace(<LeaderboardSlice />, {
    path: "/marketplace/leaderboard/:sliceId",
    route: `/marketplace/leaderboard/${sliceId}`,
  });
}

describe("LeaderboardSlice", () => {
  it("renders the slice label as the page heading", async () => {
    render();
    const heading = await screen.findByTestId("slice-label");
    expect(heading).toHaveTextContent("Top on SOL · 7d");
  });

  it("renders the slice hint text", async () => {
    render();
    await screen.findByTestId("slice-label");
    expect(screen.getByText(/asset=SOL · 7d/)).toBeInTheDocument();
  });

  it("renders the live slice count (matched rows in the curated pool)", async () => {
    render();
    await screen.findByTestId("slice-label");
    // Two SOL listings in the curated collection.
    expect(screen.getByText(/2 strategies/)).toBeInTheDocument();
  });

  it("renders a back link to /marketplace/leaderboard", async () => {
    render();
    await screen.findByTestId("slice-label");
    const backLink = screen.getByRole("link", { name: /← Leaderboard/i });
    expect(backLink).toHaveAttribute("href", "/marketplace/leaderboard");
  });

  it("renders ListingCard rows for the slice", async () => {
    render();
    // sol-7d slice filters by assets: ["SOL"]; fixture has SOL listings
    // Wait for the slice header to appear first
    await screen.findByTestId("slice-label");
    // At least one ListingCard should render (SOL-filtered rows)
    const cards = screen.getAllByRole("button", { name: /buy|run free/i });
    expect(cards.length).toBeGreaterThan(0);
  });

  it("renders the column header row", async () => {
    render();
    await screen.findByTestId("slice-label");
    expect(screen.getByText("Strategy")).toBeInTheDocument();
    expect(screen.getByText("30d return")).toBeInTheDocument();
    expect(screen.getByText("Buyers")).toBeInTheDocument();
    expect(screen.getByText("Sharpe")).toBeInTheDocument();
    expect(screen.getByText("Price")).toBeInTheDocument();
  });

  it("renders non-empty rows for the trending slice", async () => {
    renderMarketplace(<LeaderboardSlice />, {
      path: "/marketplace/leaderboard/:sliceId",
      route: "/marketplace/leaderboard/trending",
    });
    // trending slice: segment="trending", sort="return30d" — all listings pass
    await screen.findByTestId("slice-label");
    expect(screen.getByTestId("slice-label")).toHaveTextContent("Trending");
    const cards = screen.getAllByRole("button", { name: /buy|run free/i });
    expect(cards.length).toBeGreaterThan(0);
  });

  it("renders the Testnet badge from ListingCard on paid listings", async () => {
    render();
    await screen.findByTestId("slice-label");
    // ListingCard renders a [Testnet] badge on Buy CTAs
    const testnetBadges = screen.getAllByText(/testnet/i);
    expect(testnetBadges.length).toBeGreaterThan(0);
  });

  it("does not render any dialog or modal (no-popups rule)", async () => {
    render();
    await screen.findByTestId("slice-label");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

describe("LeaderboardSlice — buy routes to detail for confirmation (QA #11)", () => {
  function renderWithLineage(sliceId = "sol-7d") {
    const client = new FixtureMarketplaceData();
    const purchaseSpy = vi.spyOn(client, "purchaseIntent");
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    rtlRender(
      <QueryClientProvider client={qc}>
        <MarketplaceDataProvider client={client}>
          <MemoryRouter
            initialEntries={[`/marketplace/leaderboard/${sliceId}`]}
          >
            <Routes>
              <Route
                path="/marketplace/leaderboard/:sliceId"
                element={<LeaderboardSlice />}
              />
              <Route
                path="/marketplace/lineage/:id"
                element={<div data-testid="lineage-detail">detail</div>}
              />
            </Routes>
          </MemoryRouter>
        </MarketplaceDataProvider>
      </QueryClientProvider>,
    );
    return { purchaseSpy };
  }

  it("navigates to the strategy detail page instead of instant-purchasing", async () => {
    const { purchaseSpy } = renderWithLineage();
    await screen.findByTestId("slice-label");

    const buyButton = screen.getAllByRole("button", { name: /^buy$/i })[0];
    fireEvent.click(buyButton);

    // The buy CTA must route the user to the detail page (where requirements
    // are shown and they confirm) — NOT fire an on-chain purchase immediately.
    await screen.findByTestId("lineage-detail");
    expect(purchaseSpy).not.toHaveBeenCalled();
  });
});
