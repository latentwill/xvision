import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Suspense, lazy, useEffect } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import { router } from "./routes";

import { AppErrorBoundary } from "@/components/AppErrorBoundary";
import {
  __INTERNAL,
  noteSuccessfulPageLoad,
} from "@/lib/chunk-reload";

// Mirror of the `RouteLoaded` marker in `routes.tsx`. Re-declared here
// so the test pins the contract directly (marker mounts only after
// Suspense resolves → only then is the reload-attempted flag cleared).
// Effect-based, matching production; if the production marker moves,
// this stays a passing pin of the *behavior* (only fires post-resolve).
function RouteLoaded() {
  useEffect(() => {
    noteSuccessfulPageLoad();
  }, []);
  return null;
}

class MemorySessionStorage {
  private data = new Map<string, string>();
  get length() {
    return this.data.size;
  }
  clear() {
    this.data.clear();
  }
  getItem(key: string) {
    return this.data.has(key) ? this.data.get(key)! : null;
  }
  setItem(key: string, value: string) {
    this.data.set(key, String(value));
  }
  removeItem(key: string) {
    this.data.delete(key);
  }
  key(index: number) {
    return Array.from(this.data.keys())[index] ?? null;
  }
}

let reloadSpy: ReturnType<typeof vi.fn>;
let consoleErrorSpy: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  const storage = new MemorySessionStorage();
  Object.defineProperty(window, "sessionStorage", {
    value: storage,
    writable: true,
    configurable: true,
  });

  reloadSpy = vi.fn();
  Object.defineProperty(window, "location", {
    value: { ...window.location, reload: reloadSpy },
    writable: true,
    configurable: true,
  });

  consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  consoleErrorSpy.mockRestore();
  vi.restoreAllMocks();
});

describe("router route topology", () => {
  it("exposes /optimizer and keeps the legacy /autooptimizer path registered", () => {
    const serialized = JSON.stringify(router.routes);
    expect(serialized).toContain("optimizer");
    expect(serialized).toContain("autooptimizer");
  });
});

describe("RouteLoaded clears reload-attempted flag only after Suspense resolves", () => {
  it("does NOT clear the flag while the lazy chunk is still loading", async () => {
    // Pre-condition: previous deploy triggered a reload; flag persists
    // across the page lifecycle and we're now booting the new bundle.
    window.sessionStorage.setItem(__INTERNAL.RELOAD_FLAG, "1");

    // Pending lazy that never resolves — simulates the chunk still
    // being fetched (or hung). The marker must NOT run during this
    // window, or a second chunk-load failure would trigger a fresh
    // reload (PR #317 review — P1 loop scenario).
    const PendingLazy = lazy(() => new Promise<{ default: () => JSX.Element }>(() => {}));

    render(
      <Suspense fallback={<div data-testid="loading">Loading…</div>}>
        <RouteLoaded />
        <PendingLazy />
      </Suspense>,
    );

    await waitFor(() => {
      expect(screen.getByTestId("loading")).toBeInTheDocument();
    });
    expect(window.sessionStorage.getItem(__INTERNAL.RELOAD_FLAG)).toBe("1");
  });

  it("clears the flag once the lazy chunk resolves and Suspense unblocks", async () => {
    window.sessionStorage.setItem(__INTERNAL.RELOAD_FLAG, "1");

    const Page = () => <div data-testid="page">Loaded</div>;
    const ResolvedLazy = lazy(async () => ({ default: Page }));

    render(
      <Suspense fallback={<div data-testid="loading">Loading…</div>}>
        <RouteLoaded />
        <ResolvedLazy />
      </Suspense>,
    );

    await waitFor(() => {
      expect(screen.getByTestId("page")).toBeInTheDocument();
    });
    expect(window.sessionStorage.getItem(__INTERNAL.RELOAD_FLAG)).toBeNull();
  });

  it("when the chunk fails the boundary catches it BEFORE the marker clears the flag", async () => {
    // Post-reload state: flag is "1". If the new bundle's chunk also
    // fails, AppErrorBoundary must see the flag still set and render
    // the manual-refresh hint instead of looping.
    window.sessionStorage.setItem(__INTERNAL.RELOAD_FLAG, "1");

    const FailingLazy = lazy(async () => {
      throw new TypeError(
        "Failed to fetch dynamically imported module: /assets/x.js",
      );
    });

    render(
      <AppErrorBoundary>
        <Suspense fallback={<div data-testid="loading">Loading…</div>}>
          <RouteLoaded />
          <FailingLazy />
        </Suspense>
      </AppErrorBoundary>,
    );

    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(
        /Reload didn.t recover/i,
      );
    });
    expect(reloadSpy).not.toHaveBeenCalled();
    // Flag preserved — second reload not attempted.
    expect(window.sessionStorage.getItem(__INTERNAL.RELOAD_FLAG)).toBe("1");
  });
});
