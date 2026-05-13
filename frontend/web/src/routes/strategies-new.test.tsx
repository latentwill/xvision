import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
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
    listTemplates: vi.fn(),
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
  vi.mocked(strategyApi.listTemplates).mockResolvedValue([
    {
      name: "trend_follower",
      display_name: "Trend follower",
      plain_summary: "Trend starter",
    },
  ]);
  vi.mocked(strategyApi.createStrategy).mockResolvedValue({ id: "st_1" });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("StrategiesNewRoute", () => {
  it("starts from a name-first open form using the custom template by default", async () => {
    renderRoute();

    const name = screen.getByLabelText("Name");
    fireEvent.change(name, { target: { value: "Funding Fade Agent" } });
    expect(
      screen.getByText(
        "xvn strategy create --template custom --name 'Funding Fade Agent' --json",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Create strategy" }));

    await waitFor(() => {
      expect(strategyApi.createStrategy).toHaveBeenCalledWith({
        template: "custom",
        name: "Funding Fade Agent",
        creator: null,
      });
    });
    expect(navigate).toHaveBeenCalledWith("/authoring/st_1");
  });
});
