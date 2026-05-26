# Marketplace F1 — Browse Route Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `MarketplaceBrowseStub` at `/marketplace` with `BrowseRoute` — a fully functional, fixture-backed browse surface including: the header strip (H1 promise + `MarketplaceStats` counters + Share CTAs); toolbar (Segmented, search, sort, Filters button); applied-filter `RemovableChip` row; 232px leaderboard rail (`getSlices`, active slice → `?slice=`); strategy list (`listListings` → `ListingRow` rows using F0 primitives); and the `FilterDrawer` content (sort radio, assets checklist, models checklist, style chips, trust toggles, price range, min buyers). `useFilterState` syncs everything to the URL.

**Architecture:** One new file `BrowseRoute.tsx` + focused sub-components colocated next to it (same directory). The only change to `routes.tsx` is a single-line lazy-import swap (line 59 in the current file). `MarketplaceDataProvider` already wraps the subtree via `MarketplaceLayout`. All data goes through `useMarketplaceData()`.

**Tech Stack:** React 18, TypeScript, React-Router v6, `useQuery` pattern via raw `useState`+`useEffect` (no TanStack Query in feature-scope; match F0 pattern — the data seam is async, mock with `useState`+`useEffect`), Vitest 2 + React Testing Library + jsdom, Tailwind token classes, pnpm.

**Source spec:** [`../specs/2026-05-26-marketplace-phase-f-frontend-design.md`](../specs/2026-05-26-marketplace-phase-f-frontend-design.md) §4 F1 · **Design visual reference:** `docs/design/design_handoff_marketplace_shift/bc2-marketplace.jsx` + `README.md §1` · **F0 foundation:** [`2026-05-26-marketplace-phase-f0-foundation.md`](./2026-05-26-marketplace-phase-f0-foundation.md)

**Conventions (verified in repo, matching F0):**
- Run tests from `frontend/web/`: `pnpm exec vitest run <path>`. Typecheck: `pnpm typecheck`.
- Path alias `@/` → `src/`. Colocate tests as `*.test.tsx`. Token classes only; dark-mode border rules enforced.
- **No popups.** `FilterDrawer` is the F0 docked panel. Its content is added here. No `Dialog`/`Modal`/`Sheet`/`Popover` introduced at any point.
- `[Testnet]` on the Buy CTA (chain-bound action — `purchaseIntent` returns `TxRef` with `network`). Render the `TxChip`-style badge inline next to the button label.
- Execute on branch `feat/marketplace-f0` (the frozen F0 is already committed here). Commit per task.

**Seam freeze note:** Do NOT add methods to `MarketplaceData`. The interface is frozen. If something seems missing, add an Open question note below and work around it with what exists.

---

## File map (files created or modified by this plan)

```
src/features/marketplace/routes/
  BrowseRoute.tsx                    # main route component (Task 1–5)
  BrowseRoute.test.tsx               # integration tests (Task 6)
  browse/
    HeaderStrip.tsx                  # H1 + stats + CTAs (Task 1)
    Toolbar.tsx                      # Segmented + search + sort + Filters btn (Task 2)
    AppliedChips.tsx                 # RemovableChip row (Task 2)
    LeaderboardRail.tsx              # 232px left rail (Task 3)
    ListingCard.tsx                  # single ListingRow row (Task 4)
    FilterDrawerContent.tsx          # content for the F0 FilterDrawer shell (Task 5)
    browse.test.tsx                  # unit tests for sub-components (Tasks 1–5)
src/routes.tsx                       # single-line swap (Task 7)
```

`src/routes.tsx` is modified **once** (Task 7) as a **single-line change**: replace the `MarketplaceBrowseStub` lazy import target with `BrowseRoute`. No other file in `routes.tsx` changes. This must be its own isolated commit so it cannot contend with any parallel build that also touches `routes.tsx`.

---

## Open questions (seam gaps — do NOT unblock by inventing methods)

1. **`auditedOnly` filter has no corresponding `ListingRow` field.** `filter.ts` already documents this: `// auditedOnly: no ListingRow field yet — applied in Phase 1.` The Trust section in the drawer renders the toggle but the filter is a no-op against fixtures (same behavior as F0's `applyFilter`). Flag with a `// TODO(Phase 1): auditedOnly` comment in `FilterDrawerContent.tsx`. No seam change.
2. **No `publishedAt` field on `ListingRow`.** `SortKey = "newest"` uses `id.localeCompare` in `filter.ts` as a proxy. The sort option renders correctly; the proxy behavior is documented in `filter.ts`. No action needed.
3. **"Save view" CTA** is in the design reference but has no backing method in `MarketplaceData`. Render the button as a disabled ghost with a `// TODO(Phase F4/slice save)` comment. Do not wire it up.
4. **`getLeaderboard(sliceId)` vs `listListings(filter)` duality.** Clicking a rail slice sets `filter.slice` in the URL (via `setFilter({ slice: s.id })`). `listListings` in `FixtureMarketplaceData` does not apply `filter.slice` — it is ignored (the fixture `applyFilter` does not use it). The rail item highlights via URL `?slice=` param; row count shown in the rail item comes from `Slice.count`. The `getLeaderboard` method is reserved for the F4 `/marketplace/leaderboard/:sliceId` route. This is correct behavior for F1.
5. **`subscribeP urchases` live ticker.** The design reference has no live ticker on the browse page (only on the receipt page). Not wired in F1.

---

## Task 1: `HeaderStrip` — H1 + stats counters + Share CTAs

**Files:**
- Create: `src/features/marketplace/routes/browse/HeaderStrip.tsx`
- Test: `src/features/marketplace/routes/browse/browse.test.tsx` (shared test file, add per task)

### Step 1: Write the failing test

```tsx
// src/features/marketplace/routes/browse/browse.test.tsx
// Shared test file for all browse sub-components. Add describes per task.
import { render, screen, act } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { HeaderStrip } from "./HeaderStrip";

function Wrapper({ children }: { children: React.ReactNode }) {
  return (
    <MemoryRouter initialEntries={["/marketplace"]}>
      <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
        {children}
      </MarketplaceDataProvider>
    </MemoryRouter>
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
```

### Step 2: Run to verify it fails

Run: `pnpm exec vitest run src/features/marketplace/routes/browse/browse.test.tsx`
Expected: FAIL — module not found.

### Step 3: Implement

```tsx
// src/features/marketplace/routes/browse/HeaderStrip.tsx
import { useEffect, useState } from "react";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import type { MarketplaceStats } from "@/features/marketplace/data/types";

function fmt(n: number): string {
  return n.toLocaleString("en-US");
}

function fmtUsd(n: number): string {
  return `$${n.toLocaleString("en-US")}`;
}

export function HeaderStrip() {
  const mp = useMarketplaceData();
  const [stats, setStats] = useState<MarketplaceStats | null>(null);

  useEffect(() => {
    mp.getStats().then(setStats);
  }, [mp]);

  return (
    <div className="px-7 py-5 border-b border-border flex justify-between items-end gap-6">
      <div className="min-w-0 max-w-[780px]">
        <h1 className="m-0 text-[24px] font-semibold tracking-[-0.025em] leading-[1.15]">
          Buy a strategy. Run it. Or share yours and get paid.
        </h1>
        <div className="mt-2.5 text-[11.5px] font-mono text-text-3 flex items-center flex-wrap gap-0 tracking-[0.01em]">
          {stats ? (
            <>
              <span>
                <span className="text-text-2">{fmt(stats.totalStrategies)}</span> strategies
              </span>
              <span className="mx-2.5 text-text-4">·</span>
              <span>
                <span className="text-gold">{fmtUsd(stats.paidThisWeekUsd)}</span> paid this week
              </span>
              <span className="mx-2.5 text-text-4">·</span>
              <span className="inline-flex items-center gap-1">
                <AgentIcon size={11} />
                <span>
                  <span className="text-text-2">{fmt(stats.agentPurchases)}</span> agent purchases
                </span>
              </span>
              <span className="mx-2.5 text-text-4">·</span>
              <span>
                <span className="text-text-2">{fmt(stats.mintedLast24h)}</span> minted in 24h
              </span>
            </>
          ) : (
            <span className="text-text-4">Loading…</span>
          )}
        </div>
      </div>
      <div className="flex gap-2 items-center shrink-0">
        <button
          type="button"
          aria-label="share"
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded border border-border-strong bg-transparent text-text-2 text-[12px] font-medium hover:text-text hover:border-border"
        >
          Share
        </button>
        <button
          type="button"
          aria-label="share your strategy"
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded border border-gold/60 bg-gold/10 text-gold text-[12px] font-medium hover:bg-gold/20"
        >
          + Share your strategy
        </button>
      </div>
    </div>
  );
}
```

### Step 4: Run to verify it passes

Run: `pnpm exec vitest run src/features/marketplace/routes/browse/browse.test.tsx`
Expected: PASS (HeaderStrip describe — 3 tests).

### Step 5: Commit

```bash
git add src/features/marketplace/routes/browse/HeaderStrip.tsx src/features/marketplace/routes/browse/browse.test.tsx
git commit -m "feat(marketplace/f1): HeaderStrip — H1 + stats counters + CTAs"
```

---

## Task 2: `Toolbar` + `AppliedChips`

**Files:**
- Create: `src/features/marketplace/routes/browse/Toolbar.tsx`
- Create: `src/features/marketplace/routes/browse/AppliedChips.tsx`
- Extend: `src/features/marketplace/routes/browse/browse.test.tsx` (add describes)

### Step 1: Write failing tests (append to browse.test.tsx)

Append the following describes to the existing `browse.test.tsx`:

```tsx
// --- append below the HeaderStrip describe ---

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
```

### Step 2: Run to verify it fails

Run: `pnpm exec vitest run src/features/marketplace/routes/browse/browse.test.tsx`
Expected: FAIL — modules `Toolbar` and `AppliedChips` not found.

### Step 3: Implement Toolbar

```tsx
// src/features/marketplace/routes/browse/Toolbar.tsx
import type { FilterState, SortKey } from "@/features/marketplace/data/types";
import { defaultFilterState } from "@/features/marketplace/data/filter";

const SORT_LABELS: Record<SortKey, string> = {
  return30d: "30d return",
  sharpe: "Sharpe",
  buyers: "Buyers",
  mostCloned: "Most cloned",
  newest: "Newest",
};

const SEGMENTS: { key: FilterState["segment"]; label: string }[] = [
  { key: "trending", label: "Trending" },
  { key: "new", label: "New" },
  { key: "mine", label: "Mine" },
];

interface ToolbarProps {
  filter: FilterState;
  setFilter: (patch: Partial<FilterState>) => void;
  filterCount: number;
  onOpenDrawer: () => void;
  matchCount: number;
}

export function Toolbar({ filter, setFilter, filterCount, onOpenDrawer }: ToolbarProps) {
  return (
    <div className="relative border-b border-border">
      <div className="px-7 py-3.5 flex items-center gap-3 flex-wrap">
        {/* Segmented: Trending | New | Mine */}
        <div className="inline-flex border border-border-strong rounded bg-surface-elev p-0.5">
          {SEGMENTS.map((s) => {
            const isActive = filter.segment === s.key;
            return (
              <button
                key={s.key}
                type="button"
                aria-label={s.label}
                onClick={() => setFilter({ segment: s.key })}
                className={[
                  "px-3 py-1 rounded-[3px] text-[12px] font-semibold cursor-pointer transition-colors",
                  isActive
                    ? "bg-gold text-[#001A0A]"
                    : "bg-transparent text-text-2 hover:text-text",
                ].join(" ")}
              >
                {s.label}
              </button>
            );
          })}
        </div>

        {/* Search */}
        <div className="flex-1 min-w-[240px] max-w-[380px] flex items-center gap-2 px-2.5 py-1.5 border border-border-strong rounded bg-surface-elev">
          <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-text-3 shrink-0" aria-hidden="true">
            <circle cx="6" cy="6" r="4" />
            <path d="M9.5 9.5l2.5 2.5" strokeLinecap="round" />
          </svg>
          <input
            type="search"
            placeholder="name · creator · tag"
            value={filter.search}
            onChange={(e) => setFilter({ search: e.target.value })}
            className="flex-1 bg-transparent font-mono text-[12px] text-text-3 placeholder:text-text-3 outline-none"
          />
          <kbd className="ml-auto border border-border-strong rounded-[3px] font-mono text-[9.5px] text-text-3 px-1.5 py-0.5 tracking-[0.06em]">/</kbd>
        </div>

        {/* Sort button */}
        <button
          type="button"
          aria-label={`sort by ${SORT_LABELS[filter.sort]}`}
          className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded border border-border-strong bg-surface-elev text-text-2 text-[12px] font-medium hover:border-border"
        >
          <span className="font-medium">Sort</span>
          <span className="pl-1.5 ml-0.5 border-l border-border font-mono text-[11px] text-text-3">
            {SORT_LABELS[filter.sort]}
          </span>
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
            <path d="M2 4l3 3 3-3" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>

        <span className="w-px h-[22px] bg-border" />

        {/* Filters button */}
        <button
          type="button"
          aria-label="filters"
          onClick={onOpenDrawer}
          className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded border border-border-strong bg-surface-elev text-text-2 text-[12px] font-medium hover:border-border"
        >
          <span className="font-medium">Filters</span>
          {filterCount > 0 && (
            <span className="pl-1.5 ml-0.5 border-l border-border flex items-center gap-1">
              <span className="min-w-[14px] text-center px-1 rounded-full bg-border-strong font-mono text-[9.5px] text-text font-bold leading-[1.3]">
                {filterCount}
              </span>
            </span>
          )}
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
            <path d="M3 2h5M2 5h6M4 8h2" strokeLinecap="round" />
          </svg>
        </button>

        {/* Save view (disabled until F4 slice-save) */}
        {/* TODO(Phase F4/slice save): wire Save view to createSlice */}
        <div className="ml-auto">
          <button
            type="button"
            disabled
            aria-label="save view"
            title="Save view — available in a future phase"
            className="opacity-40 inline-flex items-center gap-1.5 px-3 py-1.5 rounded border border-border-strong bg-transparent text-text-2 text-[12px] font-medium cursor-not-allowed"
          >
            Save view
          </button>
        </div>
      </div>
    </div>
  );
}
```

### Step 4: Implement AppliedChips

```tsx
// src/features/marketplace/routes/browse/AppliedChips.tsx
import { RemovableChip } from "@/features/marketplace/components/RemovableChip";
import { defaultFilterState } from "@/features/marketplace/data/filter";
import type { FilterState } from "@/features/marketplace/data/types";

interface AppliedChipsProps {
  filter: FilterState;
  setFilter: (patch: Partial<FilterState>) => void;
  matchCount: number;
}

function hasActiveFilters(f: FilterState): boolean {
  return (
    f.assets.length > 0 ||
    f.models.length > 0 ||
    f.styles.length > 0 ||
    f.trust.verifiedOnly ||
    f.trust.acceptsAgents ||
    f.trust.auditedOnly ||
    f.minBuyers > 0 ||
    f.priceUsdc.from !== 0 ||
    f.priceUsdc.to !== 500
  );
}

export function AppliedChips({ filter, setFilter, matchCount }: AppliedChipsProps) {
  if (!hasActiveFilters(filter)) return null;

  const chips: { label: string; onRemove: () => void }[] = [];

  for (const asset of filter.assets) {
    chips.push({
      label: `Asset: ${asset}`,
      onRemove: () => setFilter({ assets: filter.assets.filter((a) => a !== asset) }),
    });
  }
  for (const model of filter.models) {
    chips.push({
      label: `Model: ${model}`,
      onRemove: () => setFilter({ models: filter.models.filter((m) => m !== model) }),
    });
  }
  for (const style of filter.styles) {
    chips.push({
      label: `Style: ${style}`,
      onRemove: () => setFilter({ styles: filter.styles.filter((s) => s !== style) }),
    });
  }
  if (filter.trust.verifiedOnly) {
    chips.push({
      label: "Verified only",
      onRemove: () => setFilter({ trust: { ...filter.trust, verifiedOnly: false } }),
    });
  }
  if (filter.trust.acceptsAgents) {
    chips.push({
      label: "Accepts agents",
      onRemove: () => setFilter({ trust: { ...filter.trust, acceptsAgents: false } }),
    });
  }
  if (filter.trust.auditedOnly) {
    chips.push({
      label: "Audited only",
      onRemove: () => setFilter({ trust: { ...filter.trust, auditedOnly: false } }),
    });
  }
  if (filter.minBuyers > 0) {
    chips.push({
      label: `Min buyers: ${filter.minBuyers}`,
      onRemove: () => setFilter({ minBuyers: 0 }),
    });
  }
  if (filter.priceUsdc.from !== 0 || filter.priceUsdc.to !== 500) {
    chips.push({
      label: `Price: ${filter.priceUsdc.from}–${filter.priceUsdc.to} USDC`,
      onRemove: () => setFilter({ priceUsdc: { from: 0, to: 500 } }),
    });
  }

  function clearAll() {
    const def = defaultFilterState();
    setFilter({
      assets: def.assets,
      models: def.models,
      styles: def.styles,
      trust: def.trust,
      minBuyers: def.minBuyers,
      priceUsdc: def.priceUsdc,
    });
  }

  return (
    <div
      data-applied-chips
      className="px-7 pb-3 pt-1 flex items-center gap-1.5 flex-wrap"
    >
      <span className="font-mono text-[9px] tracking-[0.2em] uppercase text-text-3">
        APPLIED
      </span>
      {chips.map((c) => (
        <RemovableChip key={c.label} onRemove={c.onRemove}>
          {c.label}
        </RemovableChip>
      ))}
      <button
        type="button"
        aria-label="clear all"
        onClick={clearAll}
        className="text-text-3 text-[11.5px] ml-1 cursor-pointer underline decoration-dotted underline-offset-[3px] hover:text-text bg-transparent border-none p-0"
      >
        Clear all
      </button>
      <span className="ml-auto font-mono text-[11px] text-text-3">
        <span className="text-text-2">{matchCount.toLocaleString()}</span> matches
      </span>
    </div>
  );
}
```

### Step 5: Run to verify it passes

Run: `pnpm exec vitest run src/features/marketplace/routes/browse/browse.test.tsx`
Expected: PASS (HeaderStrip + Toolbar + AppliedChips describes — 11 tests total).

### Step 6: Commit

```bash
git add src/features/marketplace/routes/browse/Toolbar.tsx src/features/marketplace/routes/browse/AppliedChips.tsx src/features/marketplace/routes/browse/browse.test.tsx
git commit -m "feat(marketplace/f1): Toolbar (segmented + search + sort + filters) + AppliedChips"
```

---

## Task 3: `LeaderboardRail`

**Files:**
- Create: `src/features/marketplace/routes/browse/LeaderboardRail.tsx`
- Extend: `src/features/marketplace/routes/browse/browse.test.tsx` (append describe)

### Step 1: Write failing test (append to browse.test.tsx)

```tsx
// --- append below AppliedChips describe ---

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
    expect(await screen.findByText(/CHAIN OPS/i)).toBeInTheDocument();
  });
});
```

### Step 2: Run to verify it fails

Run: `pnpm exec vitest run src/features/marketplace/routes/browse/browse.test.tsx`
Expected: FAIL — `LeaderboardRail` not found.

### Step 3: Implement

```tsx
// src/features/marketplace/routes/browse/LeaderboardRail.tsx
import { useEffect, useState } from "react";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import type { Slice, SliceId } from "@/features/marketplace/data/types";

interface LeaderboardRailProps {
  activeSliceId: SliceId | undefined;
  onSliceClick: (id: SliceId) => void;
}

export function LeaderboardRail({ activeSliceId, onSliceClick }: LeaderboardRailProps) {
  const mp = useMarketplaceData();
  const [slices, setSlices] = useState<Slice[]>([]);

  useEffect(() => {
    mp.getSlices().then(setSlices);
  }, [mp]);

  return (
    <aside className="border-r border-border px-3.5 py-4 flex flex-col gap-3.5 overflow-hidden w-[232px] shrink-0">
      <div>
        <div className="flex items-center justify-between mb-2">
          <span className="font-mono text-[9.5px] tracking-[0.18em] uppercase text-text-3">
            LEADERBOARDS
          </span>
          <span className="font-mono text-[10px] text-text-4">shareable URLs</span>
        </div>
        <div className="flex flex-col">
          {slices.map((s) => {
            const isActive = s.id === activeSliceId;
            return (
              <div
                key={s.id}
                data-testid={`slice-${s.id}`}
                role="button"
                tabIndex={0}
                onClick={() => onSliceClick(s.id)}
                onKeyDown={(e) => e.key === "Enter" && onSliceClick(s.id)}
                className={[
                  "px-2.5 py-2 -mx-2 rounded cursor-pointer",
                  isActive
                    ? "bg-gold/10 border border-gold/30 text-gold"
                    : "bg-transparent border border-transparent text-text hover:bg-surface-elev",
                ].join(" ")}
              >
                <div className="flex items-center gap-2">
                  <span className={`text-[12.5px] font-${isActive ? "semibold" : "medium"}`}>
                    {s.label}
                  </span>
                  <span className="font-mono ml-auto text-[10px] text-text-3">
                    {s.count.toLocaleString()}
                  </span>
                </div>
                <div className="font-mono text-[9.5px] text-text-3 mt-0.5 tracking-[0.02em]">
                  {s.hint}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Chain ops callout */}
      <div className="mt-auto px-3 py-2.5 border border-dashed border-border-strong rounded-[5px]">
        <div className="font-mono text-[9.5px] tracking-[0.18em] uppercase text-text-3 mb-1.5">
          CHAIN OPS
        </div>
        <div className="font-mono text-[10.5px] text-text-3 leading-[1.5]">
          Anchor · mint missing · attesters → in{" "}
          <span className="text-text-2">Settings → Chain ops</span>
        </div>
      </div>
    </aside>
  );
}
```

### Step 4: Run to verify it passes

Run: `pnpm exec vitest run src/features/marketplace/routes/browse/browse.test.tsx`
Expected: PASS (all describes — 15 tests).

### Step 5: Commit

```bash
git add src/features/marketplace/routes/browse/LeaderboardRail.tsx src/features/marketplace/routes/browse/browse.test.tsx
git commit -m "feat(marketplace/f1): LeaderboardRail with slice selection + CHAIN OPS callout"
```

---

## Task 4: `ListingCard` (single ListingRow row)

**Files:**
- Create: `src/features/marketplace/routes/browse/ListingCard.tsx`
- Extend: `src/features/marketplace/routes/browse/browse.test.tsx` (append describe)

### Step 1: Write failing test (append to browse.test.tsx)

```tsx
// --- append below LeaderboardRail describe ---

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
```

### Step 2: Run to verify it fails

Run: `pnpm exec vitest run src/features/marketplace/routes/browse/browse.test.tsx`
Expected: FAIL — `ListingCard` not found.

### Step 3: Implement

```tsx
// src/features/marketplace/routes/browse/ListingCard.tsx
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { Sparkline } from "@/features/marketplace/components/Sparkline";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { X402Badge } from "@/features/marketplace/components/X402Badge";
import type { ListingRow } from "@/features/marketplace/data/types";

interface ListingCardProps {
  row: ListingRow;
  onBuy: (id: string) => void;
}

// The Buy CTA is a chain-bound action (purchaseIntent → TxRef with network).
// Per CLAUDE.md: [Testnet] on any chain-bound affordance.
// The network is mantle-sepolia for fixture data; the label is inline near the button.
function TestnetBadge() {
  return (
    <span className="px-1 rounded-[3px] border border-warn/40 text-warn text-[9px] uppercase font-mono tracking-[0.06em]">
      Testnet
    </span>
  );
}

export function ListingCard({ row, onBuy }: ListingCardProps) {
  const positive = row.return30dPct >= 0;
  const retSign = positive ? "+" : "";
  const isFree = row.priceUsdc === null || row.tier === "open";

  return (
    <div
      className="grid items-center gap-3.5 px-[22px] py-3 border-b border-[var(--border-soft,theme(colors.border))] cursor-pointer hover:bg-surface-elev/40 transition-colors"
      style={{
        gridTemplateColumns: "56px 1.8fr 0.75fr 1.1fr 1.05fr 0.6fr 0.85fr 110px",
      }}
    >
      {/* Gen-art thumb */}
      <div>
        <GenArtPlaceholder seed={row.genArtSeed} size={48} className="rounded-[4px]" />
      </div>

      {/* Name + version + badges + creator line */}
      <div className="min-w-0">
        <div className="flex items-center gap-1.5 flex-nowrap whitespace-nowrap overflow-hidden">
          <span className="font-mono text-[13px] text-text font-semibold truncate">
            {row.id}
          </span>
          <span className="font-mono text-[11px] text-text-3 shrink-0">{row.version}</span>
          {row.verification === "verified" && <VerifiedBadge />}
          {row.acceptsX402 && <X402Badge />}
        </div>
        <div className="flex items-center gap-2 mt-1 whitespace-nowrap overflow-hidden">
          <span className="font-mono text-[11px] text-text-2">{row.creator.handle ?? row.creator.address.slice(0, 8)}</span>
          <span className="text-text-4 text-[10px]">·</span>
          <span className="font-mono text-[10.5px] text-text-3 truncate">{row.model}</span>
          <span className="text-text-4 text-[10px]">·</span>
          <span className="font-mono text-[10.5px] text-text-3">{row.style}</span>
        </div>
      </div>

      {/* Asset pills */}
      <div className="flex gap-1 flex-wrap">
        {row.assets.map((a) => (
          <AssetPill key={a} asset={a} />
        ))}
      </div>

      {/* 30d return + sparkline */}
      <div className="flex items-center gap-2.5 justify-end">
        <span
          data-return-pct
          className={`font-mono text-[16px] font-semibold tracking-[-0.01em] ${positive ? "text-gold" : "text-danger"}`}
        >
          {retSign}{row.return30dPct}%
        </span>
        <Sparkline seed={row.id} positive={positive} />
      </div>

      {/* Buyers: humans + agents */}
      <div className="flex items-center gap-2">
        <span className="font-mono text-[13px] text-text">{row.buyers.humans.toLocaleString()}</span>
        <span className="text-text-4 text-[10px]">·</span>
        <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-[3px] bg-gold/10 border border-gold/30">
          <AgentIcon size={10} className="text-gold" />
          <span className="font-mono text-[11px] text-gold font-semibold">{row.buyers.agents}</span>
        </span>
      </div>

      {/* Sharpe */}
      <div className="text-right">
        <span className="font-mono text-[12px] text-text-3">
          {row.sharpe > 0 ? "+" : ""}{row.sharpe.toFixed(2)}
        </span>
      </div>

      {/* Price */}
      <div>
        {isFree ? (
          <span className="inline-flex items-center gap-1.5 px-2 py-1 border border-gold/30 bg-gold/10 rounded-[3px]">
            <span className="w-1.5 h-1.5 rounded-full bg-gold" />
            <span className="font-mono text-[10.5px] text-gold tracking-[0.14em] font-semibold">
              OPEN
            </span>
          </span>
        ) : (
          <span className="font-mono text-[13px] text-text">
            {row.priceUsdc} USDC
          </span>
        )}
      </div>

      {/* CTA */}
      <div className="flex flex-col items-start gap-1">
        <button
          type="button"
          aria-label={isFree ? "run free" : "buy"}
          onClick={(e) => {
            e.stopPropagation();
            onBuy(row.id);
          }}
          className="w-full px-3 py-1.5 rounded bg-gold text-[#001A0A] text-[12px] font-bold hover:opacity-90 transition-opacity"
        >
          {isFree ? "Run free" : "Buy"}
        </button>
        {/* [Testnet] badge — all chain-bound CTAs label the network */}
        <TestnetBadge />
      </div>
    </div>
  );
}
```

### Step 4: Run to verify it passes

Run: `pnpm exec vitest run src/features/marketplace/routes/browse/browse.test.tsx`
Expected: PASS (all describes — 26 tests).

### Step 5: Commit

```bash
git add src/features/marketplace/routes/browse/ListingCard.tsx src/features/marketplace/routes/browse/browse.test.tsx
git commit -m "feat(marketplace/f1): ListingCard — GenArt thumb + Sparkline + Buy/Run free + [Testnet]"
```

---

## Task 5: `FilterDrawerContent`

**Files:**
- Create: `src/features/marketplace/routes/browse/FilterDrawerContent.tsx`
- Extend: `src/features/marketplace/routes/browse/browse.test.tsx` (append describe)

### Step 1: Write failing test (append to browse.test.tsx)

```tsx
// --- append below ListingCard describe ---

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
});
```

### Step 2: Run to verify it fails

Run: `pnpm exec vitest run src/features/marketplace/routes/browse/browse.test.tsx`
Expected: FAIL — `FilterDrawerContent` not found.

### Step 3: Implement

```tsx
// src/features/marketplace/routes/browse/FilterDrawerContent.tsx
// Content rendered inside the F0 FilterDrawer shell.
// Sections: Sort · Assets · Models · Style · Trust · Price · Min buyers.
// TODO(Phase 1): auditedOnly filter has no ListingRow field yet — toggle renders but is a no-op.
import type { FilterState, SortKey } from "@/features/marketplace/data/types";
import { defaultFilterState } from "@/features/marketplace/data/filter";

// ── Constants ────────────────────────────────────────────────────────────────

const SORT_OPTIONS: { key: SortKey; label: string }[] = [
  { key: "return30d", label: "30d return" },
  { key: "sharpe", label: "Sharpe" },
  { key: "buyers", label: "Buyers (humans + agents)" },
  { key: "mostCloned", label: "Most cloned" },
  { key: "newest", label: "Newest" },
];

const ASSET_GROUPS: { group: string; items: { sym: string; name: string }[] }[] = [
  {
    group: "Crypto · majors",
    items: [
      { sym: "BTC", name: "Bitcoin" },
      { sym: "ETH", name: "Ethereum" },
      { sym: "SOL", name: "Solana" },
      { sym: "MATIC", name: "Polygon" },
      { sym: "AVAX", name: "Avalanche" },
    ],
  },
  {
    group: "Crypto · L2 & memes",
    items: [
      { sym: "ARB", name: "Arbitrum" },
      { sym: "OP", name: "Optimism" },
      { sym: "BASE", name: "Base" },
      { sym: "MNT", name: "Mantle" },
      { sym: "DOGE", name: "Dogecoin" },
      { sym: "WIF", name: "dogwifhat" },
      { sym: "PEPE", name: "Pepe" },
    ],
  },
  {
    group: "Equities",
    items: [
      { sym: "SPY", name: "S&P 500 ETF" },
      { sym: "QQQ", name: "Nasdaq-100" },
      { sym: "NVDA", name: "NVIDIA" },
      { sym: "TSLA", name: "Tesla" },
    ],
  },
  {
    group: "FX",
    items: [
      { sym: "EUR/USD", name: "Euro / USD" },
      { sym: "USD/JPY", name: "USD / Yen" },
    ],
  },
];

const MODEL_OPTIONS = [
  "Claude · Haiku 4.5",
  "Claude · Sonnet 4.5",
  "GPT-5",
  "Gemini 3 Pro",
  "Llama 4",
];

const STYLE_OPTIONS = ["Long", "Long/Short", "Day", "Swing", "Mean-reversion", "Momentum"];

// ── Section wrapper ───────────────────────────────────────────────────────────

function DrawerSection({
  title,
  sub,
  children,
}: {
  title: string;
  sub?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="px-[18px] py-3.5 border-b border-border">
      <div className="flex items-baseline justify-between mb-2.5">
        <span className="text-[12.5px] font-semibold text-text">{title}</span>
        {sub && <span className="font-mono text-[10px] text-text-3">{sub}</span>}
      </div>
      {children}
    </div>
  );
}

// ── CheckRow helper ───────────────────────────────────────────────────────────

function CheckRow({
  id,
  label,
  sub,
  checked,
  onToggle,
}: {
  id: string;
  label: string;
  sub?: string;
  checked: boolean;
  onToggle: () => void;
}) {
  return (
    <label
      htmlFor={id}
      className={[
        "grid items-center gap-2.5 px-1.5 py-1 rounded-[3px] cursor-pointer",
        checked ? "bg-gold/10" : "hover:bg-surface-elev",
      ].join(" ")}
      style={{ gridTemplateColumns: "18px 1fr auto" }}
    >
      <input
        type="checkbox"
        id={id}
        aria-label={label}
        checked={checked}
        onChange={onToggle}
        className="sr-only"
      />
      <span
        className={[
          "w-[13px] h-[13px] rounded-[2px] border flex items-center justify-center shrink-0",
          checked ? "bg-gold border-gold" : "bg-transparent border-border-strong",
        ].join(" ")}
        aria-hidden="true"
      >
        {checked && (
          <svg width="9" height="9" viewBox="0 0 9 9" fill="none" stroke="#001A0A" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M1.5 4.5L4 7l4-5" />
          </svg>
        )}
      </span>
      <span className={`text-[12px] ${checked ? "text-gold" : "text-text"}`}>{label}</span>
      {sub && <span className="font-mono text-[10.5px] text-text-3">{sub}</span>}
    </label>
  );
}

// ── Toggle switch ─────────────────────────────────────────────────────────────

function ToggleSwitch({
  label,
  subtitle,
  checked,
  onToggle,
}: {
  label: string;
  subtitle: string;
  checked: boolean;
  onToggle: () => void;
}) {
  return (
    <div
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={onToggle}
      onKeyDown={(e) => e.key === "Enter" && onToggle()}
      tabIndex={0}
      className="flex items-center gap-2.5 py-1 cursor-pointer"
    >
      <span
        className={[
          "w-[30px] h-[17px] rounded-full relative shrink-0 transition-colors",
          checked ? "bg-gold" : "bg-border-strong",
        ].join(" ")}
      >
        <span
          className={[
            "absolute top-0.5 w-[13px] h-[13px] rounded-full bg-[#000] transition-[left]",
            checked ? "left-[15px]" : "left-0.5",
          ].join(" ")}
        />
      </span>
      <div>
        <div className="text-[12px] text-text">{label}</div>
        <div className="font-mono text-[10px] text-text-3 mt-0.5">{subtitle}</div>
      </div>
    </div>
  );
}

// ── Range visual (static SVG-free, CSS-only) ─────────────────────────────────
// Real range sliders are complex; this renders a visual indicator + number
// inputs for USDC range and min-buyers. Replaces the handoff's dual-handle
// slider with two number inputs for test-friendliness.

function PriceRange({
  from,
  to,
  onChange,
}: {
  from: number;
  to: number;
  onChange: (from: number, to: number) => void;
}) {
  const fromPct = (from / 500) * 100;
  const toPct = (to / 500) * 100;
  return (
    <div>
      <div className="relative h-[30px] py-[10px]">
        <div className="absolute left-0 right-0 top-[14px] h-[3px] rounded bg-border-strong" />
        <div
          className="absolute top-[14px] h-[3px] rounded bg-gold"
          style={{ left: `${fromPct}%`, right: `${100 - toPct}%` }}
        />
      </div>
      <div className="flex gap-2 mt-1">
        <input
          type="number"
          aria-label="price from"
          min={0}
          max={to}
          value={from}
          onChange={(e) => onChange(Number(e.target.value), to)}
          className="w-full bg-surface-elev border border-border-strong rounded px-2 py-1 font-mono text-[11px] text-text-2 outline-none focus:border-gold/60"
        />
        <input
          type="number"
          aria-label="price to"
          min={from}
          max={500}
          value={to}
          onChange={(e) => onChange(from, Number(e.target.value))}
          className="w-full bg-surface-elev border border-border-strong rounded px-2 py-1 font-mono text-[11px] text-text-2 outline-none focus:border-gold/60"
        />
      </div>
    </div>
  );
}

function MinBuyersRange({
  value,
  onChange,
}: {
  value: number;
  onChange: (v: number) => void;
}) {
  const pct = Math.min((value / 500) * 100, 100);
  return (
    <div>
      <div className="relative h-[30px] py-[10px]">
        <div className="absolute left-0 right-0 top-[14px] h-[3px] rounded bg-border-strong" />
        <div
          className="absolute left-0 top-[14px] h-[3px] rounded bg-gold"
          style={{ right: `${100 - pct}%` }}
        />
      </div>
      <div className="flex justify-between mt-1 font-mono text-[11px] text-text-2">
        <span>min {value}</span>
        <span className="text-text-3">unlimited</span>
      </div>
      <input
        type="range"
        aria-label="minimum buyers"
        min={0}
        max={500}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-full accent-[var(--gold,#00E676)] mt-1"
      />
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

interface FilterDrawerContentProps {
  filter: FilterState;
  setFilter: (patch: Partial<FilterState>) => void;
  matchCount: number;
  onClose: () => void;
}

export function FilterDrawerContent({
  filter,
  setFilter,
  matchCount,
  onClose,
}: FilterDrawerContentProps) {
  const def = defaultFilterState();

  function clearAll() {
    setFilter({
      assets: def.assets,
      models: def.models,
      styles: def.styles,
      trust: def.trust,
      minBuyers: def.minBuyers,
      priceUsdc: def.priceUsdc,
      sort: def.sort,
    });
  }

  function toggleAsset(sym: string) {
    const next = filter.assets.includes(sym)
      ? filter.assets.filter((a) => a !== sym)
      : [...filter.assets, sym];
    setFilter({ assets: next });
  }

  function toggleModel(m: string) {
    const next = filter.models.includes(m)
      ? filter.models.filter((x) => x !== m)
      : [...filter.models, m];
    setFilter({ models: next });
  }

  function toggleStyle(s: string) {
    const next = filter.styles.includes(s)
      ? filter.styles.filter((x) => x !== s)
      : [...filter.styles, s];
    setFilter({ styles: next });
  }

  return (
    <>
      {/* Header (meta line below title is provided by caller's FilterDrawer shell) */}
      <div className="px-[18px] pb-2 pt-1 font-mono text-[10.5px] text-text-3">
        {filter.assets.length + filter.models.length + filter.styles.length +
          (filter.trust.verifiedOnly ? 1 : 0) +
          (filter.trust.acceptsAgents ? 1 : 0) +
          (filter.trust.auditedOnly ? 1 : 0) > 0 ? (
          <>
            <span className="text-gold">
              {filter.assets.length + filter.models.length + filter.styles.length +
                (filter.trust.verifiedOnly ? 1 : 0) +
                (filter.trust.acceptsAgents ? 1 : 0) +
                (filter.trust.auditedOnly ? 1 : 0)}{" "}
              filters active
            </span>{" "}
            · {matchCount.toLocaleString()} of 1,247 match
          </>
        ) : (
          <span>{matchCount.toLocaleString()} strategies</span>
        )}
      </div>

      {/* Sort by */}
      <DrawerSection title="Sort by">
        <div className="flex flex-col gap-1.5">
          {SORT_OPTIONS.map((o) => (
            <label
              key={o.key}
              className="flex items-center gap-2.5 px-1 py-1.5 cursor-pointer"
            >
              <input
                type="radio"
                name="sort"
                aria-label={o.label}
                checked={filter.sort === o.key}
                onChange={() => setFilter({ sort: o.key })}
                className="sr-only"
              />
              <span
                className={[
                  "w-[13px] h-[13px] rounded-full border flex items-center justify-center shrink-0",
                  filter.sort === o.key
                    ? "border-gold bg-gold/10"
                    : "border-border-strong bg-transparent",
                ].join(" ")}
                aria-hidden="true"
              >
                {filter.sort === o.key && (
                  <span className="w-[5px] h-[5px] rounded-full bg-gold" />
                )}
              </span>
              <span
                className={`text-[12.5px] ${filter.sort === o.key ? "text-text" : "text-text-2"}`}
              >
                {o.label}
              </span>
            </label>
          ))}
        </div>
      </DrawerSection>

      {/* Assets */}
      <DrawerSection
        title="Assets"
        sub={filter.assets.length > 0 ? `${filter.assets.length} selected` : undefined}
      >
        <div className="flex items-center gap-2 px-2 py-1.5 mb-2 border border-border-strong rounded-[3px] bg-surface-elev">
          <svg width="11" height="11" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-text-3 shrink-0" aria-hidden="true">
            <circle cx="6" cy="6" r="4" />
            <path d="M9.5 9.5l2.5 2.5" strokeLinecap="round" />
          </svg>
          <span className="font-mono text-[11px] text-text-3">filter assets…</span>
        </div>
        {ASSET_GROUPS.map((g, gi) => (
          <div key={g.group} className={gi < ASSET_GROUPS.length - 1 ? "mb-1.5" : ""}>
            <div className="flex items-baseline justify-between py-1.5">
              <span className="font-mono text-[9px] tracking-[0.18em] uppercase text-text-3">
                {g.group}
              </span>
              <span className="font-mono text-[9.5px] text-text-4">{g.items.length}</span>
            </div>
            {g.items.map((a) => (
              <CheckRow
                key={a.sym}
                id={`asset-${a.sym}`}
                label={a.sym}
                sub={a.name}
                checked={filter.assets.includes(a.sym)}
                onToggle={() => toggleAsset(a.sym)}
              />
            ))}
          </div>
        ))}
      </DrawerSection>

      {/* Models */}
      <DrawerSection
        title="Models"
        sub={filter.models.length > 0 ? `${filter.models.length} selected` : undefined}
      >
        {MODEL_OPTIONS.map((m) => (
          <CheckRow
            key={m}
            id={`model-${m}`}
            label={m}
            checked={filter.models.includes(m)}
            onToggle={() => toggleModel(m)}
          />
        ))}
      </DrawerSection>

      {/* Style chips */}
      <DrawerSection title="Style">
        <div className="flex gap-1.5 flex-wrap">
          {STYLE_OPTIONS.map((s) => {
            const active = filter.styles.includes(s);
            return (
              <button
                key={s}
                type="button"
                onClick={() => toggleStyle(s)}
                className={[
                  "px-2 py-1 rounded-[3px] border font-mono text-[10.5px] cursor-pointer",
                  active
                    ? "border-gold/30 bg-gold/10 text-gold"
                    : "border-border-strong bg-transparent text-text-2 hover:border-border",
                ].join(" ")}
              >
                {s}
              </button>
            );
          })}
        </div>
      </DrawerSection>

      {/* Trust toggles */}
      <DrawerSection title="Trust">
        <div className="flex flex-col gap-2">
          <ToggleSwitch
            label="Verified only"
            subtitle="green-check strategies"
            checked={filter.trust.verifiedOnly}
            onToggle={() =>
              setFilter({ trust: { ...filter.trust, verifiedOnly: !filter.trust.verifiedOnly } })
            }
          />
          <ToggleSwitch
            label="Accepts agents (x402)"
            subtitle="agent-paid purchase"
            checked={filter.trust.acceptsAgents}
            onToggle={() =>
              setFilter({ trust: { ...filter.trust, acceptsAgents: !filter.trust.acceptsAgents } })
            }
          />
          <ToggleSwitch
            label="Audited only"
            subtitle="creator audit attestation"
            // TODO(Phase 1): auditedOnly has no ListingRow field yet — toggle is visual-only
            checked={filter.trust.auditedOnly}
            onToggle={() =>
              setFilter({ trust: { ...filter.trust, auditedOnly: !filter.trust.auditedOnly } })
            }
          />
        </div>
      </DrawerSection>

      {/* Price range */}
      <DrawerSection title="Price (USDC)">
        <PriceRange
          from={filter.priceUsdc.from}
          to={filter.priceUsdc.to}
          onChange={(from, to) => setFilter({ priceUsdc: { from, to } })}
        />
      </DrawerSection>

      {/* Min buyers */}
      <DrawerSection title="Minimum buyers">
        <MinBuyersRange
          value={filter.minBuyers}
          onChange={(v) => setFilter({ minBuyers: v })}
        />
      </DrawerSection>

      {/* Footer */}
      <div className="px-4 py-3 border-t border-border bg-[#050505] flex items-center gap-2">
        <button
          type="button"
          onClick={clearAll}
          className="text-[11.5px] text-text-3 bg-transparent border-none cursor-pointer underline decoration-dotted underline-offset-[3px] p-0 hover:text-text"
        >
          Clear all
        </button>
        <span className="ml-auto font-mono text-[11px] text-text-3">
          <span className="text-gold">{matchCount.toLocaleString()}</span> matches
        </span>
        <button
          type="button"
          aria-label="apply"
          onClick={onClose}
          className="px-5 py-1.5 rounded bg-gold text-[#001A0A] text-[12px] font-bold hover:opacity-90 transition-opacity"
        >
          Apply
        </button>
      </div>
    </>
  );
}
```

### Step 4: Run to verify it passes

Run: `pnpm exec vitest run src/features/marketplace/routes/browse/browse.test.tsx`
Expected: PASS (all describes — 33 tests).

### Step 5: Commit

```bash
git add src/features/marketplace/routes/browse/FilterDrawerContent.tsx src/features/marketplace/routes/browse/browse.test.tsx
git commit -m "feat(marketplace/f1): FilterDrawerContent — sort/assets/models/style/trust/price/buyers"
```

---

## Task 6: `BrowseRoute` — integrate all sub-components + integration test

**Files:**
- Create: `src/features/marketplace/routes/BrowseRoute.tsx`
- Create: `src/features/marketplace/routes/BrowseRoute.test.tsx`

### Step 1: Write failing integration test

```tsx
// src/features/marketplace/routes/BrowseRoute.test.tsx
import { render, screen, act, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { BrowseRoute } from "./BrowseRoute";

function Wrapper({ children }: { children: React.ReactNode }) {
  return (
    <MemoryRouter initialEntries={["/marketplace"]}>
      <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
        {children}
      </MarketplaceDataProvider>
    </MemoryRouter>
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
      expect(container.querySelectorAll('[data-genart="placeholder"]').length).toBeGreaterThan(0);
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
    expect(screen.getByRole("complementary")).toBeInTheDocument(); // F0 FilterDrawer <aside>
  });

  it("the FilterDrawer shows sort section content", async () => {
    render(<BrowseRoute />, { wrapper: Wrapper });
    const btn = await screen.findByRole("button", { name: /filters/i });
    act(() => btn.click());
    expect(screen.getByText("Sort by")).toBeInTheDocument();
  });

  it("Mine segment shows only the viewer's created listings", async () => {
    render(
      <MemoryRouter initialEntries={["/marketplace?segment=mine"]}>
        <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
          <BrowseRoute />
        </MarketplaceDataProvider>
      </MemoryRouter>
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
```

### Step 2: Run to verify it fails

Run: `pnpm exec vitest run src/features/marketplace/routes/BrowseRoute.test.tsx`
Expected: FAIL — `BrowseRoute` not found.

### Step 3: Implement `BrowseRoute`

```tsx
// src/features/marketplace/routes/BrowseRoute.tsx
// F1 implementation of the /marketplace browse surface.
// Replaces MarketplaceBrowseStub. All data via useMarketplaceData().
// No popups. FilterDrawer is the F0 docked panel; its content is FilterDrawerContent.
import { useEffect, useState, useCallback } from "react";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import { useFilterState } from "@/features/marketplace/hooks/useFilterState";
import { FilterDrawer } from "@/features/marketplace/components/FilterDrawer";
import { HeaderStrip } from "./browse/HeaderStrip";
import { Toolbar } from "./browse/Toolbar";
import { AppliedChips } from "./browse/AppliedChips";
import { LeaderboardRail } from "./browse/LeaderboardRail";
import { ListingCard } from "./browse/ListingCard";
import { FilterDrawerContent } from "./browse/FilterDrawerContent";
import type { ListingRow, SliceId } from "@/features/marketplace/data/types";

function countActiveFilters(filter: ReturnType<typeof useFilterState>["filter"]): number {
  return (
    filter.assets.length +
    filter.models.length +
    filter.styles.length +
    (filter.trust.verifiedOnly ? 1 : 0) +
    (filter.trust.acceptsAgents ? 1 : 0) +
    (filter.trust.auditedOnly ? 1 : 0) +
    (filter.minBuyers > 0 ? 1 : 0) +
    (filter.priceUsdc.from !== 0 || filter.priceUsdc.to !== 500 ? 1 : 0)
  );
}

// List header column labels matching the 8-column grid from the design ref.
function ListHeader() {
  return (
    <div
      className="grid items-center gap-3.5 px-[22px] py-2.5 border-b border-border/50 sticky top-0 bg-bg z-[1]"
      style={{ gridTemplateColumns: "56px 1.8fr 0.75fr 1.1fr 1.05fr 0.6fr 0.85fr 110px" }}
    >
      {["", "Strategy", "Assets", "30d return", "Buyers", "Sharpe", "Price", ""].map(
        (h, i) => (
          <div
            key={i}
            className={`font-mono text-[9px] tracking-[0.2em] uppercase text-text-3 font-semibold ${
              i === 3 || i === 5 ? "text-right" : "text-left"
            }`}
          >
            {h}
          </div>
        )
      )}
    </div>
  );
}

export function BrowseRoute() {
  const mp = useMarketplaceData();
  const { filter, setFilter } = useFilterState();
  const [rows, setRows] = useState<ListingRow[]>([]);
  const [total, setTotal] = useState(0);
  const [matched, setMatched] = useState(0);
  const [drawerOpen, setDrawerOpen] = useState(false);

  // Reload listings whenever filter changes
  useEffect(() => {
    let cancelled = false;
    mp.listListings(filter).then(({ rows: r, total: t, matched: m }) => {
      if (cancelled) return;
      setRows(r);
      setTotal(t);
      setMatched(m);
    });
    return () => {
      cancelled = true;
    };
  }, [mp, filter]);

  const handleBuy = useCallback(
    async (id: string) => {
      // purchaseIntent returns TxRef; in F1 we just call it to confirm the seam works.
      // F6 will route to /marketplace/receipts/:tx.
      await mp.purchaseIntent(id);
      // TODO(F6): navigate(`/marketplace/receipts/${ref.txHash}`)
    },
    [mp]
  );

  const handleSliceClick = useCallback(
    (sliceId: SliceId) => {
      setFilter({ slice: filter.slice === sliceId ? undefined : sliceId });
    },
    [filter.slice, setFilter]
  );

  const filterCount = countActiveFilters(filter);

  return (
    <div className="flex flex-col min-h-0">
      <HeaderStrip />

      <Toolbar
        filter={filter}
        setFilter={setFilter}
        filterCount={filterCount}
        onOpenDrawer={() => setDrawerOpen(true)}
        matchCount={matched}
      />

      <AppliedChips filter={filter} setFilter={setFilter} matchCount={matched} />

      {/* Body: leaderboard rail | list + optional drawer overlay */}
      <div
        className="flex-1 min-h-0 grid overflow-hidden relative"
        style={{ gridTemplateColumns: "232px 1fr" }}
      >
        <LeaderboardRail
          activeSliceId={filter.slice}
          onSliceClick={handleSliceClick}
        />

        {/* List area */}
        <div className="overflow-auto pb-2">
          <ListHeader />
          {rows.length === 0 ? (
            <div className="px-[22px] py-10 text-[13px] text-text-3 text-center">
              No strategies match the current filters.
            </div>
          ) : (
            rows.map((row) => (
              <ListingCard key={row.id} row={row} onBuy={handleBuy} />
            ))
          )}
        </div>

        {/* FilterDrawer docked panel — covers list area, rail stays visible */}
        <FilterDrawer
          open={drawerOpen}
          onClose={() => setDrawerOpen(false)}
          title="Filter strategies"
        >
          <FilterDrawerContent
            filter={filter}
            setFilter={setFilter}
            matchCount={matched}
            onClose={() => setDrawerOpen(false)}
          />
        </FilterDrawer>
      </div>
    </div>
  );
}
```

### Step 4: Run to verify it passes

Run: `pnpm exec vitest run src/features/marketplace/routes/BrowseRoute.test.tsx`
Expected: PASS (9 tests).

Run: `pnpm exec vitest run src/features/marketplace/routes/browse/browse.test.tsx`
Expected: PASS (33 tests — unchanged).

### Step 5: Commit

```bash
git add src/features/marketplace/routes/BrowseRoute.tsx src/features/marketplace/routes/BrowseRoute.test.tsx src/features/marketplace/routes/browse/
git commit -m "feat(marketplace/f1): BrowseRoute — full browse surface with FilterDrawer + Mine segment"
```

---

## Task 7: Wire `routes.tsx` — single-line swap

**Files:**
- Modify: `src/routes.tsx` (ONE line)

This is the only change to `routes.tsx`. It must be a standalone commit so it cannot contend with parallel branches also touching the file.

### Step 1: Confirm current state

Before editing, confirm the current line 59 reads:

```ts
const MarketplaceBrowseStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceBrowseStub })));
```

### Step 2: Apply the single-line change

Replace line 59 **only**. Change:

```ts
const MarketplaceBrowseStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceBrowseStub })));
```

With:

```ts
const MarketplaceBrowseStub = lazy(() => import("./features/marketplace/routes/BrowseRoute").then((m) => ({ default: m.BrowseRoute })));
```

No other lines in `routes.tsx` change. The variable name `MarketplaceBrowseStub` is intentionally kept (it is referenced on line 186 in `{ index: true, element: page(<MarketplaceBrowseStub />) }` — keeping the name avoids touching that line too).

### Step 3: Verify all route + typecheck pass

Run: `pnpm exec vitest run src/routes.test.tsx src/routes-code-splitting.test.ts`
Expected: PASS (existing routing tests still green).

Run: `pnpm exec vitest run src/features/marketplace`
Expected: PASS (all marketplace tests).

Run: `pnpm typecheck`
Expected: PASS.

### Step 4: Commit

```bash
git add src/routes.tsx
git commit -m "feat(marketplace/f1): wire BrowseRoute — replace MarketplaceBrowseStub import (single-line swap)"
```

---

## Task 8: Self-review

Before declaring F1 done, verify all acceptance criteria against running tests and manual checklist.

### Step 1: Run the full marketplace suite

Run: `pnpm exec vitest run src/features/marketplace`
Expected output:

```
✓ src/features/marketplace/data/filter.test.ts                 (5 tests)
✓ src/features/marketplace/data/fixtures/fixtures.test.ts       (6 tests)
✓ src/features/marketplace/data/FixtureMarketplaceData.test.ts  (9 tests)
✓ src/features/marketplace/data/provider.test.tsx               (2 tests)
✓ src/features/marketplace/hooks/useFilterState.test.tsx        (2 tests)
✓ src/features/marketplace/components/GenArtPlaceholder.test.tsx (3 tests)
✓ src/features/marketplace/components/Sparkline.test.tsx        (2 tests)
✓ src/features/marketplace/components/badges.test.tsx           (4 tests)
✓ src/features/marketplace/components/chips.test.tsx            (3 tests)
✓ src/features/marketplace/components/FilterDrawer.test.tsx     (3 tests)
✓ src/features/marketplace/components/ShareableCard.test.tsx    (1 test)
✓ src/features/marketplace/marketplace-routes.test.tsx          (2 tests)
✓ src/features/marketplace/routes/browse/browse.test.tsx        (33 tests)
✓ src/features/marketplace/routes/BrowseRoute.test.tsx          (9 tests)
```

All tests pass. Total: 84 tests.

### Step 2: Typecheck

Run: `pnpm typecheck`
Expected: PASS with no errors.

### Step 3: Acceptance checklist

Verify each item manually or by pointing at the corresponding passing test:

- [ ] Filtered/sorted rows match fixture expectations (BrowseRoute.test.tsx: Mine segment test; browse.test.tsx: applyFilter tests via unit tests).
- [ ] Mine segment shows only the viewer's created listings (BrowseRoute.test.tsx: "Mine segment shows only viewer's created listings").
- [ ] FilterDrawer opens without being a `dialog` role (FilterDrawer.test.tsx: "is a docked complementary panel, not a dialog").
- [ ] Drawer applies sort and asset filters (browse.test.tsx: FilterDrawerContent describes).
- [ ] RemovableChips remove individual filters and Clear all resets (browse.test.tsx: AppliedChips describe).
- [ ] Slice click loads that slice's filter (BrowseRoute.test.tsx: "clicking a leaderboard slice").
- [ ] `[Testnet]` badge renders on every Buy CTA (browse.test.tsx: "renders Buy CTA with [Testnet] indicator").
- [ ] Run free CTA and OPEN pill render for Tier-A / null-price listings (browse.test.tsx: "renders Run free CTA and OPEN pill").
- [ ] No `Dialog`/`Modal`/`Sheet`/`Popover` in any component (verified by grep below).
- [ ] `routes.tsx` modified only at line 59 (single-line swap).

Run: `grep -r "Dialog\|Modal\|Sheet\|Popover" src/features/marketplace/routes/BrowseRoute.tsx src/features/marketplace/routes/browse/ 2>/dev/null | wc -l`
Expected: `0`

---

## Done criteria (F1 complete)

- [ ] `pnpm exec vitest run src/features/marketplace` is green (all 84 tests).
- [ ] `pnpm typecheck` passes.
- [ ] `src/routes.test.tsx` + `src/routes-code-splitting.test.ts` still pass.
- [ ] `/marketplace` renders the full browse surface (header + toolbar + rail + list).
- [ ] `/marketplace?segment=mine` shows only the fixture viewer's 3 created listings.
- [ ] `/marketplace?assets=SOL` shows only SOL strategies.
- [ ] Clicking a rail slice narrows the list.
- [ ] Filters button opens the docked drawer; Apply closes it.
- [ ] Every Buy/Run-free CTA has a `[Testnet]` badge.
- [ ] No `Dialog`/`Modal`/`Sheet`/`Popover` introduced.
- [ ] `routes.tsx` changed in exactly one line (the lazy import for the browse index route).

**Next plan: F2 — Lineage identity + on-chain receipts drawer (`/marketplace/lineage/:name`).**
