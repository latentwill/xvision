import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  __INTERNAL,
  attemptChunkReload,
  consumePostReloadNotice,
  isChunkLoadError,
  noteSuccessfulPageLoad,
} from "./chunk-reload";

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

beforeEach(() => {
  const storage = new MemorySessionStorage();
  Object.defineProperty(window, "sessionStorage", {
    value: storage,
    writable: true,
    configurable: true,
  });

  reloadSpy = vi.fn();
  // jsdom defines `location` as a getter on `Window.prototype` that
  // resists straight reassignment; replacing the whole object lets us
  // observe reload() calls without navigating jsdom.
  Object.defineProperty(window, "location", {
    value: { ...window.location, reload: reloadSpy },
    writable: true,
    configurable: true,
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("isChunkLoadError", () => {
  it("matches the canonical Vite TypeError message", () => {
    const err = new TypeError(
      "Failed to fetch dynamically imported module: https://xvn.tail2bb69.ts.net/assets/scenarios-new-5cnT8cD7.js",
    );
    expect(isChunkLoadError(err)).toBe(true);
  });

  it("matches the substring shape on a plain Error", () => {
    const err = new Error(
      "fetch failure — Failed to fetch dynamically imported module foo",
    );
    expect(isChunkLoadError(err)).toBe(true);
  });

  it("matches errors with name === ChunkLoadError", () => {
    const err = new Error("some bundler-flavored failure");
    err.name = "ChunkLoadError";
    expect(isChunkLoadError(err)).toBe(true);
  });

  it("matches alternative Vite message variants", () => {
    expect(
      isChunkLoadError(new Error("error loading dynamically imported module x")),
    ).toBe(true);
    expect(
      isChunkLoadError(new Error("Importing a module script failed")),
    ).toBe(true);
  });

  it("matches string-shaped errors with the canonical substring", () => {
    expect(
      isChunkLoadError(
        "TypeError: Failed to fetch dynamically imported module xyz",
      ),
    ).toBe(true);
  });

  it("returns false for unrelated errors", () => {
    expect(isChunkLoadError(new Error("network 500 from /api/strategies"))).toBe(
      false,
    );
    expect(isChunkLoadError(new TypeError("undefined is not a function"))).toBe(
      false,
    );
    expect(isChunkLoadError(null)).toBe(false);
    expect(isChunkLoadError(undefined)).toBe(false);
    expect(isChunkLoadError({ message: "nope" })).toBe(false);
  });
});

describe("attemptChunkReload", () => {
  it("triggers window.location.reload once for a chunk error", () => {
    const err = new TypeError(
      "Failed to fetch dynamically imported module: /assets/x.js",
    );
    const triggered = attemptChunkReload(err);
    expect(triggered).toBe(true);
    expect(reloadSpy).toHaveBeenCalledTimes(1);
    expect(window.sessionStorage.getItem(__INTERNAL.RELOAD_FLAG)).toBe("1");
    expect(window.sessionStorage.getItem(__INTERNAL.NOTICE_FLAG)).toBe("1");
  });

  it("no-ops on subsequent calls within the same session", () => {
    const err = new TypeError(
      "Failed to fetch dynamically imported module: /assets/x.js",
    );
    expect(attemptChunkReload(err)).toBe(true);
    expect(attemptChunkReload(err)).toBe(false);
    expect(attemptChunkReload(err)).toBe(false);
    expect(reloadSpy).toHaveBeenCalledTimes(1);
  });

  it("returns false (and does not reload) for non-chunk errors", () => {
    const err = new Error("server 500");
    expect(attemptChunkReload(err)).toBe(false);
    expect(reloadSpy).not.toHaveBeenCalled();
    expect(window.sessionStorage.getItem(__INTERNAL.RELOAD_FLAG)).toBeNull();
  });

  it("allows another reload after noteSuccessfulPageLoad clears the flag", () => {
    const err = new TypeError(
      "Failed to fetch dynamically imported module: /assets/x.js",
    );
    expect(attemptChunkReload(err)).toBe(true);
    expect(attemptChunkReload(err)).toBe(false);

    // Simulate the page boot lifecycle completing after a reload.
    noteSuccessfulPageLoad();
    expect(window.sessionStorage.getItem(__INTERNAL.RELOAD_FLAG)).toBeNull();

    expect(attemptChunkReload(err)).toBe(true);
    expect(reloadSpy).toHaveBeenCalledTimes(2);
  });
});

describe("consumePostReloadNotice", () => {
  it("returns true once after a reload and clears the flag", () => {
    window.sessionStorage.setItem(__INTERNAL.NOTICE_FLAG, "1");
    expect(consumePostReloadNotice()).toBe(true);
    expect(consumePostReloadNotice()).toBe(false);
    expect(window.sessionStorage.getItem(__INTERNAL.NOTICE_FLAG)).toBeNull();
  });

  it("returns false when no reload notice is pending", () => {
    expect(consumePostReloadNotice()).toBe(false);
  });
});
