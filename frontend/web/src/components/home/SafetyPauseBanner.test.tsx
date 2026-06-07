import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import { getSafetyState } from "@/api/safety";
import { SafetyPauseBanner } from "./SafetyPauseBanner";

vi.mock("@/api/safety", async () => {
  const actual = await vi.importActual<typeof import("@/api/safety")>(
    "@/api/safety",
  );
  return {
    ...actual,
    getSafetyState: vi.fn(),
  };
});

function renderBanner() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <SafetyPauseBanner />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SafetyPauseBanner", () => {
  it("renders full-width danger banner with reason text when paused: true", async () => {
    vi.mocked(getSafetyState).mockResolvedValue({
      paused: true,
      reason: "Drawdown limit hit",
    });

    renderBanner();

    const alert = await screen.findByRole("alert");
    expect(alert).toBeInTheDocument();
    expect(alert.textContent).toContain("Drawdown limit hit");

    const link = screen.getByRole("link", { name: /Go to Safety/i });
    expect(link).toHaveAttribute("href", "/safety");
  });

  it("renders banner without reason when reason is null", async () => {
    vi.mocked(getSafetyState).mockResolvedValue({
      paused: true,
      reason: null,
    });

    renderBanner();

    const alert = await screen.findByRole("alert");
    expect(alert).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Go to Safety/i })).toBeInTheDocument();
  });

  it("returns null when paused: false", async () => {
    vi.mocked(getSafetyState).mockResolvedValue({
      paused: false,
      reason: null,
    });

    renderBanner();

    // Nothing should be rendered — no alert role
    await vi.waitFor(() => {
      expect(screen.queryByRole("alert")).toBeNull();
    });
  });

  it("returns null while loading (isPending)", () => {
    // Never resolves — stays pending
    vi.mocked(getSafetyState).mockReturnValue(new Promise(() => {}));

    renderBanner();

    expect(screen.queryByRole("alert")).toBeNull();
  });
});
