import { afterEach, describe, expect, it } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { MARKETPLACE_OPTIN_KEY, useMarketplaceOptIn } from "./optin";

afterEach(() => {
  localStorage.clear();
  // Reset the module-level snapshot by toggling through the public API.
  // (renderHook below re-reads localStorage on each mount.)
});

describe("useMarketplaceOptIn", () => {
  it("defaults to off when nothing is stored", () => {
    const { result } = renderHook(() => useMarketplaceOptIn());
    expect(result.current.enabled).toBe(false);
  });

  it("reads an existing stored opt-in as on", () => {
    localStorage.setItem(MARKETPLACE_OPTIN_KEY, "1");
    const { result } = renderHook(() => useMarketplaceOptIn());
    expect(result.current.enabled).toBe(true);
  });

  it("persists enabling to localStorage and flips the snapshot", () => {
    const { result } = renderHook(() => useMarketplaceOptIn());
    act(() => result.current.setEnabled(true));
    expect(result.current.enabled).toBe(true);
    expect(localStorage.getItem(MARKETPLACE_OPTIN_KEY)).toBe("1");
  });

  it("removes the key when disabled", () => {
    localStorage.setItem(MARKETPLACE_OPTIN_KEY, "1");
    const { result } = renderHook(() => useMarketplaceOptIn());
    act(() => result.current.setEnabled(false));
    expect(result.current.enabled).toBe(false);
    expect(localStorage.getItem(MARKETPLACE_OPTIN_KEY)).toBeNull();
  });

  it("propagates a change to a second subscriber", () => {
    const a = renderHook(() => useMarketplaceOptIn());
    const b = renderHook(() => useMarketplaceOptIn());
    act(() => a.result.current.setEnabled(true));
    expect(b.result.current.enabled).toBe(true);
  });
});
