import { useCallback, useEffect, useRef } from "react";
import {
  DOCK_MIN_PX,
  clampDockPx,
  dockMaxPx,
  useTraceDock,
} from "@/stores/trace-dock";

const KEYBOARD_NUDGE_PX = 24;

/**
 * Vertical resize handle for the trace dock. Pointer drag updates the
 * store's `heightPx`; the dock's `height` style binds to that slice so
 * the resize is visible in real time. The persisted height survives
 * across reloads (see `useTraceDock` localStorage hydration).
 *
 * The handle is keyboard-accessible: ArrowUp / ArrowDown nudge by
 * {@link KEYBOARD_NUDGE_PX}, Home jumps to the min height, End to the
 * max. `prefers-reduced-motion` is respected — the handle uses
 * `transition: none` and never animates the dock body itself.
 */
export function DockResizeHandle() {
  const heightPx = useTraceDock((s) => s.heightPx);
  const setHeightPx = useTraceDock((s) => s.setHeightPx);
  // Drag state plus the body styles captured at pointer-down so we can
  // restore them even if the component unmounts mid-drag (route
  // navigation, active-run clear, etc.) before pointerup fires.
  const dragStateRef = useRef<{
    startY: number;
    startHeight: number;
    prevCursor: string;
    prevUserSelect: string;
  } | null>(null);

  // Single drag-end cleanup. Idempotent: noop when no drag is active so
  // both `pointerup` and the effect-cleanup can call it safely.
  const endDrag = useCallback(() => {
    const drag = dragStateRef.current;
    if (!drag) return;
    dragStateRef.current = null;
    document.body.style.cursor = drag.prevCursor;
    document.body.style.userSelect = drag.prevUserSelect;
  }, []);

  // Drag tracking: the handle sits on the dock's TOP edge, so as the
  // pointer moves UP (lower clientY) the dock grows.
  useEffect(() => {
    function onMove(e: PointerEvent) {
      const drag = dragStateRef.current;
      if (!drag) return;
      const delta = drag.startY - e.clientY;
      setHeightPx(drag.startHeight + delta);
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", endDrag);
    window.addEventListener("pointercancel", endDrag);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", endDrag);
      window.removeEventListener("pointercancel", endDrag);
      // If the handle unmounts mid-drag — route nav, active-run clear,
      // a parent state transition — restore the body styles too;
      // otherwise the page is stuck with `ns-resize` + `userSelect:none`
      // for the rest of the session.
      endDrag();
    };
  }, [setHeightPx, endDrag]);

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) return;
      e.preventDefault();
      dragStateRef.current = {
        startY: e.clientY,
        startHeight: useTraceDock.getState().heightPx,
        prevCursor: document.body.style.cursor,
        prevUserSelect: document.body.style.userSelect,
      };
      document.body.style.cursor = "ns-resize";
      document.body.style.userSelect = "none";
    },
    [],
  );

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      switch (e.key) {
        case "ArrowUp":
          e.preventDefault();
          setHeightPx(useTraceDock.getState().heightPx + KEYBOARD_NUDGE_PX);
          return;
        case "ArrowDown":
          e.preventDefault();
          setHeightPx(useTraceDock.getState().heightPx - KEYBOARD_NUDGE_PX);
          return;
        case "Home":
          e.preventDefault();
          setHeightPx(DOCK_MIN_PX);
          return;
        case "End":
          e.preventDefault();
          setHeightPx(dockMaxPx());
          return;
      }
    },
    [setHeightPx],
  );

  return (
    <div
      data-testid="trace-dock-resize-handle"
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize trace dock"
      aria-valuemin={DOCK_MIN_PX}
      aria-valuemax={dockMaxPx()}
      aria-valuenow={clampDockPx(heightPx)}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
      className="absolute -top-1 left-0 right-0 h-2 cursor-ns-resize select-none focus:outline-none focus-visible:bg-text/30 hover:bg-text/20"
    />
  );
}
