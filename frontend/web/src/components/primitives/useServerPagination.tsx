// useServerPagination — page/pageSize state hook for SERVER-paginated lists.
//
// Lifted out of the now-removed `<ListPagination>` JSX primitive
// (lists-v1 phase 2c). The hook itself is unchanged from the
// QA-round-7 backend-pagination follow-up (#386 gap): consumers feed
// the server's reported `total`, get back `page`/`pageSize`/`limit`/
// `offset` plus setters, and pass `limit`/`offset` into a TanStack
// Query key so each page change is a fresh request.
//
// The matching footer JSX (`<ListPagination>`) was removed because
// `<MListCard>` has no footer slot — the four migrated routes inline
// a tiny pager strip below `<ResponsiveListCard>` instead. `25/50/100`
// page-size options and the `DEFAULT_PAGE_SIZE = 50` baseline live
// here so all four routes stay aligned without a separate constants
// file.

import { useEffect, useState, type ReactNode } from "react";
import { SignalSelectMenu } from "./SignalMenu";

export const DEFAULT_PAGE_SIZE = 50;
export const PAGE_SIZE_OPTIONS: ReadonlyArray<number> = [25, 50, 100];

/// Hook that owns page/pageSize state for SERVER-paginated lists.
///
/// Unlike a client-side slicer, this hook does not touch the array —
/// the caller is expected to feed `limit`/`offset` into a TanStack
/// Query key and rely on the backend to return one page at a time. The
/// returned `total` reflects the server's reported total, not the
/// length of the (partial) `items` array.
///
/// Returns `limit`/`offset` derived from the current page so the
/// caller can pass them to the API client without re-deriving the
/// math. Clamps `page` when the server's reported total shrinks
/// underneath the user (e.g. a filter change).
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

/// Inline pager strip rendered below `<ResponsiveListCard>` on each
/// migrated list route. Replaces the deleted `<ListPagination>` JSX
/// primitive — same UI, just colocated with the hook because there's
/// no `<MListCard>` footer slot to share.
///
/// Hidden for lists short enough to fit on the smallest page size
/// (`PAGE_SIZE_OPTIONS[0]`), so single-page workspaces don't show an
/// inert page-size selector.
export type ServerPagerStripProps = {
  total: number;
  page: number;
  pageSize: number;
  onPageChange: (page: number) => void;
  onPageSizeChange: (size: number) => void;
  /// Optional label override — defaults to "items".
  itemLabel?: string;
  className?: string;
};

export function ServerPagerStrip({
  total,
  page,
  pageSize,
  onPageChange,
  onPageSizeChange,
  itemLabel = "items",
  className = "",
}: ServerPagerStripProps): ReactNode {
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  const safePage = Math.min(Math.max(1, page), pageCount);
  const from = total === 0 ? 0 : (safePage - 1) * pageSize + 1;
  const to = Math.min(total, safePage * pageSize);

  if (total <= PAGE_SIZE_OPTIONS[0]) {
    return null;
  }

  return (
    <div
      className={`mt-3 flex flex-wrap items-center justify-between gap-3 text-[12px] text-text-2 ${className}`}
      data-testid="list-pagination"
    >
      <div className="flex items-center gap-2">
        <span className="text-text-3">Per page</span>
        <SignalSelectMenu
          ariaLabel="Per page"
          value={String(pageSize)}
          options={PAGE_SIZE_OPTIONS.map((opt) => ({
            value: String(opt),
            label: String(opt),
          }))}
          onChange={(value) => {
            const next = Number(value);
            if (!Number.isFinite(next)) return;
            onPageSizeChange(next);
            // Reset to first page when page size changes so the user
            // doesn't end up on a page that no longer exists.
            onPageChange(1);
          }}
          compact
          minWidth={74}
        />
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
