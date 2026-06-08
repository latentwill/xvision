import { useCallback, useMemo, useState } from "react";

// ─── Column picker ────────────────────────────────────────────────────────────

export type ListColumnMeta = {
  key: string;
  essential?: boolean;
  defaultOff?: boolean;
  priority?: number;
  estWidth?: number;
};

export type ColumnState = {
  visibleKeys: Set<string>;
  toggle: (key: string) => void;
  reset: () => void;
  isEssential: (key: string) => boolean;
};

function colStorageKey(listId: string) {
  return `xvn:list:${listId}:columns`;
}

function parseStoredKeys(raw: string | null, columns: ListColumnMeta[]): Set<string> {
  if (!raw) return defaultVisibleKeys(columns);
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return defaultVisibleKeys(columns);
    const stored = new Set(parsed.filter((k) => typeof k === "string") as string[]);
    // Always include essentials even if absent from storage.
    columns.filter((c) => c.essential).forEach((c) => stored.add(c.key));
    return stored;
  } catch {
    return defaultVisibleKeys(columns);
  }
}

function defaultVisibleKeys(columns: ListColumnMeta[]): Set<string> {
  return new Set(columns.filter((c) => !c.defaultOff).map((c) => c.key));
}

export function useListColumns(listId: string, columns: ListColumnMeta[]): ColumnState {
  const storageKey = colStorageKey(listId);

  const [visibleKeys, setVisibleKeys] = useState<Set<string>>(() => {
    try {
      return parseStoredKeys(localStorage.getItem(storageKey), columns);
    } catch {
      return defaultVisibleKeys(columns);
    }
  });

  const essentialKeys = useMemo(
    () => new Set(columns.filter((c) => c.essential).map((c) => c.key)),
    [columns],
  );

  const isEssential = useCallback((key: string) => essentialKeys.has(key), [essentialKeys]);

  const toggle = useCallback(
    (key: string) => {
      if (essentialKeys.has(key)) return;
      setVisibleKeys((prev) => {
        const next = new Set(prev);
        if (next.has(key)) {
          next.delete(key);
        } else {
          next.add(key);
        }
        essentialKeys.forEach((k) => next.add(k));
        try {
          localStorage.setItem(storageKey, JSON.stringify([...next]));
        } catch { /* ignore */ }
        return next;
      });
    },
    [essentialKeys, storageKey],
  );

  const reset = useCallback(() => {
    try {
      localStorage.removeItem(storageKey);
    } catch { /* ignore */ }
    setVisibleKeys(defaultVisibleKeys(columns));
  }, [columns, storageKey]);

  return useMemo(
    () => ({ visibleKeys, toggle, reset, isEssential }),
    [visibleKeys, toggle, reset, isEssential],
  );
}

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
