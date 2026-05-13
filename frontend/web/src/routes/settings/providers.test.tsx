import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { SettingsProvidersRoute } from "./providers";
import * as settingsApi from "@/api/settings";
import type { ProviderRow } from "@/api/types.gen";

vi.mock("@/api/settings", async () => {
  const actual = await vi.importActual<typeof import("@/api/settings")>(
    "@/api/settings",
  );
  return {
    ...actual,
    listProviders: vi.fn(),
    removeProvider: vi.fn(),
    testProviderConnection: vi.fn(),
    updateProvider: vi.fn(),
  };
});

function provider(overrides: Partial<ProviderRow> = {}): ProviderRow {
  return {
    name: "openai",
    kind: "openai-compat",
    base_url: "https://api.openai.com/v1",
    api_key_env: "OPENAI_API_KEY",
    api_key_set: true,
    synthetic: false,
    is_default: true,
    enabled_models: ["gpt-4.1-mini"],
    ...overrides,
  };
}

function renderRoute() {
  return render(
    <QueryClientProvider
      client={
        new QueryClient({
          defaultOptions: { queries: { retry: false } },
        })
      }
    >
      <SettingsProvidersRoute />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.mocked(settingsApi.listProviders).mockResolvedValue({
    providers: [provider()],
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SettingsProvidersRoute", () => {
  it("does not expose API key env in the provider edit UI", async () => {
    renderRoute();

    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));

    expect(screen.queryByText("API key env")).not.toBeInTheDocument();
    expect(screen.queryByDisplayValue("OPENAI_API_KEY")).not.toBeInTheDocument();
  });
});
