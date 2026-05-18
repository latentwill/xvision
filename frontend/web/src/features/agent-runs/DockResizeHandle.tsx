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
  const dragStateRef = useRef<{ startY: number; startHeight: number } | null>(
    null,
  );

  // Drag tracking: the handle sits on the dock's TOP edge, so as the
  // pointer moves UP (lower clientY) the dock grows.
  useEffect(() => {
    function onMove(e: PointerEvent) {
      const drag = dragStateRef.current;
      if (!drag) return;
      const delta = drag.startY - e.clientY;
      setHeightPx(drag.startHeight + delta);
    }
    function onUp() {
      if (!dragStateRef.current) return;
      dragStateRef.current = null;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
  }, [setHeightPx]);

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) return;
      e.preventDefault();
      dragStateRef.current = {
        startY: e.clientY,
        startHeight: useTraceDock.getState().heightPx,
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
