// src/features/marketplace/routes/BrowseRoute.test.tsx
import { render, screen, act, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { BrowseRoute } from "./BrowseRoute";

function Wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/marketplace"]}>
        <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
          {children}
        </MarketplaceDataProvider>
      </MemoryRouter>
    </QueryClientProvider>
  );
}

describe("BrowseRoute", () => {
  it("renders the H1 promise", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    expect(await screen.findByRole("heading", { level: 1 })).toBeInTheDocument();
  });

  it("renders strategy rows from listListings", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    // fixture ALL_LISTINGS includes "btc-momentum-v3"
    expect(await screen.findByText("btc-momentum-v3")).toBeInTheDocument();
  });

  it("renders the leaderboard rail with slices", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    expect(await screen.findByText("Trending")).toBeInTheDocument();
  });

  it("renders at least one GenArtPlaceholder thumb", async () => {
    const { container } = render(<BrowseRoute />, { wrapper: Wrapper });
    await waitFor(() => {
      expect(container.querySelectorAll('[data-genart="bitfields-v2"]').length).toBeGreaterThan(0);
    });
  });

  it("renders the Filters button in the toolbar", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    expect(await screen.findByRole("button", { name: /filters/i })).toBeInTheDocument();
  });

  it("opens the FilterDrawer when Filters is clicked", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    const btn = await screen.findByRole("button", { name: /filters/i });
    act(() => btn.click());
    // F0 FilterDrawer <aside> plus LeaderboardRail <aside> = at least 2 complementary roles
    expect(screen.getAllByRole("complementary").length).toBeGreaterThanOrEqual(2);
  });

  it("the FilterDrawer shows sort section content", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    const btn = await screen.findByRole("button", { name: /filters/i });
    act(() => btn.click());
    expect(screen.getByText("Sort by")).toBeInTheDocument();
  });

  it("Mine segment shows only the viewer's created listings", async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={["/marketplace?segment=mine"]}>
          <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
            <BrowseRoute />
          </MarketplaceDataProvider>
        </MemoryRouter>
      </QueryClientProvider>
    );
    // viewer.createdListingIds = ["btc-momentum-v3", "btc-grid-v2", "eth-mr-v2"]
    expect(await screen.findByText("btc-momentum-v3")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.queryByText("sol-strategist-pro")).not.toBeInTheDocument();
    });
  });

  it("clicking a leaderboard slice sets the slice URL param", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    const sliceItem = await screen.findByTestId("slice-sol-7d");
    act(() => sliceItem.click());
    // After click, row set should narrow to SOL assets (fixture slice filter)
    await waitFor(() => {
      expect(screen.queryByText("btc-momentum-v3")).not.toBeInTheDocument();
    });
  });
});
