// src/features/marketplace/routes/browse/Toolbar.tsx
// The marketplace browse toolbar (spec 3.1B). Sort is wired to SignalSelectMenu
// (the project-approved portal dropdown with built-in click-outside + Escape —
// NOT a focus-stealing modal, QA5). The Filters button toggles the inline
// filter accordion (no absolute overlay). A segmented List | Index view toggle.
import { useEffect } from "react";
import { SignalSelectMenu } from "@/components/primitives/SignalMenu";
import type { FilterState, SortKey } from "@/features/marketplace/data/types";

export const SORT_LABELS: Record<SortKey, string> = {
  return30d: "30d return",
  sharpe: "Sharpe",
  buyers: "Buyers",
  newest: "Newest",
};

const SEGMENTS: { key: FilterState["segment"]; label: string }[] = [
  { key: "trending", label: "Trending" },
  { key: "new", label: "New" },
  { key: "mine", label: "Mine" },
];

export type BrowseView = "list" | "index";

const VIEW_LABELS: Record<BrowseView, string> = {
  list: "List",
  index: "Index",
};

interface ToolbarProps {
  filter: FilterState;
  setFilter: (patch: Partial<FilterState>) => void;
  filterCount: number;
  /** Toggle the inline filter accordion (no popup). */
  onToggleFilters: () => void;
  filtersOpen: boolean;
  matchCount: number;
  view: BrowseView;
  setView: (v: BrowseView) => void;
  /**
   * When false (real client whose return/sharpe data is all zero), omit the
   * return30d + sharpe sort options so we never sort on zeros (spec 3.1B).
   */
  allowPerformanceSort: boolean;
  /**
   * Fixture/demo client. When the active sort is a performance sort, annotate
   * the Sort control with a quiet DEMO tag so the seeded-RNG-driven ordering is
   * never presented as authoritative live data.
   */
  isDemo?: boolean;
}

export function Toolbar({
  filter,
  setFilter,
  filterCount,
  onToggleFilters,
  filtersOpen,
  view,
  setView,
  allowPerformanceSort,
  isDemo = false,
}: ToolbarProps) {
  // Build the sort options, dropping performance sorts when they'd sort on zeros.
  const sortKeys: SortKey[] = allowPerformanceSort
    ? ["return30d", "sharpe", "buyers", "newest"]
    : ["newest", "buyers"];
  const sortOptions = sortKeys.map((k) => ({ value: k, label: SORT_LABELS[k] }));
  // If the active sort was hidden (performance sort on a zeroed client), fall
  // back to "newest" for the menu's displayed value.
  const sortValue = sortKeys.includes(filter.sort) ? filter.sort : "newest";
  // When a performance sort becomes unavailable (allowPerformanceSort false),
  // reset filter.sort to "newest" so the stored filter state matches the
  // displayed label — prevents stale sort from persisting invisibly.
  useEffect(() => {
    if (!allowPerformanceSort && (filter.sort === "return30d" || filter.sort === "sharpe")) {
      setFilter({ sort: "newest" });
    }
  }, [allowPerformanceSort, filter.sort, setFilter]);
  // On the demo client, performance-based ordering is driven by illustrative
  // returns — surface a quiet DEMO tag so it never reads as authoritative.
  const showSortDemoTag = isDemo && (sortValue === "return30d" || sortValue === "sharpe");

  return (
    <div className="relative border-b border-border">
      <div className="px-4 sm:px-7 py-3.5 flex items-center gap-2.5 sm:gap-3 flex-wrap">
        {/* Segmented: Trending | New | Mine */}
        <div className="inline-flex border border-border rounded overflow-hidden order-1">
          {SEGMENTS.map((s) => {
            const isActive = filter.segment === s.key;
            return (
              <button
                key={s.key}
                type="button"
                aria-label={s.label}
                onClick={() => {
                  // Each segment has a canonical default sort that keeps the label honest.
                  // "trending" → buyers count (interim proxy; bead xvision-ctkm.8 will
                  //   supply a real velocity/score field when marketplace metrics land).
                  // "new"      → newest (id-descending proxy for publishedAt; no timestamp
                  //   on ListingRow yet — documented as a proxy until the field lands).
                  // "mine"     → keep the current sort — no canonical order for "Mine".
                  const canonicalSort: Record<FilterState["segment"], SortKey> = {
                    trending: "buyers",
                    new: "newest",
                    mine: filter.sort,
                  };
                  setFilter({ segment: s.key, sort: canonicalSort[s.key] });
                }}
                className={[
                  "px-3 py-1 text-[12.5px] font-medium cursor-pointer transition-colors",
                  isActive
                    ? "bg-surface-elev text-text"
                    : "text-text-3 hover:text-text-2",
                ].join(" ")}
              >
                {s.label}
              </button>
            );
          })}
        </div>

        {/* Search — full-width own row on mobile, inline flex-1 on desktop */}
        <div className="order-last w-full sm:order-2 sm:w-auto sm:flex-1 sm:min-w-[240px] sm:max-w-[380px] flex items-center gap-2 px-2.5 py-1.5 border border-border rounded bg-surface-elev">
          <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-text-3 shrink-0" aria-hidden="true">
            <circle cx="6" cy="6" r="4" />
            <path d="M9.5 9.5l2.5 2.5" strokeLinecap="round" />
          </svg>
          <input
            type="search"
            placeholder="name · creator · tag"
            value={filter.search}
            onChange={(e) => setFilter({ search: e.target.value })}
            className="flex-1 bg-transparent text-[12px] text-text placeholder:text-text-3 outline-none"
          />
          <kbd className="ml-auto border border-border rounded font-mono text-[9.5px] text-text-3 px-1.5 py-0.5 tracking-[0.06em]">/</kbd>
        </div>

        {/* Sort — wired to SignalSelectMenu (click-outside + Escape built in) */}
        <div className="inline-flex items-center gap-1.5 order-2 sm:order-3">
          <SignalSelectMenu
            label="Sort"
            value={sortValue}
            options={sortOptions}
            onChange={(v) => setFilter({ sort: v as SortKey })}
          />
          {showSortDemoTag && (
            <span
              data-testid="sort-demo-tag"
              title="Demo ordering — returns are illustrative"
              className="font-mono text-[8.5px] tracking-[0.04em] rounded border border-border bg-surface-elev px-1 py-0.5 text-text-3"
            >
              demo
            </span>
          )}
        </div>

        <span className="hidden sm:block w-px h-[22px] bg-border order-3 sm:order-4" />

        {/* Filters — toggles the inline accordion (no overlay) */}
        <button
          type="button"
          aria-label="filters"
          aria-expanded={filtersOpen}
          onClick={onToggleFilters}
          className={[
            "order-3 sm:order-5 inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded border text-[12px] font-medium transition-colors",
            filtersOpen
              ? "border-border-strong bg-surface-elev text-text"
              : "border-border bg-surface-elev text-text-2 hover:border-border-strong hover:text-text",
          ].join(" ")}
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

        {/* View toggle: List | Index */}
        <div className="order-4 sm:order-6 ml-auto inline-flex border border-border rounded overflow-hidden">
          {(["list", "index"] as BrowseView[]).map((v) => {
            const isActive = view === v;
            return (
              <button
                key={v}
                type="button"
                aria-label={`${v} view`}
                aria-pressed={isActive}
                onClick={() => setView(v)}
                className={[
                  "px-2.5 py-1 text-[12px] font-medium cursor-pointer transition-colors",
                  isActive
                    ? "bg-surface-elev text-text"
                    : "text-text-3 hover:text-text-2",
                ].join(" ")}
              >
                {VIEW_LABELS[v]}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
