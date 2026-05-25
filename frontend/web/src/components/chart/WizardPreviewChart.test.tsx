import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { getScenarioPreview } from "@/api/chart";
import { WizardPreviewChart } from "./WizardPreviewChart";

vi.mock("@/api/chart", () => ({
  getScenarioPreview: vi.fn(),
}));

vi.mock("@/components/scenario/useBarsFetchJob", () => ({
  useBarsFetchJob: () => ({
    start: vi.fn(),
    statusText: null,
    outputText: null,
    errorText: null,
    canStart: false,
  }),
}));

vi.mock("./ScenarioChart", () => ({
  ScenarioChart: ({ payload }: { payload: { bars: unknown[] } }) => (
    <div data-testid="scenario-chart">bars:{payload.bars.length}</div>
  ),
}));

function renderWithClient(ui: React.ReactElement) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("WizardPreviewChart", () => {
  it("keeps hook order stable when preview data resolves", async () => {
    vi.mocked(getScenarioPreview).mockResolvedValue({
      bars: [
        {
          t: "2024-01-01T00:00:00Z",
          o: 1,
          h: 2,
          l: 0.5,
          c: 1.5,
          v: 100,
        },
      ],
      indicators: {},
      cache_status: { type: "Cached", count: 1 },
      cache_key: "preview-cache",
      baseline_equity: null,
    } as never);

    renderWithClient(
      <WizardPreviewChart
        asset="BTC"
        from="2024-01-01T00:00:00Z"
        to="2024-01-02T00:00:00Z"
        granularity="1Hour"
      />,
    );

    fireEvent.click(screen.getByTestId("wizard-preview-show"));

    await waitFor(() => {
      expect(screen.getByTestId("scenario-chart")).toHaveTextContent("bars:1");
    });
  });
});
