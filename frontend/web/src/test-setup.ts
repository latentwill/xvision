import "@testing-library/jest-dom/vitest";

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
