// src/features/marketplace/routes/browse/browse.test.tsx
// Shared test file for all browse sub-components. Add describes per task.
import { render, screen, act } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { HeaderStrip } from "./HeaderStrip";

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

describe("HeaderStrip", () => {
  it("renders the H1 promise copy", async () => {
    render(<HeaderStrip />, { wrapper: Wrapper });
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "Buy a strategy. Run it. Or share yours and get paid."
    );
  });

  it("renders the stats from getStats after load", async () => {
    render(<HeaderStrip />, { wrapper: Wrapper });
    // fixture: { totalStrategies: 1247, paidThisWeekUsd: 34820, agentPurchases: 218, mintedLast24h: 64 }
    expect(await screen.findByText(/1,247/)).toBeInTheDocument();
    expect(await screen.findByText(/\$34,820/)).toBeInTheDocument();
    expect(await screen.findByText(/218/)).toBeInTheDocument();
    expect(await screen.findByText(/64/)).toBeInTheDocument();
  });

  it("renders the Share and Share-your-strategy CTAs", async () => {
    render(<HeaderStrip />, { wrapper: Wrapper });
    expect(await screen.findByRole("button", { name: /share your strategy/i })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /^share$/i })).toBeInTheDocument();
  });
});

// --- Task 2: Toolbar + AppliedChips ---

import { Toolbar } from "./Toolbar";
import { AppliedChips } from "./AppliedChips";
import type { FilterState } from "@/features/marketplace/data/types";
import { defaultFilterState } from "@/features/marketplace/data/filter";

describe("Toolbar", () => {
  it("renders Trending | New | Mine segments", () => {
    const setFilter = vi.fn();
    render(
      <Toolbar
        filter={defaultFilterState()}
        setFilter={setFilter}
        filterCount={0}
        onOpenDrawer={() => {}}
        matchCount={100}
      />,
      { wrapper: Wrapper }
    );
    expect(screen.getByRole("button", { name: /trending/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /new/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /mine/i })).toBeInTheDocument();
  });

  it("calls setFilter with segment when a segment is clicked", () => {
    const setFilter = vi.fn();
    render(
      <Toolbar
        filter={defaultFilterState()}
        setFilter={setFilter}
        filterCount={0}
        onOpenDrawer={() => {}}
        matchCount={100}
      />,
      { wrapper: Wrapper }
    );
    act(() => screen.getByRole("button", { name: /new/i }).click());
    expect(setFilter).toHaveBeenCalledWith(expect.objectContaining({ segment: "new" }));
  });

  it("shows the filter count badge when filterCount > 0", () => {
    render(
      <Toolbar
        filter={defaultFilterState()}
        setFilter={vi.fn()}
        filterCount={3}
        onOpenDrawer={() => {}}
        matchCount={45}
      />,
      { wrapper: Wrapper }
    );
    expect(screen.getByText("3")).toBeInTheDocument();
  });

  it("renders the / shortcut hint in the search field", () => {
    render(
      <Toolbar
        filter={defaultFilterState()}
        setFilter={vi.fn()}
        filterCount={0}
        onOpenDrawer={() => {}}
        matchCount={0}
      />,
      { wrapper: Wrapper }
    );
    expect(screen.getByText("/")).toBeInTheDocument();
  });

  it("calls onOpenDrawer when the Filters button is clicked", () => {
    const onOpenDrawer = vi.fn();
    render(
      <Toolbar
        filter={defaultFilterState()}
        setFilter={vi.fn()}
        filterCount={0}
        onOpenDrawer={onOpenDrawer}
        matchCount={0}
      />,
      { wrapper: Wrapper }
    );
    act(() => screen.getByRole("button", { name: /filters/i }).click());
    expect(onOpenDrawer).toHaveBeenCalledOnce();
  });
});

describe("AppliedChips", () => {
  it("renders nothing when no filters are active", () => {
    const { container } = render(
      <AppliedChips filter={defaultFilterState()} setFilter={vi.fn()} matchCount={100} />,
      { wrapper: Wrapper }
    );
    expect(container.querySelector("[data-applied-chips]")).toBeNull();
  });

  it("renders a chip per active asset filter and removes it on ×", () => {
    const setFilter = vi.fn();
    const filter: FilterState = { ...defaultFilterState(), assets: ["BTC", "SOL"] };
    render(<AppliedChips filter={filter} setFilter={setFilter} matchCount={42} />, {
      wrapper: Wrapper,
    });
    expect(screen.getByText(/Asset: BTC/)).toBeInTheDocument();
    expect(screen.getByText(/Asset: SOL/)).toBeInTheDocument();
    const removes = screen.getAllByRole("button", { name: /remove/i });
    act(() => removes[0].click());
    expect(setFilter).toHaveBeenCalled();
    const call = setFilter.mock.calls[0][0];
    expect(call.assets).not.toContain("BTC");
  });

  it("clears all filters when Clear all is clicked", () => {
    const setFilter = vi.fn();
    const filter: FilterState = {
      ...defaultFilterState(),
      assets: ["BTC"],
      trust: { verifiedOnly: true, acceptsAgents: false, auditedOnly: false },
    };
    render(<AppliedChips filter={filter} setFilter={setFilter} matchCount={10} />, {
      wrapper: Wrapper,
    });
    act(() => screen.getByRole("button", { name: /clear all/i }).click());
    const call = setFilter.mock.calls[0][0];
    expect(call.assets).toEqual([]);
    expect(call.trust.verifiedOnly).toBe(false);
  });
});

// --- Task 3: LeaderboardRail ---

import { LeaderboardRail } from "./LeaderboardRail";

describe("LeaderboardRail", () => {
  it("renders slices from getSlices", async () => {
    render(
      <LeaderboardRail activeSliceId={undefined} onSliceClick={() => {}} />,
      { wrapper: Wrapper }
    );
    // fixture: SLICES[0].label = "Trending"
    expect(await screen.findByText("Trending")).toBeInTheDocument();
    expect(await screen.findByText("Top on SOL · 7d")).toBeInTheDocument();
  });

  it("marks the active slice with the gold style", async () => {
    render(
      <LeaderboardRail activeSliceId="trending" onSliceClick={() => {}} />,
      { wrapper: Wrapper }
    );
    const item = await screen.findByTestId("slice-trending");
    expect(item.className).toMatch(/gold/);
  });

  it("calls onSliceClick with the slice id when clicked", async () => {
    const onSliceClick = vi.fn();
    render(
      <LeaderboardRail activeSliceId={undefined} onSliceClick={onSliceClick} />,
      { wrapper: Wrapper }
    );
    const item = await screen.findByTestId("slice-trending");
    act(() => item.click());
    expect(onSliceClick).toHaveBeenCalledWith("trending");
  });

  it("renders the CHAIN OPS callout below the slices", async () => {
    render(
      <LeaderboardRail activeSliceId={undefined} onSliceClick={() => {}} />,
      { wrapper: Wrapper }
    );
    expect((await screen.findAllByText(/CHAIN OPS/i)).length).toBeGreaterThan(0);
  });
});

// --- Task 4: ListingCard ---

import { ListingCard } from "./ListingCard";
import type { ListingRow } from "@/features/marketplace/data/types";

const FIXTURE_ROW: ListingRow = {
  id: "btc-momentum-v3",
  lineageId: "btc-momentum",
  version: "v3.0",
  creator: { address: "0xa83e", handle: "@ed" },
  model: "Claude · Haiku 4.5",
  style: "Day",
  assets: ["BTC"],
  return30dPct: 47.2,
  sharpe: 1.31,
  buyers: { humans: 247, agents: 14 },
  priceUsdc: 49,
  tier: "sealed",
  verification: "verified",
  acceptsX402: true,
  clones: 8,
  transferableLicense: false,
  genArtSeed: "btc-momentum-7a91-v3",
};

const FREE_ROW: ListingRow = {
  ...FIXTURE_ROW,
  id: "meme-radar",
  priceUsdc: null,
  tier: "open",
  return30dPct: 124.8,
  verification: "unverified",
  acceptsX402: false,
};

describe("ListingCard", () => {
  it("renders listing id, version and creator handle", () => {
    render(<ListingCard row={FIXTURE_ROW} onBuy={() => {}} />, { wrapper: Wrapper });
    expect(screen.getByText("btc-momentum-v3")).toBeInTheDocument();
    expect(screen.getByText("v3.0")).toBeInTheDocument();
    expect(screen.getByText("@ed")).toBeInTheDocument();
  });

  it("renders a GenArtPlaceholder thumb", () => {
    const { container } = render(<ListingCard row={FIXTURE_ROW} onBuy={() => {}} />, {
      wrapper: Wrapper,
    });
    expect(container.querySelector('[data-genart="placeholder"]')).not.toBeNull();
  });

  it("renders the Sparkline", () => {
    const { container } = render(<ListingCard row={FIXTURE_ROW} onBuy={() => {}} />, {
      wrapper: Wrapper,
    });
    expect(container.querySelector("svg path")).not.toBeNull();
  });

  it("renders AssetPills for each asset", () => {
    render(<ListingCard row={FIXTURE_ROW} onBuy={() => {}} />, { wrapper: Wrapper });
    expect(screen.getByText("BTC")).toBeInTheDocument();
  });

  it("renders VerifiedBadge for verified listings", () => {
    render(<ListingCard row={FIXTURE_ROW} onBuy={() => {}} />, { wrapper: Wrapper });
    expect(screen.getByTitle(/backtested/i)).toBeInTheDocument();
  });

  it("renders X402Badge for x402 listings", () => {
    render(<ListingCard row={FIXTURE_ROW} onBuy={() => {}} />, { wrapper: Wrapper });
    expect(screen.getByText("x402")).toBeInTheDocument();
  });

  it("renders Buy CTA with [Testnet] indicator for paid listing", () => {
    render(<ListingCard row={FIXTURE_ROW} onBuy={() => {}} />, { wrapper: Wrapper });
    expect(screen.getByRole("button", { name: /buy/i })).toBeInTheDocument();
    expect(screen.getByText(/testnet/i)).toBeInTheDocument();
  });

  it("renders Run free CTA and OPEN pill for free-tier listing", () => {
    render(<ListingCard row={FREE_ROW} onBuy={() => {}} />, { wrapper: Wrapper });
    expect(screen.getByRole("button", { name: /run free/i })).toBeInTheDocument();
    expect(screen.getByText("OPEN")).toBeInTheDocument();
  });

  it("calls onBuy with the listing id when Buy is clicked", () => {
    const onBuy = vi.fn();
    render(<ListingCard row={FIXTURE_ROW} onBuy={onBuy} />, { wrapper: Wrapper });
    act(() => screen.getByRole("button", { name: /buy/i }).click());
    expect(onBuy).toHaveBeenCalledWith("btc-momentum-v3");
  });

  it("renders positive return in gold tone", () => {
    const { container } = render(<ListingCard row={FIXTURE_ROW} onBuy={() => {}} />, {
      wrapper: Wrapper,
    });
    const retEl = container.querySelector("[data-return-pct]");
    expect(retEl).not.toBeNull();
    expect(retEl!.className).toContain("gold");
  });

  it("renders negative return in danger tone", () => {
    const negRow: ListingRow = { ...FIXTURE_ROW, return30dPct: -5 };
    const { container } = render(<ListingCard row={negRow} onBuy={() => {}} />, {
      wrapper: Wrapper,
    });
    const retEl = container.querySelector("[data-return-pct]");
    expect(retEl!.className).toContain("danger");
  });
});

// --- Task 5: FilterDrawerContent ---

import { FilterDrawerContent } from "./FilterDrawerContent";

describe("FilterDrawerContent", () => {
  it("renders all section headings", () => {
    render(
      <FilterDrawerContent
        filter={defaultFilterState()}
        setFilter={vi.fn()}
        matchCount={100}
        onClose={() => {}}
      />,
      { wrapper: Wrapper }
    );
    expect(screen.getByText("Sort by")).toBeInTheDocument();
    expect(screen.getByText("Assets")).toBeInTheDocument();
    expect(screen.getByText("Models")).toBeInTheDocument();
    expect(screen.getByText("Style")).toBeInTheDocument();
    expect(screen.getByText("Trust")).toBeInTheDocument();
    expect(screen.getByText("Price (USDC)")).toBeInTheDocument();
    expect(screen.getByText("Minimum buyers")).toBeInTheDocument();
  });

  it("marks the active sort option", () => {
    render(
      <FilterDrawerContent
        filter={{ ...defaultFilterState(), sort: "sharpe" }}
        setFilter={vi.fn()}
        matchCount={50}
        onClose={() => {}}
      />,
      { wrapper: Wrapper }
    );
    const sharpeRadio = screen.getByRole("radio", { name: /sharpe/i });
    expect(sharpeRadio).toBeChecked();
  });

  it("calls setFilter with new sort when sort radio changes", () => {
    const setFilter = vi.fn();
    render(
      <FilterDrawerContent
        filter={defaultFilterState()}
        setFilter={setFilter}
        matchCount={100}
        onClose={() => {}}
      />,
      { wrapper: Wrapper }
    );
    const buyersRadio = screen.getByRole("radio", { name: /buyers/i });
    act(() => buyersRadio.click());
    expect(setFilter).toHaveBeenCalledWith(expect.objectContaining({ sort: "buyers" }));
  });

  it("calls setFilter with added asset when an asset checkbox is clicked", () => {
    const setFilter = vi.fn();
    render(
      <FilterDrawerContent
        filter={defaultFilterState()}
        setFilter={setFilter}
        matchCount={100}
        onClose={() => {}}
      />,
      { wrapper: Wrapper }
    );
    const btcCheckbox = screen.getByRole("checkbox", { name: /BTC/i });
    act(() => btcCheckbox.click());
    const call = setFilter.mock.calls[0][0];
    expect(call.assets).toContain("BTC");
  });

  it("calls setFilter with toggled verifiedOnly when the Verified toggle is clicked", () => {
    const setFilter = vi.fn();
    render(
      <FilterDrawerContent
        filter={defaultFilterState()}
        setFilter={setFilter}
        matchCount={100}
        onClose={() => {}}
      />,
      { wrapper: Wrapper }
    );
    const toggle = screen.getByRole("switch", { name: /verified only/i });
    act(() => toggle.click());
    expect(setFilter).toHaveBeenCalledWith(
      expect.objectContaining({ trust: expect.objectContaining({ verifiedOnly: true }) })
    );
  });

  it("renders the match count and Apply button in the footer", () => {
    render(
      <FilterDrawerContent
        filter={defaultFilterState()}
        setFilter={vi.fn()}
        matchCount={342}
        onClose={() => {}}
      />,
      { wrapper: Wrapper }
    );
    expect(screen.getByText("342")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /apply/i })).toBeInTheDocument();
  });

  it("calls onClose when Apply is clicked", () => {
    const onClose = vi.fn();
    render(
      <FilterDrawerContent
        filter={defaultFilterState()}
        setFilter={vi.fn()}
        matchCount={10}
        onClose={onClose}
      />,
      { wrapper: Wrapper }
    );
    act(() => screen.getByRole("button", { name: /apply/i }).click());
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("renders Tier section with Open and Sealed buttons", () => {
    render(
      <FilterDrawerContent
        filter={defaultFilterState()}
        setFilter={vi.fn()}
        matchCount={100}
        onClose={() => {}}
      />,
      { wrapper: Wrapper }
    );
    expect(screen.getByText("Tier")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open \(free\)/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /sealed \(paid\)/i })).toBeInTheDocument();
  });

  it("calls setFilter with tier when a tier button is clicked", () => {
    const setFilter = vi.fn();
    render(
      <FilterDrawerContent
        filter={defaultFilterState()}
        setFilter={setFilter}
        matchCount={100}
        onClose={() => {}}
      />,
      { wrapper: Wrapper }
    );
    act(() => screen.getByRole("button", { name: /open \(free\)/i }).click());
    const call = setFilter.mock.calls[0][0];
    expect(call.tier).toContain("open");
  });

  it("removes a tier from filter.tier when the active tier button is clicked again", () => {
    const setFilter = vi.fn();
    render(
      <FilterDrawerContent
        filter={{ ...defaultFilterState(), tier: ["open"] }}
        setFilter={setFilter}
        matchCount={50}
        onClose={() => {}}
      />,
      { wrapper: Wrapper }
    );
    act(() => screen.getByRole("button", { name: /open \(free\)/i }).click());
    const call = setFilter.mock.calls[0][0];
    expect(call.tier).not.toContain("open");
  });
});
