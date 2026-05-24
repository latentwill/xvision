import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { StrategiesNewRoute } from "./strategies-new";
import * as strategyApi from "@/api/strategies";

const navigate = vi.fn();

vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual<typeof import("react-router-dom")>(
    "react-router-dom",
  );
  return {
    ...actual,
    useNavigate: () => navigate,
  };
});

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return {
    ...actual,
    createStrategy: vi.fn(),
  };
});

function renderRoute() {
  return render(
    <MemoryRouter initialEntries={["/strategies/new"]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <StrategiesNewRoute />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  navigate.mockReset();
  vi.mocked(strategyApi.createStrategy).mockResolvedValue({ id: "st_1" });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("StrategiesNewRoute", () => {
  it("creates a blank strategy immediately and opens authoring", async () => {
    renderRoute();

    expect(screen.getByText("Creating strategy...")).toBeInTheDocument();

    await waitFor(() => {
      expect(strategyApi.createStrategy).toHaveBeenCalledWith({
        name: "Untitled strategy",
        creator: null,
      });
    });
    expect(navigate).toHaveBeenCalledWith("/strategies/st_1", { replace: true });
  });

  it("shows the creation error with a back link", async () => {
    vi.mocked(strategyApi.createStrategy).mockRejectedValue(
      new Error("network down"),
    );

    renderRoute();

    expect(
      await screen.findByText("couldn't create strategy"),
    ).toBeInTheDocument();
    expect(screen.getByText("network down")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Back to strategies/i }))
      .toHaveAttribute("href", "/strategies");
  });
});
