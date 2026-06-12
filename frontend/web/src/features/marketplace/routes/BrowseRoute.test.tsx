// src/features/marketplace/routes/BrowseRoute.test.tsx
import { render, screen, act, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { BrowseRoute } from "./BrowseRoute";

// The demo catalogue now renders a real MiniSparkline (uPlot pane) for the
// curated named listings. Mock uPlot so tests don't need a canvas-backed DOM
// (same pattern as LineageRoute.test.tsx).
vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    setData() {}
    destroy() {}
  },
}));

// usePlot wires a ResizeObserver; jsdom doesn't provide one.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
beforeAll(() => {
  Object.defineProperty(globalThis, "ResizeObserver", {
    writable: true,
    configurable: true,
    value: ResizeObserverStub,
  });
});
afterAll(() => {
  delete (globalThis as { ResizeObserver?: unknown }).ResizeObserver;
});

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
  it("renders the Catalogue hero headline", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent("The Catalogue");
  });

  it("renders catalogue entries from listListings as links to the inspector", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    // fixture NAMED_LISTINGS includes "btc-momentum-v3" (humanized title)
    const entry = await screen.findByText("Btc Momentum V3");
    const link = entry.closest("a");
    expect(link).toHaveAttribute("href", "/marketplace/lineage/btc-momentum-v3");
  });

  it("renders the slice chip strip (replaces the leaderboard rail)", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    // Slice chips use slice labels; "Trending" is the first slice.
    expect(await screen.findByTestId("slice-chip-trending")).toBeInTheDocument();
  });

  it("does not render a leaderboard rail or CHAIN OPS callout", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    await screen.findByTestId("slice-chip-trending");
    expect(screen.queryByText(/CHAIN OPS/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/LEADERBOARDS/i)).not.toBeInTheDocument();
  });

  it("renders at least one GenArtPlaceholder plate", async () => {
    const { container } = render(<BrowseRoute />, { wrapper: Wrapper });
    await waitFor(() => {
      expect(container.querySelectorAll('[data-genart="bitfields-v3"]').length).toBeGreaterThan(0);
    });
  });

  it("renders the Filters button in the toolbar", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    expect(await screen.findByRole("button", { name: /filters/i })).toBeInTheDocument();
  });

  it("opens the inline filter accordion (in document flow, no overlay aside) when Filters is clicked", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    const btn = await screen.findByRole("button", { name: /filters/i });
    act(() => btn.click());
    // The filter accordion exposes its content; the sort section renders.
    expect(await screen.findByText("Sort by")).toBeInTheDocument();
    // No absolute overlay <aside> for filters; the inline accordion is a region.
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  it("closes the inline filter accordion on Escape", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    const user = userEvent.setup();
    const btn = await screen.findByRole("button", { name: /filters/i });
    await user.click(btn);
    expect(await screen.findByText("Sort by")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByText("Sort by")).not.toBeInTheDocument();
    });
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
    expect(await screen.findByText("Btc Momentum V3")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.queryByText("Sol Strategist Pro")).not.toBeInTheDocument();
    });
  });

  it("clicking a slice chip narrows the catalogue", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    const chip = await screen.findByTestId("slice-chip-sol-7d");
    act(() => chip.click());
    // SOL slice filter excludes BTC-only entries.
    await waitFor(() => {
      expect(screen.queryByText("Btc Momentum V3")).not.toBeInTheDocument();
    });
  });
});
