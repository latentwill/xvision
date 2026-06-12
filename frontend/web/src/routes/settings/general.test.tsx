import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  act,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { ThemeProvider } from "@/theme/ThemeProvider";
import { SettingsGeneralRoute } from "./general";
import type { ObservabilityReport } from "@/api/types.gen";

vi.mock("@/api/settings", async () => {
  const actual = await vi.importActual<typeof import("@/api/settings")>(
    "@/api/settings",
  );
  return {
    ...actual,
    getObservability: vi.fn(),
    setObservabilityMode: vi.fn(),
  };
});

const settingsApi = await import("@/api/settings");

function obsReport(
  overrides: Partial<ObservabilityReport> = {},
): ObservabilityReport {
  return {
    mode: "full_debug",
    store_prompts: true,
    store_responses: true,
    store_tool_inputs: true,
    store_tool_outputs: true,
    redact_secrets: true,
    payload_ttl_days: 7n as unknown as bigint,
    max_payload_bytes: 200_000n as unknown as bigint,
    persisted: false,
    ...overrides,
  };
}

function renderRoute() {
  return render(
    <ThemeProvider>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <SettingsGeneralRoute />
      </QueryClientProvider>
    </ThemeProvider>,
  );
}

beforeEach(() => {
  vi.mocked(settingsApi.getObservability).mockResolvedValue(obsReport());
  vi.mocked(settingsApi.setObservabilityMode).mockImplementation(async (mode) =>
    obsReport({ mode, persisted: true }),
  );
});

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.className = "";
  vi.clearAllMocks();
});

describe("SettingsGeneralRoute", () => {
  it("renders appearance choices and persists selection", () => {
    renderRoute();

    expect(
      screen.getByRole("heading", { name: "Appearance" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Auto" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Light" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Dark" })).toBeChecked();

    fireEvent.click(screen.getByRole("radio", { name: "Light" }));
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("does not render retention mode controls", async () => {
    renderRoute();

    expect(
      screen.queryByRole("heading", { name: "Trace data retention" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: /Full debug/ })).not.toBeInTheDocument();
    expect(settingsApi.setObservabilityMode).not.toHaveBeenCalled();
  });

  it("shows helper text clarifying accent changes apply instantly", () => {
    renderRoute();

    // The accent section must contain text that tells the user no Save
    // button is needed — changes are auto-saved immediately.
    expect(
      screen.getByText(/applies instantly|saved automatically|auto.?saved/i),
    ).toBeInTheDocument();
  });

  it("shows a transient saved confirmation after clicking an accent swatch", async () => {
    renderRoute();

    // Click the Azure accent swatch (not the default green).
    const azureButton = screen.getByRole("button", { name: /azure accent/i });
    await act(async () => {
      fireEvent.click(azureButton);
    });

    // A non-modal "Saved" affordance must appear inline.
    await waitFor(() => {
      expect(screen.getByText(/saved/i)).toBeInTheDocument();
    });
  });
});
