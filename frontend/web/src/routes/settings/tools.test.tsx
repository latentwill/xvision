import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { SettingsToolsRoute } from "./tools";
import * as toolsApi from "@/api/dataTools";

// `<ResponsiveListCard>` reads `useViewportMode()` which calls
// `window.matchMedia`. jsdom doesn't provide it; install a desktop
// stub so the route mounts. Same pattern as providers.test.tsx.
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

vi.mock("@/api/dataTools", async () => {
  const actual = await vi.importActual<typeof import("@/api/dataTools")>(
    "@/api/dataTools",
  );
  return {
    ...actual,
    getDataTools: vi.fn(),
    setDataTools: vi.fn(),
  };
});

function makeEntry(overrides: Partial<{
  kind: "nansen" | "elfa";
  base_url: string;
  api_key_env: string;
  enabled: boolean;
  budget_credits_per_run: number | null;
  nansen_lookahead_lag_days: number | null;
}> = {}) {
  return {
    kind: "nansen" as const,
    base_url: "https://api.nansen.ai/v1",
    api_key_env: "NANSEN_API_KEY",
    enabled: true,
    budget_credits_per_run: 100,
    nansen_lookahead_lag_days: 1,
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
        <SettingsToolsRoute />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  stubMatchMediaDesktop();
  vi.mocked(toolsApi.getDataTools).mockResolvedValue({
    data_tools: [makeEntry()],
  });
  vi.mocked(toolsApi.setDataTools).mockResolvedValue({
    data_tools: [makeEntry()],
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SettingsToolsRoute", () => {
  it("renders the data_tools list with api_key_env shown", async () => {
    renderRoute();

    // The page heading must be visible.
    expect(await screen.findByText(/data tools/i)).toBeInTheDocument();

    // The Nansen row's api_key_env must appear (wait for data to load).
    expect(await screen.findByText("NANSEN_API_KEY")).toBeInTheDocument();
  });

  it("shows the tool kind and base_url in the list", async () => {
    renderRoute();

    await screen.findByText("NANSEN_API_KEY");

    // Kind must appear.
    expect(screen.getByText("nansen")).toBeInTheDocument();
    // Base URL must appear.
    expect(screen.getByText("https://api.nansen.ai/v1")).toBeInTheDocument();
  });

  it("shows both nansen and elfa entries when both are present", async () => {
    vi.mocked(toolsApi.getDataTools).mockResolvedValue({
      data_tools: [
        makeEntry({ kind: "nansen", api_key_env: "NANSEN_API_KEY" }),
        makeEntry({
          kind: "elfa",
          base_url: "https://api.elfa.ai/v1",
          api_key_env: "ELFA_API_KEY",
          budget_credits_per_run: null,
          nansen_lookahead_lag_days: null,
        }),
      ],
    });

    renderRoute();

    await screen.findByText("NANSEN_API_KEY");
    expect(screen.getByText("ELFA_API_KEY")).toBeInTheDocument();
    expect(screen.getByText("elfa")).toBeInTheDocument();
  });

  it("toggling enabled and saving issues a PUT with the updated list", async () => {
    vi.mocked(toolsApi.getDataTools).mockResolvedValue({
      data_tools: [makeEntry({ enabled: true })],
    });
    vi.mocked(toolsApi.setDataTools).mockResolvedValue({
      data_tools: [makeEntry({ enabled: false })],
    });

    renderRoute();

    await screen.findByText("NANSEN_API_KEY");

    // Click the enabled toggle on the first row.
    const toggle = screen.getByRole("checkbox", { name: /enabled/i });
    fireEvent.click(toggle);

    // Find and click the Save button.
    const saveButton = screen.getByRole("button", { name: /save/i });
    fireEvent.click(saveButton);

    await waitFor(() => {
      expect(toolsApi.setDataTools).toHaveBeenCalledWith({
        data_tools: expect.arrayContaining([
          expect.objectContaining({ enabled: false }),
        ]),
      });
    });
  });

  it("shows empty state when no data_tools configured", async () => {
    vi.mocked(toolsApi.getDataTools).mockResolvedValue({
      data_tools: [],
    });

    renderRoute();

    // Should show an empty / no-tools message.
    expect(await screen.findByText(/no data tools/i)).toBeInTheDocument();
  });

  it("shows an error state when the GET fails", async () => {
    vi.mocked(toolsApi.getDataTools).mockRejectedValue(
      new Error("network error"),
    );

    renderRoute();

    expect(await screen.findByText(/network error/i)).toBeInTheDocument();
  });
});
