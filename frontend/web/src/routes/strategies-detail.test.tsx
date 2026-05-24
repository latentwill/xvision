/**
 * Tests for the color picker affordance on StrategyDetailRoute.
 *
 * Coverage:
 * - Color picker row is rendered when the strategy loads.
 * - Clicking a rotation swatch fires PATCH with `{ color: "#XXXXXX" }`.
 * - Clicking the unset swatch fires PATCH with `{ color: "" }`.
 * - The active swatch shows a checkmark (aria-pressed=true).
 * - A "saved" indicator appears after successful patch.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { StrategyDetailRoute } from "./strategies-detail";
import { CHART2_STRATEGY_ROTATION } from "@/theme/themes";

const STRATEGY_ID = "01J0COLORPICKERTEST00000001";

// The first rotation color from the palette — used as a stable reference
// in most tests.
const FIRST_COLOR = CHART2_STRATEGY_ROTATION[0].color; // "#D4A547"
const SECOND_COLOR = CHART2_STRATEGY_ROTATION[1].color; // "#E8DCB0"

type ManifestState = {
  id: string;
  display_name: string;
  plain_summary: string;
  creator: string;
  template: string;
  asset_universe: string[];
  decision_cadence_minutes: number;
  color?: string | null;
};

function buildFetchMock(state: { manifest: ManifestState }) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = init?.method ?? "GET";

    if (url.startsWith("/api/strategy/") && method === "GET") {
      return new Response(JSON.stringify(state), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (url.startsWith("/api/strategy/") && method === "PATCH") {
      const body = init?.body ? (JSON.parse(init.body as string) as Partial<ManifestState>) : {};
      // Apply only fields present in the patch body (mimic backend partial patch).
      if (typeof body.display_name === "string") {
        state.manifest.display_name = body.display_name;
      }
      if (typeof body.plain_summary === "string") {
        state.manifest.plain_summary = body.plain_summary;
      }
      if ("color" in body) {
        const c = body.color as string | null | undefined;
        // mirror backend: "" → null (clear)
        state.manifest.color = c === "" ? null : c ?? state.manifest.color;
      }
      return new Response(JSON.stringify(state), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    return new Response("not implemented", { status: 501 });
  });
}

function renderDetail() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[`/strategies/${STRATEGY_ID}`]}>
        <Routes>
          <Route path="/strategies/:id" element={<StrategyDetailRoute />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("StrategyDetailRoute — color picker", () => {
  let state: { manifest: ManifestState };
  let fetchMock: ReturnType<typeof buildFetchMock>;

  beforeEach(() => {
    state = {
      manifest: {
        id: STRATEGY_ID,
        display_name: "Test Strategy",
        plain_summary: "A test",
        creator: "@op",
        template: "trend_follower",
        asset_universe: ["BTC/USD"],
        decision_cadence_minutes: 60,
        color: null,
      },
    };
    fetchMock = buildFetchMock(state);
    vi.stubGlobal("fetch", fetchMock);
  });

  it("renders the color picker row after the strategy loads", async () => {
    renderDetail();
    await waitFor(() =>
      expect(screen.getByTestId("color-picker-row")).toBeInTheDocument(),
    );
    // All 8 rotation swatches should be present.
    for (const entry of CHART2_STRATEGY_ROTATION) {
      expect(screen.getByTestId(`color-swatch-${entry.color}`)).toBeInTheDocument();
    }
    // The unset chip should also be present.
    expect(screen.getByTestId("color-swatch-unset")).toBeInTheDocument();
    // The custom picker should be present.
    expect(screen.getByTestId("color-picker-custom")).toBeInTheDocument();
  });

  it("clicking a swatch fires PATCH with the correct hex body", async () => {
    renderDetail();
    await waitFor(() =>
      expect(screen.getByTestId(`color-swatch-${FIRST_COLOR}`)).toBeInTheDocument(),
    );

    const swatch = screen.getByTestId(`color-swatch-${FIRST_COLOR}`);
    fireEvent.click(swatch);

    await waitFor(() => {
      const patchCalls = fetchMock.mock.calls.filter(
        ([, init]) => init?.method === "PATCH",
      );
      expect(patchCalls.length).toBeGreaterThan(0);
      const lastPatch = patchCalls[patchCalls.length - 1];
      const body = JSON.parse(lastPatch[1]!.body as string) as { color: string };
      expect(body.color).toBe(FIRST_COLOR);
    });
  });

  it("clicking the unset chip fires PATCH with color: empty string", async () => {
    // Start with a color set so the unset is meaningful.
    state.manifest.color = SECOND_COLOR;
    renderDetail();
    await waitFor(() =>
      expect(screen.getByTestId("color-swatch-unset")).toBeInTheDocument(),
    );

    const unset = screen.getByTestId("color-swatch-unset");
    fireEvent.click(unset);

    await waitFor(() => {
      const patchCalls = fetchMock.mock.calls.filter(
        ([, init]) => init?.method === "PATCH",
      );
      expect(patchCalls.length).toBeGreaterThan(0);
      const lastPatch = patchCalls[patchCalls.length - 1];
      const body = JSON.parse(lastPatch[1]!.body as string) as { color: string };
      expect(body.color).toBe("");
    });
  });

  it("shows aria-pressed=true on the active swatch", async () => {
    // Set initial color to FIRST_COLOR so the swatch starts active.
    state.manifest.color = FIRST_COLOR;
    renderDetail();

    await waitFor(() => {
      const swatch = screen.getByTestId(`color-swatch-${FIRST_COLOR}`);
      expect(swatch).toHaveAttribute("aria-pressed", "true");
    });

    // Other swatches should not be pressed.
    const secondSwatch = screen.getByTestId(`color-swatch-${SECOND_COLOR}`);
    expect(secondSwatch).toHaveAttribute("aria-pressed", "false");
  });

  it("shows saved indicator after a successful swatch click", async () => {
    renderDetail();
    await waitFor(() =>
      expect(screen.getByTestId(`color-swatch-${FIRST_COLOR}`)).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByTestId(`color-swatch-${FIRST_COLOR}`));

    await waitFor(() =>
      expect(screen.getByTestId("color-saved-indicator")).toBeInTheDocument(),
    );
  });

  it("shows no saved indicator before any interaction", async () => {
    renderDetail();
    await waitFor(() =>
      expect(screen.getByTestId("color-picker-row")).toBeInTheDocument(),
    );
    expect(screen.queryByTestId("color-saved-indicator")).not.toBeInTheDocument();
  });
});
