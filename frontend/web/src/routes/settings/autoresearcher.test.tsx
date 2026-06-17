import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { createElement, type ReactNode } from "react";
import { AutoresearcherSettingsRoute } from "./autoresearcher";

afterEach(() => cleanup());

// vi.hoisted runs before vi.mock hoisting, making the fn safe to reference
// inside the factory AND in test assertions.
const { mockSetAutoresearchConfig } = vi.hoisted(() => ({
  mockSetAutoresearchConfig: vi.fn().mockResolvedValue({
    min_precision_lift_pp: 3.0,
    max_pnl_regression: 0.0,
    promotion_epsilon: 0.01,
    promotion_acc_floor: 0.52,
    promotion_min_holdout: 200,
    min_cycle_count: 500,
    train_wall_clock_sec: 300,
    price_forward_threshold: 0.003,
  }),
}));

vi.mock("@/api/autoresearch-config", () => {
  const data = {
    min_precision_lift_pp: 3.0,
    max_pnl_regression: 0.0,
    promotion_epsilon: 0.01,
    promotion_acc_floor: 0.52,
    promotion_min_holdout: 200,
    min_cycle_count: 500,
    train_wall_clock_sec: 300,
    price_forward_threshold: 0.003,
  };
  return {
    getAutoresearchConfig: vi.fn().mockResolvedValue(data),
    setAutoresearchConfig: mockSetAutoresearchConfig,
    autoresearchConfigKeys: {
      all: ["autoresearch-config"],
      config: () => ["autoresearch-config", "config"],
    },
    useAutoresearchConfig: () => ({
      data,
      isPending: false,
      isError: false,
    }),
    useSetAutoresearchConfig: () => ({
      mutate: mockSetAutoresearchConfig,
      isPending: false,
      isSuccess: false,
      isError: false,
      error: null,
    }),
  };
});

function makeWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) =>
    createElement(
      QueryClientProvider,
      { client: qc },
      createElement(MemoryRouter, null, children),
    );
}

describe("AutoresearcherSettingsRoute", () => {
  it("renders all seven config fields", async () => {
    render(<AutoresearcherSettingsRoute />, { wrapper: makeWrapper() });
    expect(await screen.findByLabelText(/min.*precision.*lift/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/max.*pnl.*regression/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/promotion.*epsilon/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/promotion.*acc.*floor/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/promotion.*min.*holdout/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/min.*cycle.*count/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/train.*wall.*clock/i)).toBeInTheDocument();
  });

  it("shows validation error when promotion_acc_floor is set below 0", async () => {
    render(<AutoresearcherSettingsRoute />, { wrapper: makeWrapper() });
    const input = await screen.findByLabelText(/promotion.*acc.*floor/i);
    fireEvent.change(input, { target: { value: "-0.1" } });
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
  });

  it("shows validation error when promotion_acc_floor is set above 1", async () => {
    render(<AutoresearcherSettingsRoute />, { wrapper: makeWrapper() });
    const input = await screen.findByLabelText(/promotion.*acc.*floor/i);
    fireEvent.change(input, { target: { value: "1.5" } });
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
  });

  it("shows validation error when promotion_min_holdout is negative", async () => {
    render(<AutoresearcherSettingsRoute />, { wrapper: makeWrapper() });
    const input = await screen.findByLabelText(/promotion.*min.*holdout/i);
    fireEvent.change(input, { target: { value: "-1" } });
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
  });

  it("calls setAutoresearchConfig with the updated values on save", async () => {
    const configApi = await import("@/api/autoresearch-config");
    render(<AutoresearcherSettingsRoute />, { wrapper: makeWrapper() });

    const input = await screen.findByLabelText(/min.*precision.*lift/i);
    fireEvent.change(input, { target: { value: "5.0" } });

    const saveBtn = screen.getByRole("button", { name: /save/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(vi.mocked(configApi.setAutoresearchConfig)).toHaveBeenCalledWith(
        expect.objectContaining({ min_precision_lift_pp: 5.0 }),
      );
    });
  });

  it("disables Save while validation errors are present", async () => {
    render(<AutoresearcherSettingsRoute />, { wrapper: makeWrapper() });
    const input = await screen.findByLabelText(/promotion.*acc.*floor/i);
    fireEvent.change(input, { target: { value: "2.0" } });
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /save/i })).toBeDisabled();
    });
  });
});
