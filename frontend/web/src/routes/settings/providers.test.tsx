import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { SettingsProvidersRoute } from "./providers";
import * as settingsApi from "@/api/settings";
import type { ProviderRow, ProvidersReport } from "@/api/types.gen";

vi.mock("@/api/settings", async () => {
  const actual = await vi.importActual<typeof import("@/api/settings")>(
    "@/api/settings",
  );
  return {
    ...actual,
    listProviders: vi.fn(),
    addProvider: vi.fn(),
    updateProvider: vi.fn(),
    removeProvider: vi.fn(),
    setDefaultProvider: vi.fn(),
    listProviderModels: vi.fn(),
    setEnabledModels: vi.fn(),
    testProviderConnection: vi.fn(),
  };
});

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

function provider(overrides: Partial<ProviderRow> = {}): ProviderRow {
  return {
    name: "anthropic",
    kind: "anthropic",
    base_url: "https://api.anthropic.com",
    api_key_env: "ANTHROPIC_API_KEY",
    api_key_set: true,
    synthetic: false,
    is_default: true,
    enabled_models: ["claude-sonnet-4-6"],
    ...overrides,
  };
}

function report(rows: ProviderRow[]): ProvidersReport {
  return {
    providers: rows,
    default_model: rows.find((row) => row.is_default)?.enabled_models[0],
  };
}

describe("SettingsProvidersRoute", () => {
  beforeEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("lets the current default provider be removed", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue(report([provider()]));
    vi.mocked(settingsApi.removeProvider).mockResolvedValue(undefined);

    renderRoute();

    const remove = await screen.findByRole("button", { name: /remove/i });
    expect(remove).not.toBeDisabled();

    fireEvent.click(remove);

    await waitFor(() => {
      expect(settingsApi.removeProvider).toHaveBeenCalledWith("anthropic");
    });
  });

  it("edits an existing provider row", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue(report([provider()]));
    vi.mocked(settingsApi.updateProvider).mockResolvedValue(
      provider({
        base_url: "https://proxy.example/v1",
        api_key_env: "ANTHROPIC_PROXY_KEY",
      }),
    );

    renderRoute();

    fireEvent.click(await screen.findByRole("button", { name: /edit/i }));
    fireEvent.change(screen.getByLabelText("Base URL"), {
      target: { value: "https://proxy.example/v1" },
    });
    fireEvent.change(screen.getByLabelText("API key env"), {
      target: { value: "ANTHROPIC_PROXY_KEY" },
    });
    fireEvent.change(screen.getByLabelText(/New API key/), {
      target: { value: "sk-updated" },
    });
    fireEvent.click(screen.getByRole("button", { name: /save changes/i }));

    await waitFor(() => {
      expect(settingsApi.updateProvider).toHaveBeenCalledWith("anthropic", {
        kind: "anthropic",
        base_url: "https://proxy.example/v1",
        api_key_env: "ANTHROPIC_PROXY_KEY",
        api_key: "sk-updated",
      });
    });
  });

  it("does not synthesize a default provider in the empty state", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue(report([]));

    renderRoute();

    expect(await screen.findByText("New provider")).toBeInTheDocument();
    expect(screen.queryByText("_default_llm")).not.toBeInTheDocument();
    expect(screen.queryByText("Default LLM")).not.toBeInTheDocument();
  });
});
