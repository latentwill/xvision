import { forwardRef, useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";

import { ListToolbar, type ListToolbarProps } from "./ListToolbar";
import type { ColumnState } from "./useListState";

export type ListColumn = {
  key: string;
  label: ReactNode;
  align?: "left" | "right" | "center";
  width?: number | string;
  essential?: boolean;
  defaultOff?: boolean;
  priority?: number;
  estWidth?: number;
};

export type ListCardProps<T> = {
  listId?: string;
  title?: ReactNode;
  count?: number;
  subtitle?: ReactNode;
  density?: "full" | "compact";
  toolbar?: Omit<ListToolbarProps, "density">;
  columns?: ListColumn[];
  columnState?: ColumnState;
  rows: T[];
  renderRow: (row: T, index: number) => ReactNode;
  actions?: ReactNode;
  footer?: ReactNode;
  className?: string;
  style?: CSSProperties;
  loading?: boolean;
  error?: { message?: string; retry?: () => void } | null;
  empty?: ReactNode;
  emptyAction?: ReactNode;
};

export const ListCard = forwardRef(function ListCard<T>(
  props: ListCardProps<T>,
  searchRef: React.Ref<HTMLInputElement>,
) {
  const {
    listId,
    title,
    count,
    subtitle,
    density: densityProp = "full",
    toolbar,
    columns = [],
    columnState,
    rows,
    renderRow,
    actions,
    footer,
    className = "",
    style,
    loading = false,
    error = null,
    empty,
    emptyAction,
  } = props;

  const density = useResolvedDensity(listId, densityProp);
  const compact = density === "compact";

  // ── Column visibility ──────────────────────────────────────────────────────
  const [autoHiddenKeys, setAutoHiddenKeys] = useState<Set<string>>(new Set());

  const visibleColumns = columns.filter((c) => {
    if (columnState && !columnState.visibleKeys.has(c.key)) return false;
    if (autoHiddenKeys.has(c.key)) return false;
    return true;
  });

  // ── Scroll affordance ──────────────────────────────────────────────────────
  const scrollRef = useRef<HTMLDivElement>(null);
  const [atStart, setAtStart] = useState(true);
  const [atEnd, setAtEnd] = useState(true);
  const [overflowing, setOverflowing] = useState(false);

  const updateScrollState = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    const overflow = el.scrollWidth > el.clientWidth + 1;
    setOverflowing(overflow);
    setAtStart(el.scrollLeft <= 1);
    setAtEnd(el.scrollLeft + el.clientWidth >= el.scrollWidth - 1);
  }, []);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      updateScrollState();
      // Auto-hide: drop non-essential columns by priority when overflowing.
      if (!columnState) return;
      const totalEst = columns
        .filter((c) => columnState.visibleKeys.has(c.key))
        .reduce((sum, c) => sum + (c.estWidth ?? 120), 0);
      if (totalEst <= el.clientWidth) {
        setAutoHiddenKeys(new Set());
        return;
      }
      const candidates = columns
        .filter((c) => !c.essential && columnState.visibleKeys.has(c.key))
        .sort((a, b) => (b.priority ?? 0) - (a.priority ?? 0));
      const hidden = new Set<string>();
      let remaining = totalEst;
      for (const c of candidates) {
        if (remaining <= el.clientWidth) break;
        hidden.add(c.key);
        remaining -= c.estWidth ?? 120;
      }
      setAutoHiddenKeys(hidden);
    });
    ro.observe(el);
    updateScrollState();
    return () => ro.disconnect();
  }, [columns, columnState, updateScrollState]);

  const nudge = useCallback((dir: 1 | -1) => {
    const el = scrollRef.current;
    if (!el) return;
    const behavior = window.matchMedia("(prefers-reduced-motion: reduce)").matches
      ? ("instant" as ScrollBehavior)
      : "smooth";
    el.scrollBy({ left: dir * 240, behavior });
  }, []);

  const colSpan = Math.max(visibleColumns.length, 1);

  return (
    <div
      data-density={density}
      style={style}
      className={`flex flex-col bg-surface-card border border-border rounded-card ${className}`}
    >
      {(title != null || actions != null || count != null || subtitle != null) && (
        <div
          className={`flex items-center justify-between gap-4 px-5 ${compact ? "pt-3 pb-2" : "pt-4 pb-2"}`}
        >
          <div className="flex items-baseline gap-2.5 min-w-0">
            {title != null && (
              <h2 className="m-0 font-sans font-medium text-[22px] tracking-tight text-text truncate">
                {title}
              </h2>
            )}
            {count != null && (
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-sm border border-border text-text-2 font-mono text-[11px] -translate-y-0.5">
                {count}
              </span>
            )}
            {subtitle != null && (
              <span className="text-text-3 text-[12.5px] ml-1 truncate">
                {subtitle}
              </span>
            )}
          </div>
          {actions != null && (
            <div className="flex items-center gap-2 shrink-0">{actions}</div>
          )}
        </div>
      )}
      {toolbar && (
        <div className={`px-5 ${compact ? "pt-1 pb-2.5" : "pt-1 pb-3.5"}`}>
          <ListToolbar
            ref={searchRef}
            {...toolbar}
            density={density}
            columnState={columnState}
            columns={columns}
            autoHiddenKeys={autoHiddenKeys}
          />
        </div>
      )}

      {/* Scroll track with fade overlays and nudge arrows */}
      <div className="relative border-t border-border-soft">
        {/* Left fade */}
        {overflowing && !atStart && (
          <div
            aria-hidden
            className="pointer-events-none absolute left-0 top-0 bottom-0 w-12 z-10"
            style={{ background: "linear-gradient(to right, var(--surface-card), transparent)" }}
          />
        )}
        {/* Right fade */}
        {overflowing && !atEnd && (
          <div
            aria-hidden
            className="pointer-events-none absolute right-0 top-0 bottom-0 w-12 z-10"
            style={{ background: "linear-gradient(to left, var(--surface-card), transparent)" }}
          />
        )}
        {/* Left nudge */}
        {overflowing && !atStart && (
          <button
            type="button"
            aria-label="Scroll left"
            onClick={() => nudge(-1)}
            className="absolute left-1 top-1/2 -translate-y-1/2 z-20 flex h-6 w-6 items-center justify-center rounded-sm border border-border bg-surface-panel text-text-3 hover:text-text hover:border-text-3 transition-colors"
          >
            <svg viewBox="0 0 16 16" fill="currentColor" className="w-3 h-3">
              <path d="M10 12L6 8l4-4" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </button>
        )}
        {/* Right nudge */}
        {overflowing && !atEnd && (
          <button
            type="button"
            aria-label="Scroll right"
            onClick={() => nudge(1)}
            className="absolute right-1 top-1/2 -translate-y-1/2 z-20 flex h-6 w-6 items-center justify-center rounded-sm border border-border bg-surface-panel text-text-3 hover:text-text hover:border-text-3 transition-colors"
          >
            <svg viewBox="0 0 16 16" fill="currentColor" className="w-3 h-3">
              <path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </button>
        )}

        <div
          ref={scrollRef}
          className="overflow-x-auto xvn-scroll"
          onScroll={updateScrollState}
        >
          <table className="w-full min-w-max border-collapse">
            {visibleColumns.length > 0 && (
              <thead>
                <tr
                  style={{ position: "sticky", top: 0, zIndex: 1, background: "var(--surface-card)" }}
                >
                  {visibleColumns.map((c, i) => (
                    <th
                      key={c.key}
                      style={{
                        textAlign: c.align ?? "left",
                        width: c.width,
                        paddingLeft: i === 0 ? 20 : undefined,
                        paddingRight: i === visibleColumns.length - 1 ? 20 : undefined,
                      }}
                      className="font-mono text-[10.5px] uppercase tracking-wider text-text-3 py-2 border-b border-border-soft"
                    >
                      {c.label}
                    </th>
                  ))}
                </tr>
              </thead>
            )}
            <tbody>
              {loading ? (
                <ListSkeleton columnSpan={colSpan} compact={compact} />
              ) : error ? (
                <ListErrorRow columnSpan={colSpan} error={error} />
              ) : rows.length === 0 ? (
                <ListEmptyRow
                  columnSpan={colSpan}
                  message={empty ?? "No results."}
                  action={emptyAction}
                  compact={compact}
                />
              ) : (
                rows.map((r, i) => renderRow(r, i))
              )}
            </tbody>
          </table>
        </div>
      </div>

      {footer != null && (
        <div className="px-5 py-2.5 border-t border-border-soft text-text-3 text-[12px] flex items-center justify-between">
          {footer}
        </div>
      )}
    </div>
  );
}) as <T>(
  props: ListCardProps<T> & { ref?: React.Ref<HTMLInputElement> },
) => JSX.Element;

function ListSkeleton({
  columnSpan,
  compact,
}: {
  columnSpan: number;
  compact: boolean;
}) {
  const rowCount = compact ? 3 : 6;
  return (
    <>
      {Array.from({ length: rowCount }).map((_, i) => (
        <tr key={i} aria-hidden>
          <td colSpan={columnSpan} className="px-5 py-3">
            <div className="h-4 w-full bg-surface-elev/80 rounded-sm animate-pulse" />
          </td>
        </tr>
      ))}
    </>
  );
}

function ListEmptyRow({
  columnSpan,
  message,
  action,
  compact,
}: {
  columnSpan: number;
  message: ReactNode;
  action?: ReactNode;
  compact: boolean;
}) {
  return (
    <tr>
      <td
        colSpan={columnSpan}
        className={`text-center text-text-3 ${compact ? "py-6 px-5" : "py-7 px-5"}`}
      >
        <div className="text-[13px]">{message}</div>
        {action != null && (
          <div className="mt-2.5 inline-flex items-center justify-center">
            {action}
          </div>
        )}
      </td>
    </tr>
  );
}

function ListErrorRow({
  columnSpan,
  error,
}: {
  columnSpan: number;
  error: { message?: string; retry?: () => void };
}) {
  return (
    <tr>
      <td colSpan={columnSpan} className="text-center px-5 py-7">
        <div className="text-text-3 text-[13px]">
          {error.message ?? "Couldn't load this list."}
        </div>
        {error.retry && (
          <button
            type="button"
            onClick={error.retry}
            className="mt-2.5 inline-flex items-center gap-1.5 px-3 h-7 border border-border-strong rounded-sm bg-transparent text-text-2 text-[12px] cursor-pointer hover:text-text hover:border-text-3"
          >
            Retry
          </button>
        )}
      </td>
    </tr>
  );
}

function densityKey(listId: string): string {
  return `xvn:list:${listId}:density`;
}

function readPersistedDensity(
  listId: string | undefined,
): "full" | "compact" | null {
  if (!listId || typeof window === "undefined") return null;
  try {
    const v = window.localStorage.getItem(densityKey(listId));
    if (v === "full" || v === "compact") return v;
  } catch {
    // ignore — private mode, disabled storage, etc.
  }
  return null;
}

function useResolvedDensity(
  listId: string | undefined,
  fallback: "full" | "compact",
): "full" | "compact" {
  const [resolved, setResolved] = useState<"full" | "compact">(() => {
    const persisted = readPersistedDensity(listId);
    return persisted ?? fallback;
  });

  useEffect(() => {
    const persisted = readPersistedDensity(listId);
    setResolved(persisted ?? fallback);
  }, [listId, fallback]);

  return resolved;
}
