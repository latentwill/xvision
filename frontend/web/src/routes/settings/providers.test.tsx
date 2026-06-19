import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { SettingsProvidersRoute } from "./providers";
import * as settingsApi from "@/api/settings";
import type { ProviderRow } from "@/api/types.gen";

// `<ResponsiveListCard>` reads `useViewportMode()` which calls
// `window.matchMedia`. jsdom doesn't provide it; install a desktop
// stub so the route mounts. Same pattern as `routes/scenarios.test.tsx`.
function stubMatchMediaDesktop() {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string) => ({
      matches: query.includes("min-width: 1280px"),
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

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
      <MemoryRouter>
        <SettingsProvidersRoute />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  stubMatchMediaDesktop();
  vi.mocked(settingsApi.listProviders).mockResolvedValue({
    providers: [provider()],

      default_model: null,
      invalid: [],
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SettingsProvidersRoute", () => {
  it("filters the list by name via the search box", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        provider({ name: "openai", kind: "openai-compat" }),
        provider({ name: "anthropic", kind: "anthropic" }),
        provider({ name: "openrouter", kind: "openai-compat" }),
      ],
    
        default_model: null,
        invalid: [],
    });

    renderRoute();
    await screen.findByText("openai");

    const search = screen.getByPlaceholderText(/Search name or kind/i);
    fireEvent.change(search, { target: { value: "anth" } });

    // Each remaining row renders the name twice (in the name column
    // <code> and the kind column for anthropic). Use queryAllByText and
    // assert presence by length, with the unfiltered names absent.
    expect(screen.queryAllByText("openai").length).toBe(0);
    expect(screen.queryAllByText("openrouter").length).toBe(0);
    expect(screen.queryAllByText("anthropic").length).toBeGreaterThan(0);
  });

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
    
        default_model: null,
        invalid: [],
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

  it("renders a true empty state on fresh install with zero providers", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [],
      default_model: null,
      invalid: [],
    });

    renderRoute();

    expect(await screen.findByText("New provider")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Save provider" }),
    ).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Edit" })).not.toBeInTheDocument();
  });

  it("shows Ollama rows as no auth rather than missing key", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        provider({
          name: "ollama",
          kind: "ollama",
          base_url: "http://localhost:11434",
          api_key_env: "",
          api_key_set: false,
          is_default: false,
          enabled_models: ["qwen2.5-coder:7b"],
        }),
      ],
      default_model: null,
      invalid: [],
    });
    vi.mocked(settingsApi.testProviderConnection).mockResolvedValue({
      ok: true,
      status: 200,
      duration_ms: 10,
      model_count: 1,
      error: null,
    });

    renderRoute();

    await screen.findByRole("button", { name: "Test" });
    expect(screen.queryAllByText("ollama").length).toBeGreaterThan(0);
    expect(screen.getByText("no auth")).toBeInTheDocument();
    expect(screen.queryByText("○ missing")).not.toBeInTheDocument();
  });

  it("offers Gemini, Nous Research, and vLLM presets and validates custom names inline", async () => {
    renderRoute();
    await screen.findByText("openai");

    // Open the add form.
    fireEvent.click(screen.getByRole("button", { name: /Add provider/i }));

    // The two new presets appear in the provider dropdown.
    expect(
      screen.getByRole("option", { name: "Google Gemini" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("option", { name: "Nous Research" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("option", { name: "vLLM (local)" }),
    ).toBeInTheDocument();

    const form = screen.getByText("New provider").closest("form") as HTMLElement;
    fireEvent.change(within(form).getByRole("combobox"), {
      target: { value: "vllm" },
    });
    expect(screen.queryByPlaceholderText("e.g. ollama")).not.toBeInTheDocument();
    expect(screen.getByPlaceholderText("https://api.example.com/v1")).toHaveValue(
      "http://localhost:8000/v1",
    );
    expect(screen.getByPlaceholderText("paste key here")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save provider" })).toBeEnabled();

    // Switch to Custom (the only path where the user types a name) and supply a
    // valid base URL + key so the ONLY blocker is the name itself. Scope to the
    // form's Provider select (the list toolbar also renders comboboxes).
    fireEvent.change(within(form).getByRole("combobox"), {
      target: { value: "custom" },
    });
    fireEvent.change(
      screen.getByPlaceholderText("https://api.example.com/v1"),
      { target: { value: "https://api.example.com/v1" } },
    );
    fireEvent.change(screen.getByPlaceholderText("paste key here"), {
      target: { value: "sk-test" },
    });

    const nameInput = screen.getByPlaceholderText("e.g. ollama");
    fireEvent.change(nameInput, { target: { value: "Gemini" } });

    // Uppercase name → inline error + disabled submit.
    expect(
      screen.getByText(/lowercase letters, digits, and hyphens/i),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Save provider" }),
    ).toBeDisabled();

    // Fixing the name clears the error and enables submit.
    fireEvent.change(nameInput, { target: { value: "gemini" } });
    expect(
      screen.queryByText(/lowercase letters, digits, and hyphens/i),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Save provider" }),
    ).toBeEnabled();
  });

  it("surfaces invalid provider rows with an inline Remove affordance", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [provider()],
      default_model: null,
      invalid: [
        { name: "Gemini", reason: "provider name must match [a-z0-9-]+" },
      ],
    });
    vi.mocked(settingsApi.removeProvider).mockResolvedValue(
      undefined as never,
    );

    renderRoute();
    await screen.findByText(/could not be loaded/i);

    // The bad row's name + reason are shown.
    expect(screen.getByText("Gemini")).toBeInTheDocument();
    expect(screen.getByText(/must match/i)).toBeInTheDocument();

    // The strip's Remove button calls removeProvider with the bad name.
    const strip = screen
      .getByText(/could not be loaded/i)
      .closest("div") as HTMLElement;
    fireEvent.click(within(strip).getByRole("button", { name: /Remove/i }));
    await waitFor(() =>
      expect(settingsApi.removeProvider).toHaveBeenCalledWith("Gemini"),
    );
  });

  it("does not re-order rows mid-session when a checkbox is toggled", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [provider({ enabled_models: ["gpt-4.1"] })],
    
        default_model: null,
        invalid: [],
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
