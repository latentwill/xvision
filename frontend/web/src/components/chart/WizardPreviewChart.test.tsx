import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const chartMocks = vi.hoisted(() => ({
  useQuery: vi.fn(),
  useBarsFetchJob: vi.fn(),
  ScenarioChart: vi.fn(),
  getScenarioPreview: vi.fn(),
}));

vi.mock("@tanstack/react-query", async () => {
  const actual = await vi.importActual<typeof import("@tanstack/react-query")>(
    "@tanstack/react-query",
  );
  return {
    ...actual,
    useQuery: chartMocks.useQuery,
  };
});

vi.mock("@/api/chart", () => ({
  getScenarioPreview: chartMocks.getScenarioPreview,
}));

vi.mock("@/components/scenario/useBarsFetchJob", () => ({
  useBarsFetchJob: chartMocks.useBarsFetchJob,
}));

vi.mock("./ScenarioChart", () => ({
  ScenarioChart: (props: { payload: { scenario: { display_name: string } } }) => {
    chartMocks.ScenarioChart(props);
    return <div data-testid="scenario-chart">{props.payload.scenario.display_name}</div>;
  },
}));

import { WizardPreviewChart } from "./WizardPreviewChart";

const previewData = {
  cache_key: "preview-cache",
  bars: [{ time: 1, open: 1, high: 1, low: 1, close: 1, volume: 1 }],
  cache_status: { type: "FullyCached", bar_count: 1, fetched_at: "2026-05-13T00:00:00Z" },
  baseline_equity: [],
};

function renderChart() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <WizardPreviewChart
        asset="BTC"
        from="2026-05-01"
        to="2026-05-02"
        granularity="Hour1"
      />
    </QueryClientProvider>,
  );
}

describe("WizardPreviewChart", () => {
  beforeEach(() => {
    chartMocks.useQuery.mockReturnValue({
      data: previewData,
      isLoading: false,
      error: null,
    });
    chartMocks.useBarsFetchJob.mockReturnValue({
      start: vi.fn(),
      canStart: true,
      statusText: null,
      outputText: null,
      errorText: null,
    });
    chartMocks.ScenarioChart.mockClear();
    chartMocks.getScenarioPreview.mockClear();
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("keeps the preview hidden until the operator clicks the button", async () => {
    const user = userEvent.setup();

    renderChart();

    expect(screen.getByRole("button", { name: "Show preview chart" })).toBeInTheDocument();
    expect(screen.queryByTestId("scenario-chart")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Show preview chart" }));

    expect(await screen.findByTestId("scenario-chart")).toHaveTextContent("Preview");
    expect(chartMocks.ScenarioChart).toHaveBeenCalledTimes(1);
    expect(chartMocks.getScenarioPreview).not.toHaveBeenCalled();
  });
});
