import {
  isFilterActive,
  type ActiveFilter,
  type ListSearchState,
} from "./useListState";

export function ListActiveChips({
  search,
  filters = [],
  clearAll,
}: {
  search?: ListSearchState;
  filters?: ActiveFilter[];
  clearAll?: () => void;
}) {
  const activeFilters = filters.filter(isFilterActive);
  const hasSearch = !!(search && search.value.trim());

  if (!hasSearch && activeFilters.length === 0) return null;

  return (
    <div
      role="region"
      aria-label="Active filters"
      className="flex items-center gap-1.5 flex-wrap"
    >
      <span className="text-[10.5px] tracking-[0.1em] uppercase text-text-3 mr-0.5">
        Active
      </span>
      {hasSearch && (
        <button
          type="button"
          onClick={() => search!.setValue("")}
          className="inline-flex items-center gap-1.5 h-[22px] pl-2 pr-1 rounded-[3px] border border-gold/35 bg-gold/10 text-gold text-[11px] font-sans cursor-pointer transition-colors hover:bg-gold/15"
        >
          <span className="text-text-3">search</span>
          <span className="font-medium">&ldquo;{search!.value}&rdquo;</span>
          <span className="text-base leading-none ml-0.5 px-1 text-text-2 group-hover:text-text">
            ×
          </span>
        </button>
      )}
      {activeFilters.map((f) => {
        const opt = f.def.options.find((o) => o.value === f.value);
        const defaultValue =
          f.def.defaultValue ?? f.def.options[0]?.value ?? "";
        return (
          <button
            key={f.def.id}
            type="button"
            onClick={() => f.setValue(defaultValue)}
            className="inline-flex items-center gap-1.5 h-[22px] pl-2 pr-1 rounded-[3px] border border-gold/35 bg-gold/10 text-gold text-[11px] font-sans cursor-pointer transition-colors hover:bg-gold/15"
          >
            <span className="text-text-3">{f.def.label.toLowerCase()}</span>
            <span className="font-medium">{opt?.label ?? f.value}</span>
            <span className="text-base leading-none ml-0.5 px-1 text-text-2">
              ×
            </span>
          </button>
        );
      })}
      {clearAll && (
        <button
          type="button"
          onClick={clearAll}
          className="border-none bg-transparent cursor-pointer text-text-3 font-sans text-[11.5px] px-1 underline decoration-text-4 underline-offset-[3px] hover:text-text"
        >
          Clear all
        </button>
      )}
    </div>
  );
}
