// frontend/web/src/features/agent-runs/use-trace-scope.test.ts
import { describe, expect, test } from "vitest";
import { renderHook } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { MemoryRouter } from "react-router-dom";
import { scopeForPath, useCurrentTraceScope } from "./use-trace-scope";

function routerWrapper(initial: string) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(MemoryRouter, { initialEntries: [initial] }, children);
  };
}

describe("scopeForPath", () => {
  test("live surfaces map to the live scope", () => {
    expect(scopeForPath("/live")).toBe("live");
    expect(scopeForPath("/live/runs/01ABC")).toBe("live");
    expect(scopeForPath("/live/01ABC")).toBe("live");
  });

  test("eval surfaces map to the eval scope", () => {
    expect(scopeForPath("/eval-runs")).toBe("eval");
    expect(scopeForPath("/eval-runs/01ABC")).toBe("eval");
  });

  test("optimizer surfaces map to the opti scope (WS-11a)", () => {
    expect(scopeForPath("/optimizer")).toBe("opti");
    expect(scopeForPath("/optimizer?session=01ABC")).toBe("opti");
    expect(scopeForPath("/optimizer/experiment/abc")).toBe("opti");
  });

  test("the standalone agent-run route maps to the eval scope", () => {
    expect(scopeForPath("/agent-runs/01ABC")).toBe("eval");
  });

  test("unrelated routes default to the eval scope", () => {
    expect(scopeForPath("/")).toBe("eval");
    expect(scopeForPath("/strategies")).toBe("eval");
    // Guard the prefix-match boundary: a route that merely contains
    // "live" mid-path is NOT the live surface.
    expect(scopeForPath("/strategies/live-trading")).toBe("eval");
  });
});

describe("useCurrentTraceScope", () => {
  test("derives the scope from the current router location", () => {
    expect(
      renderHook(() => useCurrentTraceScope(), {
        wrapper: routerWrapper("/live/runs/01ABC"),
      }).result.current,
    ).toBe("live");
    expect(
      renderHook(() => useCurrentTraceScope(), {
        wrapper: routerWrapper("/eval-runs/01ABC"),
      }).result.current,
    ).toBe("eval");
    expect(
      renderHook(() => useCurrentTraceScope(), {
        wrapper: routerWrapper("/agent-runs/01ABC"),
      }).result.current,
    ).toBe("eval");
  });
});
