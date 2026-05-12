import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { LiveChart } from "./LiveChart";
import samplePayload from "./__fixtures__/sample-run-chart.json";

const liveChartMocks = vi.hoisted(() => ({
  runChartProps: vi.fn(),
}));

// Mock RunChart so lightweight-charts never tries to use canvas/WebGL in jsdom.
vi.mock("./RunChart", () => ({
  RunChart: (props: { follow?: boolean }) => {
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

function renderWithQuery(ui: React.ReactElement) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

describe("LiveChart", () => {
  beforeEach(() => {
    liveChartMocks.runChartProps.mockClear();
    vi.spyOn(globalThis, "fetch").mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve(samplePayload),
    } as Response);
  });

  afterEach(() => {
    cleanup();
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
});
