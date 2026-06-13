/**
 * WU9 — Tests for the StrategiesPineLibraryRoute component.
 *
 * Tests:
 * 1. Renders library entries from a mocked GET response (list renders).
 * 2. Empty-library state renders when items is [].
 * 3. No overlay element present (dialog/modal/sheet/popover).
 * 4. Clicking Import triggers the POST endpoint (mocked).
 */

import { cleanup, render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrategiesPineLibraryRoute } from "./strategies-pine-library";

// ─── helpers ────────────────────────────────────────────────────────────────

function mockJson(body: unknown, ok = true) {
  return Promise.resolve({
    ok,
    status: ok ? 200 : 404,
    json: () => Promise.resolve(body),
  } as Response);
}

const MOCK_LIBRARY = {
  items: [
    { id: "rsi-threshold", name: "RSI Threshold", description: "RSI fade strategy" },
    { id: "ma-crossover", name: "MA Crossover", description: "MA cross strategy" },
    { id: "ema-cross-rsi-filter", name: "EMA Cross + RSI Filter", description: "EMA cross" },
  ],
};

const MOCK_IMPORT_RESULT = {
  strategy: {
    manifest: { id: "strat_01", display_name: "RSI Threshold" },
  },
  fidelity_report: {
    captured: [],
    approximated: [],
    dropped: [],
    cost_model: {
      commission_type: "flat_bps",
      commission_value_bps: 5,
      slippage_model: "fixed_bps",
      slippage_value_bps: 3,
      fill_timing: "next_open",
      note: "",
    },
  },
};

function renderLibrary(fetchImpl: (url: string, init?: RequestInit) => Promise<Response>) {
  vi.spyOn(globalThis, "fetch").mockImplementation(fetchImpl as typeof fetch);
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <StrategiesPineLibraryRoute />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

// ─── tests ──────────────────────────────────────────────────────────────────

describe("StrategiesPineLibraryRoute", () => {
  beforeEach(() => {
    // Reset any mocks before each test.
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  // ── 1. Renders library entries from mocked GET response ───────────────────

  it("renders entry names from the mocked GET response", async () => {
    renderLibrary((url: string) => {
      if (url.includes("pine-library") && !url.includes("/import")) {
        return mockJson(MOCK_LIBRARY);
      }
      return mockJson({});
    });

    // Wait for entries to appear.
    await waitFor(() => {
      expect(screen.getByText("RSI Threshold")).toBeInTheDocument();
    });

    expect(screen.getByText("MA Crossover")).toBeInTheDocument();
    expect(screen.getByText("EMA Cross + RSI Filter")).toBeInTheDocument();
  });

  // ── 2. Empty-library state renders when items is [] ───────────────────────

  it("renders an empty-library message when items is empty", async () => {
    renderLibrary((url: string) => {
      if (url.includes("pine-library")) {
        return mockJson({ items: [] });
      }
      return mockJson({});
    });

    await waitFor(() => {
      expect(screen.getByText(/no library entries available/i)).toBeInTheDocument();
    });
  });

  // ── 3. No overlay element (dialog/modal/sheet/popover) ────────────────────

  it("renders no dialog, modal, sheet or popover overlays", async () => {
    renderLibrary((url: string) => {
      if (url.includes("pine-library")) {
        return mockJson(MOCK_LIBRARY);
      }
      return mockJson({});
    });

    await waitFor(() => {
      expect(screen.getByText("RSI Threshold")).toBeInTheDocument();
    });

    // No dialog/modal/sheet/popover roles.
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    // No element with typical modal class patterns.
    expect(document.querySelector("[data-radix-popper-content-wrapper]")).toBeNull();
  });

  // ── 4. Clicking Import calls the POST endpoint ────────────────────────────

  it("clicking Import button calls the POST endpoint for that entry", async () => {
    // Capture which URLs were called.
    const calledUrls: string[] = [];

    renderLibrary((url: string) => {
      calledUrls.push(url);
      if (url.includes("/import")) {
        return mockJson(MOCK_IMPORT_RESULT);
      }
      return mockJson(MOCK_LIBRARY);
    });

    await waitFor(() => {
      expect(screen.getByText("RSI Threshold")).toBeInTheDocument();
    });

    // Click the first Import button.
    const importBtns = screen.getAllByRole("button", { name: /import/i });
    fireEvent.click(importBtns[0]);

    await waitFor(() => {
      // The POST endpoint must have been called.
      const importCall = calledUrls.find((url) => url.includes("/import"));
      expect(importCall).toBeDefined();
    });
  });
});
