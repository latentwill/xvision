// ListPagination — tactical page-size picker + page-nav strip for list pages.
//
// Shipped as part of the QA-round-7 list wave (F-4) to give long list pages
// (eval runs, strategies, scenarios, agents) a uniform 25/50/100 page-size
// picker and prev/next nav. Inline, no popups (per the frontend rule).
//
// Operates on an already-fetched, server-sorted array. Pure presentation +
// arithmetic; consumers feed `total` and the current `page`/`pageSize`,
// receive `onPageChange` / `onPageSizeChange`, and slice their own array.
// `useListPagination` is the matching state hook — keeps the slice math in
// one place and clamps the page index when the underlying data shrinks
// (e.g. a status filter trims the list while the user was on page 3).
//
// The whole component is a tactical placeholder. The unified list component
// planned in team/intake/2026-05-19-list-component-design-intake.md will
// subsume it; until that lands, this is the smallest possible thing that
// gets the recency-default sort (F-3) and page-size picker (F-4) onto every
// list page without forking each page's existing table.

import { useEffect, useMemo, useState } from "react";

export const DEFAULT_PAGE_SIZE = 50;
export const PAGE_SIZE_OPTIONS: ReadonlyArray<number> = [25, 50, 100];

type Props = {
  total: number;
  page: number;
  pageSize: number;
  onPageChange: (page: number) => void;
  onPageSizeChange: (size: number) => void;
  /** Optional label override — defaults to "items". */
  itemLabel?: string;
  className?: string;
};

export function ListPagination({
  total,
  page,
  pageSize,
  onPageChange,
  onPageSizeChange,
  itemLabel = "items",
  className = "",
}: Props) {
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  const safePage = Math.min(Math.max(1, page), pageCount);
  const from = total === 0 ? 0 : (safePage - 1) * pageSize + 1;
  const to = Math.min(total, safePage * pageSize);

  // Don't render anything for trivially small lists — the picker only
  // matters once you're past one page on the smallest size.
  if (total <= PAGE_SIZE_OPTIONS[0]) {
    return null;
  }

  return (
    <div
      className={`mt-3 flex flex-wrap items-center justify-between gap-3 text-[12px] text-text-2 ${className}`}
      data-testid="list-pagination"
    >
      <div className="flex items-center gap-2">
        <label
          htmlFor="list-pagination-size"
          className="text-text-3"
        >
          Per page
        </label>
        <select
          id="list-pagination-size"
          value={pageSize}
          onChange={(e) => {
            const next = Number(e.target.value);
            if (!Number.isFinite(next)) return;
            onPageSizeChange(next);
            // Reset to first page when page size changes so the user
            // doesn't end up on a page that no longer exists.
            onPageChange(1);
          }}
          className="rounded border border-border bg-surface-card px-2 py-1 text-[12px] text-text outline-none focus:border-text-3"
        >
          {PAGE_SIZE_OPTIONS.map((opt) => (
            <option key={opt} value={opt}>
              {opt}
            </option>
          ))}
        </select>
        <span className="text-text-3" aria-live="polite">
          {total === 0
            ? `0 ${itemLabel}`
            : `${from}–${to} of ${total} ${itemLabel}`}
        </span>
      </div>

      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={() => onPageChange(Math.max(1, safePage - 1))}
          disabled={safePage <= 1}
          className="rounded border border-border px-2.5 py-1 text-[12px] text-text-2 transition-colors hover:border-text-3 hover:text-text disabled:cursor-not-allowed disabled:opacity-40"
          aria-label="Previous page"
        >
          ← Prev
        </button>
        <span className="font-mono tabular-nums text-text-3">
          page {safePage} / {pageCount}
        </span>
        <button
          type="button"
          onClick={() => onPageChange(Math.min(pageCount, safePage + 1))}
          disabled={safePage >= pageCount}
          className="rounded border border-border px-2.5 py-1 text-[12px] text-text-2 transition-colors hover:border-text-3 hover:text-text disabled:cursor-not-allowed disabled:opacity-40"
          aria-label="Next page"
        >
          Next →
        </button>
      </div>
    </div>
  );
}

/// Hook that owns page/pageSize state for a list and returns the visible
/// slice. Clamps `page` when `total` shrinks underneath the user so the UI
/// can't get stuck on an empty page after a filter change.
export function useListPagination<T>(
  items: ReadonlyArray<T>,
  initialPageSize: number = DEFAULT_PAGE_SIZE,
): {
  visible: T[];
  page: number;
  pageSize: number;
  setPage: (p: number) => void;
  setPageSize: (s: number) => void;
  total: number;
} {
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(initialPageSize);

  const total = items.length;
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  const safePage = Math.min(Math.max(1, page), pageCount);

  // If the data shrinks and our recorded page is now out-of-range, clamp it
  // so the next render lands on a valid page. Effects rather than inline
  // state writes to keep React's render purity intact.
  useEffect(() => {
    if (page !== safePage) {
      setPage(safePage);
    }
  }, [page, safePage]);

  const visible = useMemo(
    () => items.slice((safePage - 1) * pageSize, safePage * pageSize),
    [items, safePage, pageSize],
  );

  return {
    visible: visible as T[],
    page: safePage,
    pageSize,
    setPage,
    setPageSize,
    total,
  };
}

/// Hook that owns page/pageSize state for SERVER-paginated lists.
///
/// Unlike `useListPagination`, this hook does not slice an array — the
/// caller is expected to feed `limit`/`offset` into a TanStack Query
/// key and rely on the backend to return one page at a time. The
/// returned `total` reflects the server's reported total, not the
/// length of the (partial) `items` array.
///
/// Returns `limit`/`offset` derived from the current page so the
/// caller can pass them to the API client without re-deriving the
/// math. Clamps `page` when the server's reported total shrinks
/// underneath the user (e.g. a filter change), mirroring the
/// client-side hook's semantics.
export function useServerPagination(
  total: number,
  initialPageSize: number = DEFAULT_PAGE_SIZE,
): {
  page: number;
  pageSize: number;
  setPage: (p: number) => void;
  setPageSize: (s: number) => void;
  total: number;
  limit: number;
  offset: number;
} {
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(initialPageSize);

  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  const safePage = Math.min(Math.max(1, page), pageCount);

  useEffect(() => {
    if (page !== safePage) {
      setPage(safePage);
    }
  }, [page, safePage]);

  const limit = pageSize;
  const offset = (safePage - 1) * pageSize;

  return {
    page: safePage,
    pageSize,
    setPage,
    setPageSize,
    total,
    limit,
    offset,
  };
}
