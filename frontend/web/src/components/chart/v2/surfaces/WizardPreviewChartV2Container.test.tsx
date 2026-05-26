/**
 * Behavior parity tests for WizardPreviewChartV2Container (Task 8).
 *
 * The container reproduces the v1 WizardPreviewChart gating/fetch logic
 * verbatim, only swapping the rendered surface to WizardPreviewChartV2.
 * The v2 surface instantiates klinecharts + uPlot, so we stub it to a
 * lightweight testid marker. useBarsFetchJob is stubbed so the container
 * does not reach the CLI job API. Renders are wrapped in a
 * QueryClientProvider with retry disabled.
 */
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

vi.mock("./WizardPreviewChartV2", () => ({
  WizardPreviewChartV2: () => <div data-testid="wizard-preview-chart-v2" />,
}));

const barsFetchStub = {
  start: vi.fn(),
  canStart: true,
  isActive: false,
  statusText: null,
  outputText: null,
  errorText: null,
  job: undefined,
};
vi.mock("@/components/scenario/useBarsFetchJob", () => ({
  useBarsFetchJob: () => barsFetchStub,
}));

const getScenarioPreviewMock = vi.fn();
vi.mock("@/api/chart", () => ({
  getScenarioPreview: (...args: unknown[]) => getScenarioPreviewMock(...args),
}));

import { WizardPreviewChartV2Container } from "./WizardPreviewChartV2Container";

function renderContainer(props: {
  asset: string;
  from: string;
  to: string;
  granularity: string;
  includeBaseline?: boolean;
}) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <WizardPreviewChartV2Container {...props} />
    </QueryClientProvider>,
  );
}

describe("WizardPreviewChartV2Container", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
  });

  it("prompts to fill the form when not ready", () => {
    renderContainer({ asset: "BTC", from: "", to: "", granularity: "1h" });
    expect(
      screen.getByText("Fill asset + date range to see preview…"),
    ).toBeInTheDocument();
  });

  it("shows the gate button (not the chart) when ready but not shown", async () => {
    renderContainer({
      asset: "BTC",
      from: "2024-01-01",
      to: "2024-02-01",
      granularity: "1h",
    });
    // Debounce (350ms) must settle for `ready` to flip true.
    const showBtn = await screen.findByTestId("wizard-preview-show");
    expect(showBtn).toHaveTextContent("Show preview chart");
    expect(
      screen.queryByTestId("wizard-preview-chart-v2"),
    ).not.toBeInTheDocument();
  });

  it("renders the v2 surface + header + hide button after clicking show", async () => {
    getScenarioPreviewMock.mockResolvedValue({
      cache_key: "k",
      asset: "BTC",
      granularity: "1h",
      bars: [],
      cache_status: { state: "Ready" } as never,
      baseline_equity: null,
    });

    renderContainer({
      asset: "BTC",
      from: "2024-01-01",
      to: "2024-02-01",
      granularity: "1h",
    });

    fireEvent.click(await screen.findByTestId("wizard-preview-show"));

    await waitFor(() =>
      expect(
        screen.getByTestId("wizard-preview-chart-v2"),
      ).toBeInTheDocument(),
    );
    expect(getScenarioPreviewMock).toHaveBeenCalled();
    expect(
      screen.getByText(/Preview — BTC · 2024-01-01 → 2024-02-01 · 1h/),
    ).toBeInTheDocument();
    expect(screen.getByTestId("wizard-preview-hide")).toBeInTheDocument();
  });
});
