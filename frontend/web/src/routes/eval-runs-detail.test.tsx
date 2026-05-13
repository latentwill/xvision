import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { EvalRunDetailRoute } from "./eval-runs-detail";
import * as chartApi from "@/api/chart";
import * as evalApi from "@/api/eval";
import type { DecisionRowDto, RunDetail } from "@/api/types.gen";

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>(
    "@/api/eval",
  );
  return {
    ...actual,
    getRun: vi.fn(),
  };
});

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

  removeEventListener(name: string, cb: (ev: MessageEvent) => void) {
    this.listeners.get(name)?.delete(cb);
  }

  close() {
    this.closed = true;
  }

  emit(name: string, payload: unknown) {
    const ev = { data: JSON.stringify(payload) } as MessageEvent;
    this.listeners.get(name)?.forEach((cb) => cb(ev));
  }
}

function renderDetail() {
  return render(
    <MemoryRouter initialEntries={["/eval-runs/01LIVE"]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <Routes>
          <Route path="/eval-runs/:runId" element={<EvalRunDetailRoute />} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

function decision(overrides: Partial<DecisionRowDto> = {}): DecisionRowDto {
  return {
    decision_index: 0,
    timestamp: "2026-05-13T15:00:00Z",
    asset: "BTC/USD",
    action: "long_open",
    conviction: 0.77,
    justification: "breakout confirmed",
    order_size: 0.1,
    fill_price: 69000,
    fill_size: 0.1,
    fee: 0.25,
    pnl_realized: null,
    ...overrides,
  };
}

function detail(overrides: Partial<RunDetail> = {}): RunDetail {
  return {
    summary: {
      id: "01LIVE",
      agent_id: "01AGENT",
      scenario_id: "btc-4h",
      mode: "backtest",
      status: "running",
      started_at: "2026-05-13T14:00:00Z",
      completed_at: null,
      sharpe: null,
      max_drawdown_pct: null,
      total_return_pct: null,
      error: null,
    },
    decisions: [],
    equity_curve: [],
    ...overrides,
  };
}

describe("EvalRunDetailRoute", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    FakeEventSource.instances = [];
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.mocked(chartApi.getRunChart).mockResolvedValue(null as never);
    vi.mocked(chartApi.openRunStream).mockImplementation(
      (runId: string) => new EventSource(`/stream/${runId}`),
    );
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("appends streamed decisions while a run is active", async () => {
    vi.mocked(evalApi.getRun).mockResolvedValue(detail());

    renderDetail();

    await screen.findByText("no decisions");
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));

    FakeEventSource.instances[0].emit("decision", {
      event: "decision",
      data: decision(),
    });

    expect(await screen.findByText("long_open")).toBeInTheDocument();
    expect(screen.getByText("BTC/USD")).toBeInTheDocument();
    expect(screen.getByText("0.77")).toBeInTheDocument();
  });
});
