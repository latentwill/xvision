import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "react-router-dom";

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>("@/api/eval");
  return {
    ...actual,
    listRuns: vi.fn().mockResolvedValue([]),
  };
});

vi.mock("@/api/scenarios", async () => {
  const actual = await vi.importActual<typeof import("@/api/scenarios")>("@/api/scenarios");
  return {
    ...actual,
    listScenarios: vi.fn().mockResolvedValue([]),
  };
});

vi.mock("@/api/settings", async () => {
  const actual = await vi.importActual<typeof import("@/api/settings")>("@/api/settings");
  return {
    ...actual,
    getBrokers: vi.fn(),
    listProviders: vi.fn(),
  };
});

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>("@/api/strategies");
  return {
    ...actual,
    listStrategies: vi.fn().mockResolvedValue([]),
  };
});

describe("legacy eval route", () => {
  beforeEach(() => {
    vi.resetModules();
    window.history.pushState({}, "", "/eval");
  });

  afterEach(() => {
    cleanup();
  });

  it("redirects /eval to /eval-runs", async () => {
    const { router } = await import("../routes.tsx");
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });

    render(
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );

    await waitFor(() => {
      expect(window.location.pathname).toBe("/eval-runs");
    });
  });
});
