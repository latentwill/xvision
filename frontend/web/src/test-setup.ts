import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

// Ensure DOM is cleaned between tests. @testing-library/react registers
// its own afterEach cleanup when it detects a compatible runner, but jsdom
// environments using the `pure` import path skip that. Add a guaranteed
// global cleanup to make tests independent regardless of import path.
afterEach(() => {
  cleanup();
});

// Node 22+ exposes an experimental built-in `localStorage` that lacks
// the standard Storage methods (no setItem / getItem / clear). jsdom's
// implementation is shadowed in this environment, so install a minimal
// in-memory polyfill on both `globalThis` and `window`. Tests that need
// a clean slate should call `localStorage.clear()` in beforeEach.
class MemoryStorage {
  private data = new Map<string, string>();
  get length() { return this.data.size; }
  clear() { this.data.clear(); }
  getItem(key: string) { return this.data.has(key) ? this.data.get(key)! : null; }
  setItem(key: string, value: string) { this.data.set(key, String(value)); }
  removeItem(key: string) { this.data.delete(key); }
  key(index: number) {
    return Array.from(this.data.keys())[index] ?? null;
  }
}
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const memoryStorage: any = new MemoryStorage();
Object.defineProperty(globalThis, "localStorage", {
  value: memoryStorage,
  writable: true,
  configurable: true,
});
if (typeof window !== "undefined") {
  Object.defineProperty(window, "localStorage", {
    value: memoryStorage,
    writable: true,
    configurable: true,
  });
}

// Provide a minimal EventSource stub for components that try to
// subscribe to SSE during tests. Individual tests can replace this.
class StubEventSource {
  url: string;
  readyState = 0;
  onopen: ((this: EventSource, ev: Event) => unknown) | null = null;
  onmessage: ((this: EventSource, ev: MessageEvent) => unknown) | null = null;
  onerror: ((this: EventSource, ev: Event) => unknown) | null = null;
  constructor(url: string) {
    this.url = url;
  }
  addEventListener() {}
  removeEventListener() {}
  close() {}
}
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(globalThis as any).EventSource = StubEventSource;
