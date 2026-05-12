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
  it("renders Strategy ID and display name with a humanized cadence", async () => {
    vi.mocked(strategiesApi.listStrategies).mockResolvedValue([
      {
        agent_id: "01TEST",
        display_name: "Trend 4H",
        template: "trend_follower",
        decision_cadence_minutes: 240,
        model: "claude-sonnet",
      },
    ]);

    renderRoute();

    expect(await screen.findByText("Strategy ID")).toBeTruthy();
    expect(screen.getByText("Trend 4H")).toBeTruthy();
    expect(screen.getByText("4h")).toBeTruthy();
  });
});
