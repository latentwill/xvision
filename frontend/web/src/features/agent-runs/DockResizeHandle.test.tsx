import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import {
  DOCK_HEIGHT_STORAGE_KEY,
  DOCK_MIN_PX,
  DEFAULT_DOCK_PX,
  dockMaxPx,
  useTraceDock,
} from "@/stores/trace-dock";
import { DockResizeHandle } from "./DockResizeHandle";

function resetStore() {
  useTraceDock.setState({ heightPx: DEFAULT_DOCK_PX });
}

beforeEach(() => {
  localStorage.clear();
  resetStore();
});

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("DockResizeHandle", () => {
  test("renders a focusable separator with aria values", () => {
    render(<DockResizeHandle />);
    const handle = screen.getByTestId("trace-dock-resize-handle");
    expect(handle).toHaveAttribute("role", "separator");
    expect(handle).toHaveAttribute("aria-orientation", "horizontal");
    expect(handle).toHaveAttribute("aria-valuenow", String(DEFAULT_DOCK_PX));
    expect(handle).toHaveAttribute("aria-valuemin", String(DOCK_MIN_PX));
    expect(handle.tabIndex).toBe(0);
  });

  test("pointer drag updates the store heightPx", () => {
    render(<DockResizeHandle />);
    const handle = screen.getByTestId("trace-dock-resize-handle");
    // The handle sits on the dock's TOP edge — moving the pointer UP
    // (smaller clientY) GROWS the dock.
    fireEvent.pointerDown(handle, { button: 0, clientY: 500 });
    fireEvent.pointerMove(window, { clientY: 380 });
    fireEvent.pointerUp(window);
    expect(useTraceDock.getState().heightPx).toBe(DEFAULT_DOCK_PX + 120);
  });

  test("pointer drag clamps to the dock min", () => {
    render(<DockResizeHandle />);
    const handle = screen.getByTestId("trace-dock-resize-handle");
    fireEvent.pointerDown(handle, { button: 0, clientY: 100 });
    fireEvent.pointerMove(window, { clientY: 100_000 }); // drag way down
    fireEvent.pointerUp(window);
    expect(useTraceDock.getState().heightPx).toBe(DOCK_MIN_PX);
  });

  test("ArrowUp / ArrowDown nudge by 24px", () => {
    render(<DockResizeHandle />);
    const handle = screen.getByTestId("trace-dock-resize-handle");
    fireEvent.keyDown(handle, { key: "ArrowUp" });
    expect(useTraceDock.getState().heightPx).toBe(DEFAULT_DOCK_PX + 24);
    fireEvent.keyDown(handle, { key: "ArrowDown" });
    fireEvent.keyDown(handle, { key: "ArrowDown" });
    expect(useTraceDock.getState().heightPx).toBe(DEFAULT_DOCK_PX - 24);
  });

  test("Home / End jump to min / max", () => {
    render(<DockResizeHandle />);
    const handle = screen.getByTestId("trace-dock-resize-handle");
    fireEvent.keyDown(handle, { key: "Home" });
    expect(useTraceDock.getState().heightPx).toBe(DOCK_MIN_PX);
    fireEvent.keyDown(handle, { key: "End" });
    expect(useTraceDock.getState().heightPx).toBe(dockMaxPx());
  });

  test("persists the new height to localStorage", () => {
    render(<DockResizeHandle />);
    const handle = screen.getByTestId("trace-dock-resize-handle");
    fireEvent.keyDown(handle, { key: "ArrowUp" });
    expect(localStorage.getItem(DOCK_HEIGHT_STORAGE_KEY)).toBe(
      String(DEFAULT_DOCK_PX + 24),
    );
  });

  test("persists across mount / unmount via localStorage", async () => {
    localStorage.setItem(DOCK_HEIGHT_STORAGE_KEY, "612");
    // Re-import the store module so it re-reads localStorage on first
    // evaluation. The store's heightPx is initialized from
    // `readPersistedHeightPx` at module load, so reset + re-import
    // simulates a fresh page reload with a persisted value.
    vi.resetModules();
    const fresh = await import("@/stores/trace-dock");
    expect(fresh.useTraceDock.getState().heightPx).toBe(612);
  });
});
