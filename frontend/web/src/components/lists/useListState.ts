import { useCallback, useMemo, useState } from "react";

export type FilterOption = { value: string; label: string };

export type FilterDef = {
  id: string;
  label: string;
  options: FilterOption[];
  defaultValue?: string;
  icon?: string;
};

export type SortOption = { value: string; label: string };

export type ActiveFilter = {
  def: FilterDef;
  value: string;
  setValue: (v: string) => void;
};

export type ListSearchState = {
  value: string;
  setValue: (s: string) => void;
  placeholder?: string;
};

export type ListSortState = {
  value: string;
  setValue: (v: string) => void;
  options: SortOption[];
};

export type ListState<T> = {
  search: ListSearchState;
  filters: ActiveFilter[];
  sort: ListSortState;
  rows: T[];
  totalRows: number;
  clearAll: () => void;
};

export const LIST_STD_DEFAULT_SORT: SortOption[] = [
  { value: "added", label: "Recently added" },
  { value: "added-asc", label: "Oldest first" },
  { value: "updated", label: "Recently updated" },
  { value: "name", label: "Name A → Z" },
  { value: "name-desc", label: "Name Z → A" },
];

export function useListState<T>(opts: {
  rows: T[];
  filters?: FilterDef[];
  sortOptions?: SortOption[];
  filterFn?: (row: T, query: string, values: Record<string, string>) => boolean;
  sortFn?: (rows: T[], sortKey: string) => T[];
  initialSort?: string;
}): ListState<T> {
  const filterDefs = opts.filters ?? [];
  const sortOptions = opts.sortOptions ?? LIST_STD_DEFAULT_SORT;

  const [search, setSearch] = useState("");
  const [filterValues, setFilterValues] = useState<Record<string, string>>(() => {
    const o: Record<string, string> = {};
    filterDefs.forEach((f) => {
      o[f.id] = f.defaultValue ?? f.options[0]?.value ?? "";
    });
    return o;
  });
  const [sortKey, setSortKey] = useState(opts.initialSort ?? sortOptions[0]?.value ?? "");

  const setFilterValue = useCallback(
    (id: string, value: string) =>
      setFilterValues((s) => ({ ...s, [id]: value })),
    [],
  );

  const filters: ActiveFilter[] = useMemo(
    () =>
      filterDefs.map((def) => ({
        def,
        value: filterValues[def.id] ?? def.options[0]?.value ?? "",
        setValue: (v: string) => setFilterValue(def.id, v),
      })),
    [filterDefs, filterValues, setFilterValue],
  );

  const derived = useMemo(() => {
    let out = opts.rows;
    if (opts.filterFn) {
      out = out.filter((r) => opts.filterFn!(r, search, filterValues));
    }
    if (opts.sortFn) {
      out = opts.sortFn([...out], sortKey);
    }
    return out;
  }, [opts.rows, opts.filterFn, opts.sortFn, search, filterValues, sortKey]);

  const clearAll = useCallback(() => {
    setSearch("");
    setFilterValues(() => {
      const o: Record<string, string> = {};
      filterDefs.forEach((f) => {
        o[f.id] = f.defaultValue ?? f.options[0]?.value ?? "";
      });
      return o;
    });
  }, [filterDefs]);

  return {
    search: { value: search, setValue: setSearch },
    filters,
    sort: { value: sortKey, setValue: setSortKey, options: sortOptions },
    rows: derived,
    totalRows: opts.rows.length,
    clearAll,
  };
}

export function isFilterActive(f: ActiveFilter): boolean {
  const defaultValue = f.def.defaultValue ?? f.def.options[0]?.value ?? "";
  return f.value !== defaultValue;
}
