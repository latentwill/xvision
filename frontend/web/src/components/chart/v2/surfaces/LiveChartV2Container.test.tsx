/**
 * Behavior parity tests for LiveChartV2Container (Task 9).
 *
 * The container owns the proven `useRunStream(runId)` SSE hook, adapts each
 * streamed RunChartPayload via `runChartPayloadToV2`, and renders LiveChartV2
 * with follow/freeze/resume controls — reproducing v1 `LiveChart`.
 *
 * We mock `@/components/chart/use-run-stream` to a controllable
 * `{ data, status }` and stub `./LiveChartV2` to a lightweight testid div that
 * exposes `payload.connection` + `follow` via data attributes. This keeps the
 * test off klinecharts/uPlot and off the real EventSource.
 */
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";

import type { RunChartPayload } from "@/api/types.gen";
import type { LiveStatus } from "@/components/chart/use-run-stream";

const useRunStreamMock = vi.fn();
vi.mock("@/components/chart/use-run-stream", () => ({
  useRunStream: (...args: unknown[]) => useRunStreamMock(...args),
}));

vi.mock("./LiveChartV2", () => ({
  LiveChartV2: ({
    payload,
    follow,
  }: {
    payload: { connection: string };
    follow?: boolean;
  }) => (
    <div
      data-testid="live-chart-v2"
      data-connection={payload.connection}
      data-follow={String(!!follow)}
    />
  ),
}));

import { LiveChartV2Container } from "./LiveChartV2Container";

/** Minimal RunChartPayload: one bar, empty indicator arrays, empty
 * equity/drawdown/position, empty markers. Matches the ts-rs shapes in
 * src/api/types.gen/*. */
function runPayload(runId = "run-1"): RunChartPayload {
  const emptyIndicators = {
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
  };
  return {
    run_id: runId,
    scenario_id: "scn-1",
    asset: "BTC",
    granularity: "1h",
    time_window: { start: "2024-01-01T00:00:00Z", end: "2024-01-01T01:00:00Z" },
    bars: [
      { time: 1_700_000_000, open: 1, high: 2, low: 0.5, close: 1.5, volume: 10 },
    ],
    indicators: emptyIndicators,
    equity: [],
    drawdown: [],
    position: [],
    markers: { trades: [], vetoes: [], holds: [] },
  };
}

function setStream(data: RunChartPayload | undefined, status: LiveStatus) {
  useRunStreamMock.mockReturnValue({ data, status });
}

describe("LiveChartV2Container", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("maps status 'streaming' to connection 'connected'", () => {
    setStream(runPayload(), "streaming");
    render(<LiveChartV2Container runId="run-1" />);
    expect(screen.getByTestId("live-chart-v2")).toHaveAttribute(
      "data-connection",
      "connected",
    );
  });

  it("maps status 'closed' to connection 'offline'", () => {
    setStream(runPayload(), "closed");
    render(<LiveChartV2Container runId="run-1" />);
    expect(screen.getByTestId("live-chart-v2")).toHaveAttribute(
      "data-connection",
      "offline",
    );
  });

  it("maps status 'snapshot' to connection 'reconnecting'", () => {
    setStream(runPayload(), "snapshot");
    render(<LiveChartV2Container runId="run-1" />);
    expect(screen.getByTestId("live-chart-v2")).toHaveAttribute(
      "data-connection",
      "reconnecting",
    );
  });

  it("maps status 'reconnecting' to connection 'reconnecting'", () => {
    setStream(runPayload(), "reconnecting");
    render(<LiveChartV2Container runId="run-1" />);
    expect(screen.getByTestId("live-chart-v2")).toHaveAttribute(
      "data-connection",
      "reconnecting",
    );
  });

  it("defaults to following live, and toggling freezes then resumes", () => {
    setStream(runPayload(), "streaming");
    render(<LiveChartV2Container runId="run-1" />);

    // Default: following live.
    expect(screen.getByText("Following live")).toBeInTheDocument();
    expect(screen.getByTestId("live-chart-v2")).toHaveAttribute(
      "data-follow",
      "true",
    );

    // Uncheck → frozen + resume affordance.
    fireEvent.click(screen.getByRole("checkbox"));
    expect(screen.getByText("Frozen")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Resume live" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("live-chart-v2")).toHaveAttribute(
      "data-follow",
      "false",
    );

    // Resume → following again.
    fireEvent.click(screen.getByRole("button", { name: "Resume live" }));
    expect(screen.getByText("Following live")).toBeInTheDocument();
    expect(screen.getByTestId("live-chart-v2")).toHaveAttribute(
      "data-follow",
      "true",
    );
  });

  it("shows the waiting placeholder when there is no data yet", () => {
    setStream(undefined, "snapshot");
    render(<LiveChartV2Container runId="run-1" />);
    expect(screen.getByText("Waiting for first event…")).toBeInTheDocument();
    expect(screen.queryByTestId("live-chart-v2")).not.toBeInTheDocument();
  });
});
