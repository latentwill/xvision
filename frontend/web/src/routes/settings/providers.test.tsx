import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
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
    listProviderModels: vi.fn(),
    setEnabledModels: vi.fn(),
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

  it("renders enabled models above unselected models in the Pick models dialog", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        provider({
          enabled_models: ["gpt-4.1-mini", "gpt-4.1"],
        }),
      ],
    });
    vi.mocked(settingsApi.listProviderModels).mockResolvedValue({
      models: [
        // Intentionally interleaved: enabled rows are NOT first in catalog order.
        { id: "gpt-3.5-turbo", display_name: null, owned_by: null, context_length: null },
        { id: "gpt-4.1", display_name: null, owned_by: null, context_length: null },
        { id: "gpt-4o", display_name: null, owned_by: null, context_length: null },
        { id: "gpt-4.1-mini", display_name: null, owned_by: null, context_length: null },
        { id: "o1-preview", display_name: null, owned_by: null, context_length: null },
      ],
    });

    renderRoute();

    fireEvent.click(
      await screen.findByRole("button", { name: /Models · 2/ }),
    );

    // Wait for catalog rows to render.
    await screen.findByText("gpt-3.5-turbo");

    const rows = screen
      .getAllByRole("checkbox")
      .map((cb) => cb.closest("tr")!)
      .filter((tr): tr is HTMLTableRowElement => tr !== null);
    const ids = rows.map(
      (row) => row.querySelector("code")?.textContent ?? "",
    );

    // First two rows must be the enabled models (in upstream order within the
    // enabled group: gpt-4.1 appears before gpt-4.1-mini in the catalog).
    expect(ids.slice(0, 2)).toEqual(["gpt-4.1", "gpt-4.1-mini"]);
    // Remaining rows are the unselected ones, preserving upstream order.
    expect(ids.slice(2)).toEqual(["gpt-3.5-turbo", "gpt-4o", "o1-preview"]);
  });

  it("does not re-order rows mid-session when a checkbox is toggled", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [provider({ enabled_models: ["gpt-4.1"] })],
    });
    vi.mocked(settingsApi.listProviderModels).mockResolvedValue({
      models: [
        { id: "gpt-3.5-turbo", display_name: null, owned_by: null, context_length: null },
        { id: "gpt-4.1", display_name: null, owned_by: null, context_length: null },
        { id: "gpt-4o", display_name: null, owned_by: null, context_length: null },
      ],
    });

    renderRoute();

    fireEvent.click(
      await screen.findByRole("button", { name: /Models · 1/ }),
    );
    await screen.findByText("gpt-3.5-turbo");

    // Toggle on gpt-4o (currently unselected). It should NOT jump above
    // gpt-3.5-turbo — ordering is driven by persisted state, not local state.
    const gpt4oRow = screen.getByText("gpt-4o").closest("tr") as HTMLElement;
    fireEvent.click(within(gpt4oRow).getByRole("checkbox"));

    const rows = screen
      .getAllByRole("checkbox")
      .map((cb) => cb.closest("tr")!)
      .filter((tr): tr is HTMLTableRowElement => tr !== null);
    const ids = rows.map(
      (row) => row.querySelector("code")?.textContent ?? "",
    );
    expect(ids).toEqual(["gpt-4.1", "gpt-3.5-turbo", "gpt-4o"]);
  });
});
