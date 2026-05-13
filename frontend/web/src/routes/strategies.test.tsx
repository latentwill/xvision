import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { StrategiesRoute } from "./strategies";
import * as strategiesApi from "@/api/strategies";

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return {
    ...actual,
    listStrategies: vi.fn(),
  };
});

function renderRoute() {
  return render(
    <MemoryRouter>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <StrategiesRoute />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("StrategiesRoute", () => {
  it("renders strategy id, display name, model summary, tags, and humanized cadence", async () => {
    vi.mocked(strategiesApi.listStrategies).mockResolvedValue([
      {
        agent_id: "01TEST",
        display_name: "Trend 4H",
        template: "trend_follower",
        decision_cadence_minutes: 240,
        model: "claude-sonnet +1",
        tags: ["trend_follower", "BTC/USD", "trending_bull"],
      },
    ]);

    renderRoute();

    expect((await screen.findAllByText("Strategy ID")).length).toBeGreaterThan(0);
    expect(screen.getAllByText("Trend 4H").length).toBeGreaterThan(0);
    expect(screen.getAllByText("4h").length).toBeGreaterThan(0);
    expect(screen.getAllByText("claude-sonnet +1").length).toBeGreaterThan(0);
    expect(screen.getAllByText("BTC/USD").length).toBeGreaterThan(0);
    expect(screen.getAllByText("trending_bull").length).toBeGreaterThan(0);
  });
});
