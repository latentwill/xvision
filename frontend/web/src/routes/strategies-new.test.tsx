import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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
  it("does not create a strategy on mount — waits for explicit submit", () => {
    renderRoute();
    expect(strategyApi.createStrategy).not.toHaveBeenCalled();
  });

  it("renders the name input and Create strategy button", () => {
    renderRoute();
    expect(screen.getByLabelText("Name")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /create strategy/i }),
    ).toBeInTheDocument();
  });

  it("submits with typed name and navigates to the new strategy", async () => {
    const user = userEvent.setup();
    renderRoute();

    await user.type(screen.getByLabelText("Name"), "Momentum BTC");
    await user.click(screen.getByRole("button", { name: /create strategy/i }));

    await waitFor(() => {
      expect(strategyApi.createStrategy).toHaveBeenCalledWith({
        name: "Momentum BTC",
        creator: null,
      });
    });
    expect(navigate).toHaveBeenCalledWith("/strategies/st_1", {
      replace: true,
    });
  });

  it("falls back to 'Untitled strategy' when name is blank", async () => {
    const user = userEvent.setup();
    renderRoute();

    await user.click(screen.getByRole("button", { name: /create strategy/i }));

    await waitFor(() => {
      expect(strategyApi.createStrategy).toHaveBeenCalledWith({
        name: "Untitled strategy",
        creator: null,
      });
    });
  });

  it("shows the creation error inline without navigating", async () => {
    vi.mocked(strategyApi.createStrategy).mockRejectedValue(
      new Error("network down"),
    );
    const user = userEvent.setup();
    renderRoute();

    await user.click(screen.getByRole("button", { name: /create strategy/i }));

    expect(
      await screen.findByText("couldn't create strategy"),
    ).toBeInTheDocument();
    expect(screen.getByText("network down")).toBeInTheDocument();
    expect(navigate).not.toHaveBeenCalled();
  });

  it("renders a Cancel link back to /strategies", () => {
    renderRoute();
    expect(screen.getByRole("link", { name: /cancel/i })).toHaveAttribute(
      "href",
      "/strategies",
    );
  });
});
