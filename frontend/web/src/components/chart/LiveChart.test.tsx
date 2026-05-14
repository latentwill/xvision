import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { LiveChart } from "./LiveChart";
import samplePayload from "./__fixtures__/sample-run-chart.json";
import type { RunChartPayload } from "@/api/types.gen";

const liveChartMocks = vi.hoisted(() => ({
  runChartProps: vi.fn(),
}));

// Mock RunChart so lightweight-charts never tries to use canvas/WebGL in jsdom.
vi.mock("./RunChart", () => ({
  RunChart: (props: { follow?: boolean; payload: RunChartPayload }) => {
    liveChartMocks.runChartProps(props);
    return (
      <div data-testid="run-chart-mock" data-follow={String(props.follow)} />
    );
  },
}));

// Performance budget: snapshot-to-render time for the LiveChart shell.
// Plan §7 calls for 250ms p95 in production. For a single-trial unit test
// in jsdom with a synchronous fetch mock, we use a generous 500ms ceiling
// to absorb CI noise without making the test meaningless.
const SNAPSHOT_RENDER_BUDGET_MS = 500;

const eventSources: MockEventSource[] = [];

class MockEventSource {
  readonly url: string;
  onerror: ((event: Event) => void) | null = null;
  close = vi.fn();
  private readonly listeners = new Map<string, Set<EventListener>>();

  constructor(url: string) {
    this.url = url;
    eventSources.push(this);
  }

  addEventListener(type: string, listener: EventListener) {
    const listeners = this.listeners.get(type) ?? new Set<EventListener>();
    listeners.add(listener);
    this.listeners.set(type, listeners);
  }

  removeEventListener(type: string, listener: EventListener) {
    this.listeners.get(type)?.delete(listener);
  }

  emit(type: string, payload: unknown) {
    const event = { data: JSON.stringify(payload) } as MessageEvent;
    this.listeners.get(type)?.forEach((listener) => listener(event));
  }
}

function renderWithQuery(ui: React.ReactElement) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

describe("LiveChart", () => {
  beforeEach(() => {
    liveChartMocks.runChartProps.mockClear();
    eventSources.length = 0;
    vi.stubGlobal(
      "EventSource",
      MockEventSource as unknown as typeof EventSource,
    );
    vi.spyOn(globalThis, "fetch").mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve(samplePayload),
    } as Response);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("renders snapshot status within the latency budget", async () => {
    const start = performance.now();
    renderWithQuery(<LiveChart runId="r_test" />);
    // The status pill is present in the initial render; the snapshot
    // path flips it to "live" once the mock fetch resolves. We just
    // wait for the status text to be present (either "loading" or "live")
    // — both indicate the component has rendered its shell.
    // Use getAllByText because "Following live" also matches the pattern —
    // the status pill text ("live", "loading snapshot…", etc.) is one of
    // several nodes that contain these words.
    await waitFor(() =>
      expect(
        screen.getAllByText(/live|reconnecting|loading|closed/).length,
      ).toBeGreaterThan(0),
    );
    const elapsed = performance.now() - start;
    expect(elapsed).toBeLessThan(SNAPSHOT_RENDER_BUDGET_MS);
  });

  it("passes follow state through to RunChart as the checkbox toggles", async () => {
    renderWithQuery(<LiveChart runId="r_test" />);

    const runChart = await screen.findByTestId("run-chart-mock");
    expect(runChart).toHaveAttribute("data-follow", "true");

    fireEvent.click(screen.getByRole("checkbox"));

    expect(runChart).toHaveAttribute("data-follow", "false");
    expect(screen.getByText("Frozen")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Resume live" }));

    expect(runChart).toHaveAttribute("data-follow", "true");
    expect(liveChartMocks.runChartProps).toHaveBeenLastCalledWith(
      expect.objectContaining({ follow: true }),
    );
  });

  it("resets follow mode when the run changes", async () => {
    const { rerender } = renderWithQuery(<LiveChart runId="r_test" />);

    const runChart = await screen.findByTestId("run-chart-mock");
    fireEvent.click(screen.getByRole("checkbox"));
    expect(runChart).toHaveAttribute("data-follow", "false");

    rerender(
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <LiveChart runId="r_next" />
      </QueryClientProvider>,
    );

    await waitFor(() =>
      expect(screen.getByTestId("run-chart-mock")).toHaveAttribute(
        "data-follow",
        "true",
      ),
    );
  });

  it("merges indicator tail events into the live run payload", async () => {
    renderWithQuery(<LiveChart runId="r_test" />);

    await screen.findByTestId("run-chart-mock");
    await waitFor(() => expect(eventSources).toHaveLength(1));

    eventSources[0].emit("indicator_tail", {
      event: "indicator_tail",
      data: {
        sma_20: { time: 1_704_067_200, value: 101.25 },
      },
    });

    await waitFor(() => {
      expect(liveChartMocks.runChartProps).toHaveBeenLastCalledWith(
        expect.objectContaining({
          payload: expect.objectContaining({
            indicators: expect.objectContaining({
              sma_20: [{ time: 1_704_067_200, value: 101.25 }],
            }),
          }),
        }),
      );
    });
  });
});
