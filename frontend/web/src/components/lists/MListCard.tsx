import { useState } from "react";
import type { ReactNode } from "react";

import { Icon } from "@/components/primitives/Icon";

import { MListSheet, activeFilterCount } from "./MListSheet";
import {
  isFilterActive,
  type ActiveFilter,
  type ListSearchState,
  type ListSortState,
} from "./useListState";

export type MListCardProps<T> = {
  title?: ReactNode;
  count?: number;
  subtitle?: ReactNode;
  rightAction?: ReactNode;
  toolbar: {
    search?: ListSearchState;
    filters?: ActiveFilter[];
    sort?: ListSortState;
    clearAll?: () => void;
  };
  rows: T[];
  renderRow: (row: T, index: number) => ReactNode;
  loading?: boolean;
  error?: { message?: string; retry?: () => void } | null;
  empty?: ReactNode;
  emptyAction?: ReactNode;
  pad?: boolean;
  className?: string;
};

export function MListCard<T>(props: MListCardProps<T>) {
  const {
    title,
    count,
    subtitle,
    rightAction,
    toolbar,
    rows,
    renderRow,
    loading = false,
    error = null,
    empty = "No matches.",
    emptyAction,
    pad = true,
    className = "",
  } = props;

  const [sheet, setSheet] = useState<null | "filters" | "sort">(null);
  const filters = toolbar.filters ?? [];
  const filterCount = activeFilterCount(filters);
  const sortOptions = toolbar.sort?.options ?? [];
  const currentSort =
    sortOptions.find((o) => o.value === toolbar.sort?.value) ?? sortOptions[0];

  return (
    <div className={`flex flex-col h-full min-h-0 bg-bg ${className}`}>
      {(title || count != null || subtitle || rightAction) && (
        <div className="flex items-baseline justify-between gap-2.5 px-4 pt-4 pb-1.5">
          <div className="flex items-baseline gap-2 min-w-0">
            {title && (
              <h2 className="m-0 font-sans font-medium text-[26px] tracking-tight text-text truncate">
                {title}
              </h2>
            )}
            {count != null && (
              <span className="inline-flex items-center justify-center min-w-[22px] h-5 px-1.5 rounded-full bg-gold/15 border border-gold/35 text-gold font-mono text-[11px] tabular-nums">
                {count}
              </span>
            )}
            {subtitle && (
              <span className="font-mono text-[11.5px] text-text-3 ml-1 truncate">
                {subtitle}
              </span>
            )}
          </div>
          {rightAction}
        </div>
      )}

      <div className="px-4 pt-1.5 pb-2 flex flex-col gap-2 border-b border-border-soft">
        {toolbar.search && (
          <MSearch
            value={toolbar.search.value}
            onChange={toolbar.search.setValue}
            placeholder={toolbar.search.placeholder}
          />
        )}
        <MControls
          filterCount={filterCount}
          sortLabel={currentSort?.label ?? "Sort"}
          onOpenFilters={() => setSheet("filters")}
          onOpenSort={() => setSheet("sort")}
        />
        <MChips
          search={toolbar.search}
          filters={filters}
          clearAll={toolbar.clearAll}
        />
      </div>

      <div
        className={`flex-1 overflow-y-auto flex flex-col ${pad ? "px-3 pt-1 pb-6 gap-1.5" : ""}`}
      >
        {loading ? (
          <MSkeletonRows />
        ) : error ? (
          <MErrorState error={error} />
        ) : rows.length === 0 ? (
          <MEmptyState message={empty} action={emptyAction} />
        ) : (
          rows.map((r, i) => renderRow(r, i))
        )}
      </div>

      <MListSheet
        open={sheet !== null}
        focus={sheet ?? "filters"}
        onClose={() => setSheet(null)}
        filters={filters}
        sort={toolbar.sort}
        resultCount={rows.length}
        clearAll={toolbar.clearAll}
      />
    </div>
  );
}

function MSearch({
  value,
  onChange,
  placeholder = "Search…",
}: {
  value: string;
  onChange: (s: string) => void;
  placeholder?: string;
}) {
  return (
    <div className="flex items-center gap-2.5 h-[38px] px-3 bg-surface-elev border border-border rounded-full focus-within:border-gold-soft">
      <Icon name="search" size={14} className="text-text-3" />
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        spellCheck={false}
        className="flex-1 min-w-0 bg-transparent border-none outline-none text-text font-sans text-[14px] p-0 placeholder:text-text-3"
      />
      {value && (
        <button
          type="button"
          onClick={() => onChange("")}
          aria-label="Clear search"
          className="border-none bg-transparent cursor-pointer text-text-3 text-[20px] leading-none px-1"
        >
          ×
        </button>
      )}
    </div>
  );
}

function MControls({
  filterCount,
  sortLabel,
  onOpenFilters,
  onOpenSort,
}: {
  filterCount: number;
  sortLabel: string;
  onOpenFilters: () => void;
  onOpenSort: () => void;
}) {
  const filtersActive = filterCount > 0;
  return (
    <div className="flex gap-2">
      <button
        type="button"
        onClick={onOpenFilters}
        data-active={filtersActive || undefined}
        className="flex-none inline-flex items-center gap-1.5 h-8 px-3 bg-surface-card border border-border rounded-full text-text-2 font-sans text-[13px] cursor-pointer data-[active]:bg-gold/[0.06] data-[active]:border-gold/45 data-[active]:text-gold"
      >
        <Icon
          name="sliders"
          size={13}
          className={filtersActive ? "text-gold" : "text-text-2"}
        />
        <span>Filter</span>
        {filtersActive && (
          <span className="inline-flex items-center justify-center min-w-[18px] h-[18px] px-[5px] rounded-full bg-gold text-bg font-mono text-[10.5px] font-semibold">
            {filterCount}
          </span>
        )}
      </button>
      <button
        type="button"
        onClick={onOpenSort}
        className="flex-1 inline-flex items-center justify-between gap-1.5 h-8 px-3 bg-surface-card border border-border rounded-full text-text-2 font-sans text-[13px] cursor-pointer"
      >
        <span className="text-text-3 text-[11.5px] tracking-wide">Sort</span>
        <span className="flex-1 text-left text-text font-medium truncate">
          {sortLabel}
        </span>
        <svg
          width="9"
          height="9"
          viewBox="0 0 16 16"
          fill="none"
          aria-hidden
          className="shrink-0 text-text-3"
        >
          <path
            d="M4 6l4 4 4-4"
            stroke="currentColor"
            strokeWidth="1.6"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>
    </div>
  );
}

function MChips({
  search,
  filters,
  clearAll,
}: {
  search?: ListSearchState;
  filters: ActiveFilter[];
  clearAll?: () => void;
}) {
  const active = filters.filter(isFilterActive);
  const hasSearch = !!(search && search.value.trim());
  if (!hasSearch && active.length === 0) return null;
  return (
    <div
      role="region"
      aria-label="Active filters"
      className="flex gap-1.5 flex-wrap mt-0.5"
    >
      {hasSearch && (
        <button
          type="button"
          onClick={() => search!.setValue("")}
          className="inline-flex items-center gap-1.5 h-6 pl-2.5 pr-1 rounded-full border border-gold/35 bg-gold/10 text-gold font-sans text-[11.5px] cursor-pointer"
        >
          <span className="text-text-3">&ldquo;{search!.value}&rdquo;</span>
          <span className="text-[14px] px-1 leading-none">×</span>
        </button>
      )}
      {active.map((f) => {
        const defaultValue =
          f.def.defaultValue ?? f.def.options[0]?.value ?? "";
        const opt = f.def.options.find((o) => o.value === f.value);
        return (
          <button
            key={f.def.id}
            type="button"
            onClick={() => f.setValue(defaultValue)}
            className="inline-flex items-center gap-1.5 h-6 pl-2.5 pr-1 rounded-full border border-gold/35 bg-gold/10 text-gold font-sans text-[11.5px] cursor-pointer"
          >
            <span className="text-text-3">{f.def.label.toLowerCase()}</span>
            <span className="font-medium">{opt?.label ?? f.value}</span>
            <span className="text-[14px] px-1 leading-none">×</span>
          </button>
        );
      })}
      {clearAll && (hasSearch || active.length > 1) && (
        <button
          type="button"
          onClick={clearAll}
          className="border-none bg-transparent cursor-pointer text-text-3 font-sans text-[11.5px] px-1.5 underline decoration-text-4 underline-offset-[3px]"
        >
          Clear
        </button>
      )}
    </div>
  );
}

function MSkeletonRows() {
  return (
    <>
      {Array.from({ length: 4 }).map((_, i) => (
        <div
          key={i}
          aria-hidden
          className="h-16 bg-surface-card border border-border rounded-lg animate-pulse"
        />
      ))}
    </>
  );
}

function MEmptyState({
  message,
  action,
}: {
  message: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="py-9 px-5 text-center text-text-3 text-[13px]">
      <div>{message}</div>
      {action && (
        <div className="mt-3 inline-flex items-center justify-center">
          {action}
        </div>
      )}
    </div>
  );
}

function MErrorState({
  error,
}: {
  error: { message?: string; retry?: () => void };
}) {
  return (
    <div className="py-9 px-5 text-center">
      <div className="text-text-3 text-[13px]">
        {error.message ?? "Couldn't load this list."}
      </div>
      {error.retry && (
        <button
          type="button"
          onClick={error.retry}
          className="mt-3 inline-flex items-center gap-1.5 px-3 h-8 border border-border-strong rounded-full bg-transparent text-text-2 text-[12px] cursor-pointer"
        >
          Retry
        </button>
      )}
    </div>
  );
}
