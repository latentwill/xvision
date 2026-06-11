import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, waitFor, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";

import * as chartApi from "@/api/chart";
import type { RunChartPayload } from "@/api/types.gen";
import { useRunStream } from "./use-run-stream";

vi.mock("@/api/chart", () => ({
  chartKeys: {
    run: (id: string) => ["chart", "run", id],
  },
  getRunChart: vi.fn(),
  openRunStream: vi.fn((runId: string) => new EventSource(`/stream/${runId}`)),
}));

class FakeEventSource {
  static instances: FakeEventSource[] = [];
  listeners = new Map<string, Set<(ev: MessageEvent) => void>>();
  closed = false;

  constructor(public url: string) {
    FakeEventSource.instances.push(this);
  }

  addEventListener(name: string, cb: (ev: MessageEvent) => void) {
    const listeners = this.listeners.get(name) ?? new Set();
    listeners.add(cb);
    this.listeners.set(name, listeners);
  }

  close() {
    this.closed = true;
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

function wrapper({ children }: { children: ReactNode }) {
  return (
    <QueryClientProvider
      client={
        new QueryClient({
          defaultOptions: { queries: { retry: false } },
        })
      }
    >
      {children}
    </QueryClientProvider>
  );
}

function runPayload(runId: string): RunChartPayload {
  return {
    run_id: runId,
    scenario_id: "scn-1",
    asset: "BTC",
    granularity: "1h",
    time_window: { start: "2024-01-01T00:00:00Z", end: "2024-01-01T01:00:00Z" },
    bars: [
      { time: 1_700_000_000, open: 1, high: 2, low: 0.5, close: 1.5, volume: 10 },
    ],
    indicators: {
      sma_20: [],
      sma_30: [],
      sma_50: [],
      sma_60: [],
      sma_90: [],
      sma_200: [],
      ema_20: [],
      ema_30: [],
      ema_50: [],
      ema_60: [],
      ema_90: [],
      ema_200: [],
      bollinger: { upper: [], middle: [], lower: [] },
      donchian: { upper: [], lower: [] },
      rsi_14: [],
      macd: { line: [], signal: [], histogram: [] },
      atr_14: [],
    },
    equity: [],
    baseline_equity: null,
    drawdown: [],
    position: [],
    markers: { trades: [], vetoes: [], holds: [] },
  };
}

function Probe({
  runId,
  seen,
}: {
  runId: string;
  seen: Array<string | undefined>;
}) {
  const { data } = useRunStream(runId);
  seen.push(data?.run_id);
  return null;
}

describe("useRunStream", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    FakeEventSource.instances = [];
    vi.stubGlobal("EventSource", FakeEventSource);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("does not expose a previous run payload during a runId change render", async () => {
    const snapshots = {
      "run-1": deferred<RunChartPayload>(),
      "run-2": deferred<RunChartPayload>(),
    };
    vi.mocked(chartApi.getRunChart).mockImplementation(
      (runId: string) => snapshots[runId as keyof typeof snapshots].promise,
    );

    const seen: Array<string | undefined> = [];
    const view = render(<Probe runId="run-1" seen={seen} />, { wrapper });

    snapshots["run-1"].resolve(runPayload("run-1"));
    await waitFor(() => expect(seen.at(-1)).toBe("run-1"));

    seen.length = 0;
    view.rerender(<Probe runId="run-2" seen={seen} />);

    expect(seen.at(-1)).toBeUndefined();
    expect(seen).not.toContain("run-1");

    snapshots["run-2"].resolve(runPayload("run-2"));
    await waitFor(() => expect(seen.at(-1)).toBe("run-2"));
  });
});
