import { useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent } from "react";

import { Icon } from "@/components/primitives/Icon";

import {
  isFilterActive,
  type ActiveFilter,
  type ListSortState,
} from "./useListState";

export type MListSheetProps = {
  open: boolean;
  focus?: "filters" | "sort";
  onClose: () => void;
  filters?: ActiveFilter[];
  sort?: ListSortState;
  resultCount?: number;
  clearAll?: () => void;
};

const SWIPE_DISMISS_PX = 120;
const SWIPE_DISMISS_VELOCITY = 0.5;
const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function MListSheet({
  open,
  focus = "filters",
  onClose,
  filters = [],
  sort,
  resultCount,
  clearAll,
}: MListSheetProps) {
  const sheetRef = useRef<HTMLDivElement | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  const [dragOffset, setDragOffset] = useState(0);
  const dragStateRef = useRef<{
    startY: number;
    startTime: number;
    pointerId: number;
  } | null>(null);

  useEffect(() => {
    if (!open) return;

    previousFocusRef.current = (document.activeElement as HTMLElement) ?? null;

    const originalOverflow = document.body.style.overflow;
    const originalPadding = document.body.style.paddingRight;
    const scrollbarWidth =
      window.innerWidth - document.documentElement.clientWidth;
    document.body.style.overflow = "hidden";
    if (scrollbarWidth > 0) {
      document.body.style.paddingRight = `${scrollbarWidth}px`;
    }

    const first = sheetRef.current?.querySelector<HTMLElement>(FOCUSABLE_SELECTOR);
    first?.focus({ preventScroll: true });

    return () => {
      document.body.style.overflow = originalOverflow;
      document.body.style.paddingRight = originalPadding;
      previousFocusRef.current?.focus?.({ preventScroll: true });
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
        return;
      }
      if (e.key !== "Tab") return;
      const sheet = sheetRef.current;
      if (!sheet) return;
      const focusables = Array.from(
        sheet.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
      ).filter((el) => !el.hasAttribute("hidden"));
      if (focusables.length === 0) {
        e.preventDefault();
        return;
      }
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement as HTMLElement | null;
      if (e.shiftKey && active === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  const onPointerDown = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      const target = e.target as HTMLElement;
      // Only initiate swipe from grip handle or sheet head, not from buttons or scrollable body.
      if (!target.closest("[data-sheet-drag]")) return;
      dragStateRef.current = {
        startY: e.clientY,
        startTime: performance.now(),
        pointerId: e.pointerId,
      };
      (e.currentTarget as HTMLDivElement).setPointerCapture(e.pointerId);
    },
    [],
  );

  const onPointerMove = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      const drag = dragStateRef.current;
      if (!drag || drag.pointerId !== e.pointerId) return;
      const delta = e.clientY - drag.startY;
      setDragOffset(Math.max(0, delta));
    },
    [],
  );

  const onPointerUp = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      const drag = dragStateRef.current;
      if (!drag || drag.pointerId !== e.pointerId) return;
      const delta = e.clientY - drag.startY;
      const elapsed = Math.max(1, performance.now() - drag.startTime);
      const velocity = delta / elapsed;
      dragStateRef.current = null;
      setDragOffset(0);
      if (delta > SWIPE_DISMISS_PX || velocity > SWIPE_DISMISS_VELOCITY) {
        onClose();
      }
    },
    [onClose],
  );

  const onPointerCancel = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      if (dragStateRef.current?.pointerId !== e.pointerId) return;
      dragStateRef.current = null;
      setDragOffset(0);
    },
    [],
  );

  if (!open) return null;

  const focusSort = focus === "sort";
  const sortOptions = sort?.options ?? [];
  const sheetStyle: CSSProperties = dragOffset
    ? { transform: `translateY(${dragOffset}px)` }
    : {};

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={focusSort ? "Sort by" : "Filter and sort"}
      onClick={onClose}
      className="fixed inset-0 z-[100] flex items-end bg-black/55 backdrop-blur-[2px] motion-safe:animate-[fade-in_120ms_ease-out]"
    >
      <div
        ref={sheetRef}
        onClick={(e) => e.stopPropagation()}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerCancel}
        style={sheetStyle}
        className="w-full max-h-[88%] bg-surface-card border-t border-border-strong rounded-t-[18px] flex flex-col shadow-[0_-20px_60px_rgba(0,0,0,0.6)] motion-safe:animate-[slide-up_220ms_cubic-bezier(.2,.7,.3,1)]"
      >
        <div data-sheet-drag className="cursor-grab active:cursor-grabbing">
          <div
            className="mx-auto w-9 h-1 rounded-sm bg-border-strong mt-2.5 mb-1.5"
            aria-hidden
          />
          <div className="flex items-center justify-between px-[18px] pt-1 pb-2.5">
            <h3 className="m-0 font-sans font-semibold text-[22px] tracking-tight text-text">
              {focusSort ? "Sort by" : "Filter & sort"}
            </h3>
            {clearAll && (
              <button
                type="button"
                onClick={clearAll}
                className="border-none bg-transparent cursor-pointer text-text-3 font-mono text-[11px] tracking-[0.14em] uppercase"
              >
                Clear all
              </button>
            )}
          </div>
        </div>
        <div className="flex-1 min-h-0 overflow-y-auto px-[18px] pt-1 pb-3.5 flex flex-col gap-4.5">
          {!focusSort &&
            filters.map((f) => (
              <div key={f.def.id} className="flex flex-col gap-2">
                <div className="inline-flex items-center gap-1.5 font-mono text-[10.5px] tracking-[0.14em] uppercase text-text-3">
                  {f.def.label}
                </div>
                <div className="flex gap-1.5 flex-wrap">
                  {f.def.options.map((o) => {
                    const on = o.value === f.value;
                    return (
                      <button
                        key={o.value}
                        type="button"
                        data-on={on || undefined}
                        onClick={() => f.setValue(o.value)}
                        className="inline-flex items-center gap-1.5 h-8 px-3 bg-surface-elev border border-border rounded-full text-text-2 font-sans text-[12.5px] cursor-pointer data-[on]:bg-gold/10 data-[on]:border-gold/50 data-[on]:text-gold"
                      >
                        {on && (
                          <span className="text-[10px] text-gold -mr-0.5">
                            ✓
                          </span>
                        )}
                        {o.label}
                      </button>
                    );
                  })}
                </div>
              </div>
            ))}
          <div className="flex flex-col gap-2">
            <div className="inline-flex items-center gap-1.5 font-mono text-[10.5px] tracking-[0.14em] uppercase text-text-3">
              <Icon name="sliders" size={11} className="text-gold" /> Sort by
            </div>
            <div className="flex flex-col bg-surface-elev border border-border rounded-lg overflow-hidden">
              {sortOptions.map((o) => {
                const on = o.value === (sort?.value ?? sortOptions[0]?.value);
                return (
                  <button
                    key={o.value}
                    type="button"
                    data-on={on || undefined}
                    onClick={() => sort?.setValue(o.value)}
                    className="flex items-center gap-3 px-3.5 py-3 bg-transparent border-0 border-b border-border-soft last:border-b-0 text-text-2 font-sans text-[14px] text-left cursor-pointer data-[on]:text-gold data-[on]:bg-gold/[0.06]"
                  >
                    <span
                      className={`text-base w-3.5 ${on ? "text-gold" : "text-text-4"}`}
                    >
                      {on ? "●" : "○"}
                    </span>
                    <span>{o.label}</span>
                  </button>
                );
              })}
            </div>
          </div>
        </div>
        <div className="px-[18px] py-4.5 border-t border-border-soft">
          <button
            type="button"
            onClick={onClose}
            className="w-full h-[46px] inline-flex items-center justify-center rounded-full bg-gold border border-gold text-bg font-medium text-[14px] cursor-pointer active:scale-[0.96]"
          >
            Show {resultCount ?? 0}{" "}
            {resultCount === 1 ? "result" : "results"}
          </button>
        </div>
      </div>
    </div>
  );
}

export function activeFilterCount(filters: ActiveFilter[]): number {
  return filters.filter(isFilterActive).length;
}
