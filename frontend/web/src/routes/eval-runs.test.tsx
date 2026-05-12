import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { EvalRunsRoute } from "./eval-runs";
import * as evalApi from "@/api/eval";
import * as strategyApi from "@/api/strategies";

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>(
    "@/api/eval",
  );
  return {
    ...actual,
    listRuns: vi.fn(),
    listScenarios: vi.fn(),
    startRun: vi.fn(),
  };
});

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return {
    ...actual,
    listStrategies: vi.fn(),
  };
});

function renderRoute(initialEntry = "/eval-runs") {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <EvalRunsRoute />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("EvalRunsRoute", () => {
  it("preselects strategy from the query string in the start eval dialog", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    vi.mocked(evalApi.listScenarios).mockResolvedValue([
      {
        id: "crypto-bull-q1-2025",
        display_name: "Bull",
        asset_universe: [],
        regime_tags: [],
        time_window_days: 90,
      },
    ]);
    vi.mocked(strategyApi.listStrategies).mockResolvedValue([
      {
        agent_id: "01TEST",
        display_name: "Trend 4H",
        template: "trend_follower",
        decision_cadence_minutes: 240,
      },
    ]);

    renderRoute("/eval-runs?strategy=01TEST&start=1");

    const strategy = (await screen.findByLabelText("Strategy")) as HTMLSelectElement;
    expect(strategy.value).toBe("01TEST");
  });
});
