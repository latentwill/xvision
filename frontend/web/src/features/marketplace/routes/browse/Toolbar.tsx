// src/features/marketplace/routes/browse/Toolbar.tsx
import type { FilterState, SortKey } from "@/features/marketplace/data/types";

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
