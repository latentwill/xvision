import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { AppErrorBoundary } from "./AppErrorBoundary";
import { __INTERNAL } from "@/lib/chunk-reload";

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

function Boom({ error }: { error: unknown }): JSX.Element {
  throw error;
}

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

  // React logs caught errors to console.error in development. Silence
  // those during the assertion phase so test output stays readable.
  consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  consoleErrorSpy.mockRestore();
  vi.restoreAllMocks();
});

describe("AppErrorBoundary", () => {
  it("triggers a reload and renders the Updating… placeholder for chunk-load errors", () => {
    const err = new TypeError(
      "Failed to fetch dynamically imported module: /assets/scenarios-new-abc.js",
    );

    render(
      <AppErrorBoundary>
        <Boom error={err} />
      </AppErrorBoundary>,
    );

    expect(reloadSpy).toHaveBeenCalledTimes(1);
    expect(window.sessionStorage.getItem(__INTERNAL.RELOAD_FLAG)).toBe("1");
    expect(window.sessionStorage.getItem(__INTERNAL.NOTICE_FLAG)).toBe("1");
    expect(screen.getByRole("status")).toHaveTextContent(/Updating/i);
  });

  it("renders the manual-refresh hint when a reload was already attempted this session", () => {
    window.sessionStorage.setItem(__INTERNAL.RELOAD_FLAG, "1");
    const err = new TypeError(
      "Failed to fetch dynamically imported module: /assets/x.js",
    );

    render(
      <AppErrorBoundary>
        <Boom error={err} />
      </AppErrorBoundary>,
    );

    expect(reloadSpy).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent(
      /Reload didn.t recover/i,
    );
  });

  it("falls through to the existing error render for non-chunk errors", () => {
    const err = new Error("boom — totally unrelated");

    render(
      <AppErrorBoundary>
        <Boom error={err} />
      </AppErrorBoundary>,
    );

    expect(reloadSpy).not.toHaveBeenCalled();
    expect(window.sessionStorage.getItem(__INTERNAL.RELOAD_FLAG)).toBeNull();
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(/Something went wrong/i);
    // Importantly: not the chunk-specific copy.
    expect(alert).not.toHaveTextContent(/latest app bundle/i);
  });

  it("renders children when no error occurs", () => {
    render(
      <AppErrorBoundary>
        <div>healthy child</div>
      </AppErrorBoundary>,
    );
    expect(screen.getByText("healthy child")).toBeInTheDocument();
    expect(reloadSpy).not.toHaveBeenCalled();
  });
});
