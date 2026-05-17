import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
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
    expect(screen.getByRole("radio", { name: "Folio dark" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Black" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("radio", { name: "Black" }));
    expect(document.documentElement.dataset.theme).toBe("black");
  });

  it("renders all three retention modes and reflects the loaded value", async () => {
    renderRoute();

    expect(
      screen.getByRole("heading", { name: "Trace data retention" }),
    ).toBeInTheDocument();

    const fullDebug = await screen.findByRole("radio", {
      name: /Full debug/,
    });
    const redacted = await screen.findByRole("radio", { name: /Redacted/ });
    const hashOnly = await screen.findByRole("radio", { name: /Hash only/ });

    expect(fullDebug).toBeChecked();
    expect(redacted).not.toBeChecked();
    expect(hashOnly).not.toBeChecked();
  });

  it("sends a PUT when a different retention mode is picked", async () => {
    renderRoute();

    const hashOnly = await screen.findByRole("radio", { name: /Hash only/ });
    fireEvent.click(hashOnly);

    await waitFor(() => {
      expect(settingsApi.setObservabilityMode).toHaveBeenCalledWith(
        "hash_only",
      );
    });
  });
});
