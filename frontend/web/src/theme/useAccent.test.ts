import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { ACCENT_PREFERENCE_KEY } from "./themes";

// Must be imported AFTER any localStorage setup so the module singleton
// is initialised with the right value.  We use vi.resetModules() in
// beforeEach to guarantee a fresh singleton each test.
async function importHook() {
  const mod = await import("./useAccent");
  return mod.useAccent;
}

beforeEach(() => {
  vi.resetModules();
  localStorage.clear();
});

afterEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

describe("useAccent persistence round-trip", () => {
  it("defaults to green when nothing is stored", async () => {
    const useAccent = await importHook();
    const { result } = renderHook(() => useAccent());
    expect(result.current.accentKey).toBe("green");
  });

  it("reads the stored value on first mount (simulated reload)", async () => {
    // Pre-seed storage as if the user had saved 'azure' in a previous session.
    localStorage.setItem(ACCENT_PREFERENCE_KEY, "azure");

    // Fresh module import → singleton is initialised from storage.
    const useAccent = await importHook();
    const { result } = renderHook(() => useAccent());

    expect(result.current.accentKey).toBe("azure");
  });

  it("writes the selected accent to localStorage so it survives a reload", async () => {
    const useAccent = await importHook();
    const { result } = renderHook(() => useAccent());

    act(() => {
      result.current.setAccent("magenta");
    });

    expect(localStorage.getItem(ACCENT_PREFERENCE_KEY)).toBe("magenta");
  });

  it("round-trips: value written in one mount is read back on the next mount", async () => {
    // First mount — user picks amber.
    const useAccent1 = await importHook();
    const { result: r1 } = renderHook(() => useAccent1());
    act(() => {
      r1.current.setAccent("amber");
    });
    expect(localStorage.getItem(ACCENT_PREFERENCE_KEY)).toBe("amber");

    // Simulate reload: reset modules so the singleton is re-initialised.
    vi.resetModules();
    const useAccent2 = await importHook();
    const { result: r2 } = renderHook(() => useAccent2());

    expect(r2.current.accentKey).toBe("amber");
  });

  it("reflects the new accent immediately in the same mount after setAccent", async () => {
    const useAccent = await importHook();
    const { result } = renderHook(() => useAccent());

    expect(result.current.accentKey).toBe("green");

    act(() => {
      result.current.setAccent("teal");
    });

    expect(result.current.accentKey).toBe("teal");
  });
});
