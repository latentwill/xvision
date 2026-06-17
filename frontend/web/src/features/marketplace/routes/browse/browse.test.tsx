// src/features/marketplace/routes/browse/browse.test.tsx
// Tests for the marketplace browse sub-components (spec 3.1, 7).
import { render, screen, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi, beforeAll, afterAll } from "vitest";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { HeaderStrip } from "./HeaderStrip";

// MiniSparkline (used by the entry's demo performance caption) renders a
// uPlot pane behind a ResizeObserver. jsdom provides neither a canvas context
// nor ResizeObserver — mock uPlot as a no-op and stub ResizeObserver (matches
// the pattern in EquityPanel.test.tsx / LineageRoute tests).
vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    destroy() {}
  },
}));
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

describe("HeaderStrip", () => {
  it("renders the plain 'Marketplace' page title", async () => {
    render(<HeaderStrip />, { wrapper: Wrapper });
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent("Marketplace");
  });

  it("renders a one-line product description and no editorial eyebrow", async () => {
    render(<HeaderStrip />, { wrapper: Wrapper });
    expect(
      await screen.findByText(/Buy and sell trading strategies as on-chain agents on Mantle\./i)
    ).toBeInTheDocument();
    // Regression guard: the old editorial brand eyebrow (an all-caps
    // "XVISION · …" lead-in) must not return. We match the distinctive
    // lead-in fragment rather than the full editorial phrase.
    expect(screen.queryByText(/XVISION ·/i)).not.toBeInTheDocument();
  });

  it("renders the honest stats line (entries / creators) and not the fixture cells", async () => {
    const rows = [
      { creator: { address: "0xAAA" } },
      { creator: { address: "0xbbb" } },
      { creator: { address: "0xAAA" } },
    ] as never;
    render(<HeaderStrip rows={rows} />, { wrapper: Wrapper });
    // The two honest stat cells render.
    expect(await screen.findByText(/entries/i)).toBeInTheDocument();
    expect(screen.getByText(/creators/i)).toBeInTheDocument();
    // Removed fixture-or-zero cells: paid this week / agent purchases / minted 24h.
    expect(screen.queryByText(/paid this week/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/agent purchases/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/minted in 24h/i)).not.toBeInTheDocument();
    // No fabricated "paid to creators" line at all anymore.
    expect(screen.queryByText(/paid to creators/i)).not.toBeInTheDocument();
    // Distinct creators from the two unique addresses above.
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("shows the dev-fixtures marker on the fixture client in dev", async () => {
    render(<HeaderStrip />, { wrapper: Wrapper });
    expect(await screen.findByTestId("dev-fixtures-marker")).toBeInTheDocument();
  });

  it("renders the List-your-strategy CTA and no Share button", async () => {
    render(<HeaderStrip />, { wrapper: Wrapper });
    expect(
      await screen.findByRole("button", { name: /list your strategy/i })
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^share$/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /share your strategy/i })).not.toBeInTheDocument();
  });

  it("keeps the Wallet link", async () => {
    render(<HeaderStrip />, { wrapper: Wrapper });
    expect(await screen.findByRole("link", { name: /wallet/i })).toBeInTheDocument();
  });

  it("renders a Testnet badge in the hero (once, as a standalone chip)", async () => {
    const { container } = render(<HeaderStrip />, { wrapper: Wrapper });
    await screen.findByRole("heading", { level: 1 });
    // The standalone TestnetBadge chip renders exactly the word "Testnet".
    const badges = Array.from(container.querySelectorAll("span")).filter(
      (el) => el.textContent === "Testnet"
    );
    expect(badges).toHaveLength(1);
  });
});

// --- Toolbar ---

import { Toolbar } from "./Toolbar";
import { AppliedChips } from "./AppliedChips";
import type { FilterState } from "@/features/marketplace/data/types";
import { defaultFilterState } from "@/features/marketplace/data/filter";

function toolbarProps(overrides: Partial<React.ComponentProps<typeof Toolbar>> = {}) {
  return {
    filter: defaultFilterState(),
    setFilter: vi.fn(),
    filterCount: 0,
    filtersOpen: false,
    onToggleFilters: vi.fn(),
    matchCount: 100,
    view: "list" as const,
    setView: vi.fn(),
    allowPerformanceSort: true,
    ...overrides,
  };
}

describe("Toolbar", () => {
  it("renders Trending | New | Mine segments", () => {
    render(<Toolbar {...toolbarProps()} />, { wrapper: Wrapper });
    expect(screen.getByRole("button", { name: /trending/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /new/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /mine/i })).toBeInTheDocument();
  });

  it("calls setFilter with segment when a segment is clicked", () => {
    const setFilter = vi.fn();
    render(<Toolbar {...toolbarProps({ setFilter })} />, { wrapper: Wrapper });
    act(() => screen.getByRole("button", { name: /new/i }).click());
    expect(setFilter).toHaveBeenCalledWith(expect.objectContaining({ segment: "new" }));
  });

  it("opens the SignalSelectMenu sort dropdown and selects a sort", async () => {
    const setFilter = vi.fn();
    const { container } = render(<Toolbar {...toolbarProps({ setFilter })} />, { wrapper: Wrapper });
    const user = userEvent.setup();
    // The sort trigger is the listbox button (SignalSelectMenu).
    const trigger = container.querySelector('button[aria-haspopup="listbox"]')!;
    await user.click(trigger);
    const option = await screen.findByRole("option", { name: /sharpe/i });
    await user.click(option);
    expect(setFilter).toHaveBeenCalledWith(expect.objectContaining({ sort: "sharpe" }));
  });

  it("omits return30d and sharpe sort options when performance sort is disallowed", async () => {
    const { container } = render(
      <Toolbar {...toolbarProps({ allowPerformanceSort: false })} />,
      { wrapper: Wrapper }
    );
    const user = userEvent.setup();
    const trigger = container.querySelector('button[aria-haspopup="listbox"]')!;
    await user.click(trigger);
    expect(await screen.findByRole("option", { name: /newest/i })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /30d return/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /^sharpe$/i })).not.toBeInTheDocument();
  });

  it("shows the filter count badge when filterCount > 0", () => {
    render(<Toolbar {...toolbarProps({ filterCount: 3 })} />, { wrapper: Wrapper });
    expect(screen.getByText("3")).toBeInTheDocument();
  });

  it("renders the / shortcut hint in the search field", () => {
    render(<Toolbar {...toolbarProps()} />, { wrapper: Wrapper });
    expect(screen.getByText("/")).toBeInTheDocument();
  });

  it("calls onToggleFilters when the Filters button is clicked", () => {
    const onToggleFilters = vi.fn();
    render(<Toolbar {...toolbarProps({ onToggleFilters })} />, { wrapper: Wrapper });
    act(() => screen.getByRole("button", { name: /filters/i }).click());
    expect(onToggleFilters).toHaveBeenCalledOnce();
  });

  it("renders a List | Index view toggle and switches view", () => {
    const setView = vi.fn();
    render(<Toolbar {...toolbarProps({ setView })} />, { wrapper: Wrapper });
    act(() => screen.getByRole("button", { name: /index view/i }).click());
    expect(setView).toHaveBeenCalledWith("index");
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

// --- SliceChips (replaces LeaderboardRail) ---

import { SliceChips } from "./SliceChips";
import type { Slice } from "@/features/marketplace/data/types";

const SLICES_FIXTURE: Slice[] = [
  { id: "trending", label: "Trending", hint: "velocity × return", count: 5, filter: { sort: "return30d" } },
  { id: "sol-7d", label: "Top on SOL · 7d", hint: "asset=SOL", count: 3, filter: { assets: ["SOL"] } },
  { id: "empty", label: "Empty slice", hint: "nothing", count: 0, filter: {} },
];

describe("SliceChips", () => {
  it("renders a chip per slice with a real count > 0 and omits count-0 slices", () => {
    render(
      <SliceChips slices={SLICES_FIXTURE} activeSliceId={undefined} onSliceClick={() => {}} />,
      { wrapper: Wrapper }
    );
    expect(screen.getByTestId("slice-chip-trending")).toBeInTheDocument();
    expect(screen.getByTestId("slice-chip-sol-7d")).toBeInTheDocument();
    expect(screen.queryByTestId("slice-chip-empty")).not.toBeInTheDocument();
  });

  it("renders nothing when every slice has count 0", () => {
    const { container } = render(
      <SliceChips
        slices={[{ id: "z", label: "Z", hint: "", count: 0, filter: {} }]}
        activeSliceId={undefined}
        onSliceClick={() => {}}
      />,
      { wrapper: Wrapper }
    );
    expect(container.querySelector("[data-slice-chips]")).toBeNull();
  });

  it("calls onSliceClick with the slice id when a chip is clicked", () => {
    const onSliceClick = vi.fn();
    render(
      <SliceChips slices={SLICES_FIXTURE} activeSliceId={undefined} onSliceClick={onSliceClick} />,
      { wrapper: Wrapper }
    );
    act(() => screen.getByTestId("slice-chip-trending").click());
    expect(onSliceClick).toHaveBeenCalledWith("trending");
  });

  it("marks the active chip", () => {
    render(
      <SliceChips slices={SLICES_FIXTURE} activeSliceId="trending" onSliceClick={() => {}} />,
      { wrapper: Wrapper }
    );
    expect(screen.getByTestId("slice-chip-trending")).toHaveAttribute("aria-pressed", "true");
  });

  it("does not render any CHAIN OPS callout (deleted from browse)", () => {
    render(
      <SliceChips slices={SLICES_FIXTURE} activeSliceId={undefined} onSliceClick={() => {}} />,
      { wrapper: Wrapper }
    );
    expect(screen.queryByText(/CHAIN OPS/i)).not.toBeInTheDocument();
  });
});

// --- ListingEntry (replaces ListingCard) ---

import { ListingEntry, humanize } from "./ListingEntry";
import type { ListingRow } from "@/features/marketplace/data/types";

const PAID_ROW: ListingRow = {
  id: "btc-momentum-v3",
  lineageId: "btc-momentum",
  version: "v3.0",
  name: "BTC Momentum",
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
  transferableLicense: false,
  genArtSeed: "btc-momentum-7a91-v3",
};

const OPEN_ROW: ListingRow = {
  ...PAID_ROW,
  id: "meme-radar",
  name: undefined,
  priceUsdc: null,
  tier: "open",
  return30dPct: 124.8,
  verification: "unverified",
  acceptsX402: false,
  assets: [],
};

function renderEntry(row: ListingRow, props: Partial<React.ComponentProps<typeof ListingEntry>> = {}) {
  return render(<ListingEntry row={row} {...props} />, { wrapper: Wrapper });
}

describe("humanize helper", () => {
  it("humanize slug → Title Case with acronyms upper-cased and version segments lowercase", () => {
    expect(humanize("btc-momentum-v3")).toBe("BTC Momentum v3");
  });
  it("humanize upper-cases other known asset acronyms", () => {
    expect(humanize("eth-mean-reversion-v2")).toBe("ETH Mean Reversion v2");
    expect(humanize("sol-strategist-pro")).toBe("SOL Strategist Pro");
  });
  it("humanize numeric → Strategy #id", () => {
    expect(humanize("42")).toBe("Strategy #42");
  });
});

describe("ListingEntry", () => {
  it("wraps the whole entry in a Link to the inspector (no list-row tx)", () => {
    renderEntry(PAID_ROW);
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("href", "/marketplace/lineage/btc-momentum-v3");
    // No buy/run button that fires a tx — the entry is the link.
    expect(screen.queryByRole("button", { name: /^buy$/i })).not.toBeInTheDocument();
  });

  it("renders the display name with a title attr", () => {
    renderEntry(PAID_ROW);
    const title = screen.getByText("BTC Momentum");
    expect(title).toHaveAttribute("title", "BTC Momentum");
  });

  it("falls back to humanize(id) when no name is present", () => {
    renderEntry(OPEN_ROW);
    expect(screen.getByText("Meme Radar")).toBeInTheDocument();
  });

  it("does not render an editorial plate number glyph", () => {
    const { container } = renderEntry(PAID_ROW);
    // The plate-number glyph (numero sign, U+2116) must not appear anywhere.
    const numeroSign = String.fromCharCode(0x2116);
    expect(container.textContent ?? "").not.toContain(numeroSign);
  });

  it("renders a GenArtPlaceholder thumbnail", () => {
    const { container } = renderEntry(PAID_ROW);
    expect(container.querySelector('[data-genart="bitfields-v3"]')).not.toBeNull();
  });

  it("renders AssetPills inline only when assets are present", () => {
    renderEntry(PAID_ROW);
    expect(screen.getByText("BTC")).toBeInTheDocument();
  });

  it("omits the assets segment entirely when assets are empty (no blank cell)", () => {
    renderEntry(OPEN_ROW);
    expect(screen.queryByText("BTC")).not.toBeInTheDocument();
  });

  it("renders the dignified pending performance caption (no sparkline) by default", () => {
    const { container } = renderEntry(PAID_ROW);
    expect(screen.getByText(/pending first live cycle/i)).toBeInTheDocument();
    expect(container.querySelector("[data-perf-spark]")).toBeNull();
  });

  it("renders the real return + sparkline only for the demo client (showSparkline)", () => {
    const { container } = renderEntry(PAID_ROW, { showSparkline: true });
    expect(container.querySelector("[data-perf-spark]")).not.toBeNull();
    const ret = container.querySelector("[data-return-pct]");
    expect(ret).not.toBeNull();
    expect(ret!.className).toContain("gold");
  });

  it("renders an OPEN edition seal and Run free label for open-tier listings", () => {
    renderEntry(OPEN_ROW);
    // "Open edition" appears as the acquisition seal (and the tier label in the
    // provenance caption) — assert at least one renders.
    expect(screen.getAllByText(/open edition/i).length).toBeGreaterThan(0);
    expect(screen.getByText(/run free/i)).toBeInTheDocument();
  });

  it("renders the USDC price (no fee) and Acquire label for paid listings", () => {
    renderEntry(PAID_ROW);
    expect(screen.getByText(/49/)).toBeInTheDocument();
    expect(screen.getByText("USDC")).toBeInTheDocument();
    expect(screen.getByText(/acquire/i)).toBeInTheDocument();
    // No fee embedded in the price (QA15).
    expect(screen.queryByText(/fee/i)).not.toBeInTheDocument();
  });

  it("renders a VerifiedBadge for verified listings only", () => {
    // VerifiedBadge is a shared component (owned elsewhere); assert via its
    // testid so this stays robust to the badge's own copy/styling.
    const { rerender } = render(
      <ListingEntry row={{ ...PAID_ROW, verification: "verified" }} />,
      { wrapper: Wrapper },
    );
    expect(screen.getByText("Verified")).toBeInTheDocument();
    rerender(<ListingEntry row={OPEN_ROW} />);
    expect(screen.queryByText("Verified")).not.toBeInTheDocument();
  });

  it("does not render a per-row Testnet badge", () => {
    renderEntry(PAID_ROW);
    expect(screen.queryByText(/testnet/i)).not.toBeInTheDocument();
  });
});

// --- FilterDrawerContent ---

import { FilterDrawerContent } from "./FilterDrawerContent";

function fdcProps(overrides: Partial<React.ComponentProps<typeof FilterDrawerContent>> = {}) {
  return {
    filter: defaultFilterState(),
    setFilter: vi.fn(),
    matchCount: 100,
    totalCount: 9,
    onClose: vi.fn(),
    ...overrides,
  };
}

describe("FilterDrawerContent", () => {
  it("renders all section headings", () => {
    render(<FilterDrawerContent {...fdcProps()} />, { wrapper: Wrapper });
    expect(screen.getByText("Sort by")).toBeInTheDocument();
    expect(screen.getByText("Assets")).toBeInTheDocument();
    expect(screen.getByText("Models")).toBeInTheDocument();
    expect(screen.getByText("Style")).toBeInTheDocument();
    expect(screen.getByText("Trust")).toBeInTheDocument();
    expect(screen.getByText("Price (USDC)")).toBeInTheDocument();
    expect(screen.getByText("Minimum buyers")).toBeInTheDocument();
  });

  it("uses the totalCount prop in the 'of N match' line (no 1,247 literal)", () => {
    render(
      <FilterDrawerContent {...fdcProps({ filter: { ...defaultFilterState(), assets: ["BTC"] }, matchCount: 4, totalCount: 9 })} />,
      { wrapper: Wrapper }
    );
    expect(screen.getByText(/of 9 match/)).toBeInTheDocument();
    expect(screen.queryByText(/1,247/)).not.toBeInTheDocument();
  });

  it("marks the active sort option", () => {
    render(<FilterDrawerContent {...fdcProps({ filter: { ...defaultFilterState(), sort: "sharpe" } })} />, {
      wrapper: Wrapper,
    });
    expect(screen.getByRole("radio", { name: /sharpe/i })).toBeChecked();
  });

  it("calls setFilter with new sort when sort radio changes", () => {
    const setFilter = vi.fn();
    render(<FilterDrawerContent {...fdcProps({ setFilter })} />, { wrapper: Wrapper });
    act(() => screen.getByRole("radio", { name: /buyers/i }).click());
    expect(setFilter).toHaveBeenCalledWith(expect.objectContaining({ sort: "buyers" }));
  });

  it("calls setFilter with added asset when an asset checkbox is clicked", () => {
    const setFilter = vi.fn();
    render(<FilterDrawerContent {...fdcProps({ setFilter })} />, { wrapper: Wrapper });
    act(() => screen.getByRole("checkbox", { name: /BTC/i }).click());
    expect(setFilter.mock.calls[0][0].assets).toContain("BTC");
  });

  it("calls setFilter with toggled verifiedOnly when the Verified toggle is clicked", () => {
    const setFilter = vi.fn();
    render(<FilterDrawerContent {...fdcProps({ setFilter })} />, { wrapper: Wrapper });
    act(() => screen.getByRole("switch", { name: /verified only/i }).click());
    expect(setFilter).toHaveBeenCalledWith(
      expect.objectContaining({ trust: expect.objectContaining({ verifiedOnly: true }) })
    );
  });

  it("renders the match count and a Done button in the footer", () => {
    render(<FilterDrawerContent {...fdcProps({ matchCount: 342 })} />, { wrapper: Wrapper });
    expect(screen.getByText("342")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /done/i })).toBeInTheDocument();
  });

  it("calls onClose when Done is clicked", () => {
    const onClose = vi.fn();
    render(<FilterDrawerContent {...fdcProps({ onClose, matchCount: 10 })} />, { wrapper: Wrapper });
    act(() => screen.getByRole("button", { name: /done/i }).click());
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("renders Tier section with Open and Sealed buttons", () => {
    render(<FilterDrawerContent {...fdcProps()} />, { wrapper: Wrapper });
    expect(screen.getByText("Tier")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open \(free\)/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /sealed \(paid\)/i })).toBeInTheDocument();
  });

  it("calls setFilter with tier when a tier button is clicked", () => {
    const setFilter = vi.fn();
    render(<FilterDrawerContent {...fdcProps({ setFilter })} />, { wrapper: Wrapper });
    act(() => screen.getByRole("button", { name: /open \(free\)/i }).click());
    expect(setFilter.mock.calls[0][0].tier).toContain("open");
  });

  it("removes a tier from filter.tier when the active tier button is clicked again", () => {
    const setFilter = vi.fn();
    render(
      <FilterDrawerContent {...fdcProps({ filter: { ...defaultFilterState(), tier: ["open"] }, setFilter, matchCount: 50 })} />,
      { wrapper: Wrapper }
    );
    act(() => screen.getByRole("button", { name: /open \(free\)/i }).click());
    expect(setFilter.mock.calls[0][0].tier).not.toContain("open");
  });
});
