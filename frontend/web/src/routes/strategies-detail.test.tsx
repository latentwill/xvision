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

    if (url.includes("/diagnostics") && method === "GET") {
      // StrategyReadinessPanel fetches /api/strategy/:id/diagnostics.
      // Return a minimal launchable diagnostics payload so the panel renders
      // without crashing. The color-picker tests don't assert on readiness
      // content — they only need the panel to mount successfully.
      const diagnostics = {
        strategy_id: state.manifest.id,
        per_agent: [],
        unregistered_tools: [],
        has_decision_path: true,
        launchable: true,
      };
      return new Response(JSON.stringify(diagnostics), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
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

// ───────────────────────────────────────────────────────────────────────────
// TunableBoundsPanel — auto-generated settings strip
// ───────────────────────────────────────────────────────────────────────────

type TunableBound = {
  path: string;
  min: number | null;
  max: number | null;
  step: number | null;
  kind: "int" | "float" | "bool";
};

type ManifestWithBounds = ManifestState & {
  tunable_bounds?: TunableBound[];
};

function buildFetchMockWithBounds(state: {
  manifest: ManifestWithBounds;
  tunable_bounds?: TunableBound[];
}) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = init?.method ?? "GET";

    if (url.includes("/diagnostics") && method === "GET") {
      const diagnostics = {
        strategy_id: state.manifest.id,
        per_agent: [],
        unregistered_tools: [],
        has_decision_path: true,
        launchable: true,
      };
      return new Response(JSON.stringify(diagnostics), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (url.startsWith("/api/strategy/") && method === "GET") {
      return new Response(JSON.stringify(state), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (url.startsWith("/api/strategy/") && method === "PATCH") {
      const body = init?.body
        ? (JSON.parse(init.body as string) as Partial<ManifestState>)
        : {};
      if (typeof body.display_name === "string") {
        state.manifest.display_name = body.display_name;
      }
      if (typeof body.plain_summary === "string") {
        state.manifest.plain_summary = body.plain_summary;
      }
      if ("color" in body) {
        const c = body.color as string | null | undefined;
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

const STRATEGY_ID_BOUNDS = "01J0TUNABLEBOUNDSTEST000001";

const TWO_BOUNDS: TunableBound[] = [
  {
    path: "conditions.0.rhs.numeric",
    min: 2,
    max: 50,
    step: 1,
    kind: "int",
  },
  {
    path: "mechanistic.close_policies.0.pct",
    min: 0.5,
    max: 10,
    step: 0.5,
    kind: "float",
  },
];

function renderDetailWithBounds(bounds?: TunableBound[]) {
  const stateWithBounds = {
    manifest: {
      id: STRATEGY_ID_BOUNDS,
      display_name: "Bounds Strategy",
      plain_summary: "Has tunable bounds",
      creator: "@op",
      template: "mechanistic",
      asset_universe: ["BTC/USD"],
      decision_cadence_minutes: 60,
      color: null,
    },
    tunable_bounds: bounds,
  };
  const fetchMock = buildFetchMockWithBounds(stateWithBounds);
  vi.stubGlobal("fetch", fetchMock);

  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[`/strategies/${STRATEGY_ID_BOUNDS}`]}>
        <Routes>
          <Route path="/strategies/:id" element={<StrategyDetailRoute />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

// ───────────────────────────────────────────────────────────────────────────
// Clone strategy button
// ───────────────────────────────────────────────────────────────────────────

const CLONE_SOURCE_ID = "01J0CLONESOURCE000000000001";
const CLONE_RESULT_ID = "01J0CLONERESULT000000000001";

function buildCloneFetchMock(state: { manifest: ManifestState }) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = init?.method ?? "GET";

    if (url.includes("/diagnostics") && method === "GET") {
      const diagnostics = {
        strategy_id: state.manifest.id,
        per_agent: [],
        unregistered_tools: [],
        has_decision_path: true,
        launchable: true,
      };
      return new Response(JSON.stringify(diagnostics), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    // POST /api/strategy/:id/clone → 201 Created with the new Strategy.
    if (url.includes("/clone") && method === "POST") {
      const body = init?.body
        ? (JSON.parse(init.body as string) as { display_name?: string })
        : {};
      const cloned = {
        manifest: {
          ...state.manifest,
          id: CLONE_RESULT_ID,
          display_name:
            body.display_name ?? `${state.manifest.display_name} (clone)`,
        },
      };
      return new Response(JSON.stringify(cloned), {
        status: 201,
        headers: { "content-type": "application/json" },
      });
    }
    if (url.startsWith("/api/strategy/") && method === "GET") {
      // Serve the source for the source id; serve the cloned payload once the
      // route navigates to the new id.
      const payload = url.includes(CLONE_RESULT_ID)
        ? {
            manifest: {
              ...state.manifest,
              id: CLONE_RESULT_ID,
              display_name: `${state.manifest.display_name} (clone)`,
            },
          }
        : state;
      return new Response(JSON.stringify(payload), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    return new Response("not implemented", { status: 501 });
  });
}

function renderCloneDetail() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[`/strategies/${CLONE_SOURCE_ID}`]}>
        <Routes>
          <Route path="/strategies/:id" element={<StrategyDetailRoute />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("StrategyDetailRoute — clone button", () => {
  let state: { manifest: ManifestState };
  let fetchMock: ReturnType<typeof buildCloneFetchMock>;

  beforeEach(() => {
    state = {
      manifest: {
        id: CLONE_SOURCE_ID,
        display_name: "Test Strategy",
        plain_summary: "A test",
        creator: "@op",
        template: "trend_follower",
        asset_universe: ["BTC/USD"],
        decision_cadence_minutes: 60,
        color: null,
      },
    };
    fetchMock = buildCloneFetchMock(state);
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("renders the clone button after the strategy loads", async () => {
    renderCloneDetail();
    await waitFor(() =>
      expect(screen.getByTestId("strategy-detail-clone")).toBeInTheDocument(),
    );
    expect(
      screen.getByRole("button", { name: /clone strategy/i }),
    ).toBeInTheDocument();
  });

  it("clicking clone POSTs to /clone with the '(clone)' display name", async () => {
    renderCloneDetail();
    await waitFor(() =>
      expect(screen.getByTestId("strategy-detail-clone")).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByTestId("strategy-detail-clone"));

    await waitFor(() => {
      const cloneCalls = fetchMock.mock.calls.filter(
        ([url, init]) =>
          (typeof url === "string" ? url : url.toString()).includes(
            `/api/strategy/${CLONE_SOURCE_ID}/clone`,
          ) && init?.method === "POST",
      );
      expect(cloneCalls.length).toBe(1);
      const body = JSON.parse(cloneCalls[0][1]!.body as string) as {
        display_name: string;
      };
      expect(body.display_name).toBe("Test Strategy (clone)");
    });
  });

  it("navigates to the cloned strategy on success", async () => {
    renderCloneDetail();
    await waitFor(() =>
      expect(screen.getByTestId("strategy-detail-clone")).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByTestId("strategy-detail-clone"));

    // The same route renders the new id after navigation; the detail view's
    // data-strategy-id should flip to the clone's id.
    await waitFor(() =>
      expect(screen.getByTestId("strategy-detail-view")).toHaveAttribute(
        "data-strategy-id",
        CLONE_RESULT_ID,
      ),
    );
  });

  it("does not render any dialog / modal / overlay for the clone action", async () => {
    renderCloneDetail();
    await waitFor(() =>
      expect(screen.getByTestId("strategy-detail-clone")).toBeInTheDocument(),
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });
});

describe("StrategyDetailRoute — TunableBoundsPanel", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("renders one row per tunable bound with a human label and range", async () => {
    renderDetailWithBounds(TWO_BOUNDS);

    await waitFor(() =>
      expect(
        screen.getByTestId("tunable-bounds-panel"),
      ).toBeInTheDocument(),
    );

    // Two bound rows
    const rows = screen.getAllByTestId("tunable-bound-row");
    expect(rows).toHaveLength(2);

    // First row: path "conditions.0.rhs.numeric" → label "Condition 1 threshold"
    // and range min=2 max=50
    expect(rows[0]).toHaveTextContent("Condition 1 threshold");
    expect(rows[0]).toHaveTextContent("2");
    expect(rows[0]).toHaveTextContent("50");

    // Second row: path "mechanistic.close_policies.0.pct" → label "Stop/target %"
    // and range min=0.5 max=10
    expect(rows[1]).toHaveTextContent("Stop/target %");
    expect(rows[1]).toHaveTextContent("0.5");
    expect(rows[1]).toHaveTextContent("10");
  });

  it("renders NO panel element when tunable_bounds is empty", async () => {
    renderDetailWithBounds([]);

    // Wait for the strategy to load (color picker is a reliable sentinel)
    await waitFor(() =>
      expect(screen.getByTestId("color-picker-row")).toBeInTheDocument(),
    );

    expect(
      screen.queryByTestId("tunable-bounds-panel"),
    ).not.toBeInTheDocument();
  });

  it("renders NO panel element when tunable_bounds is absent", async () => {
    renderDetailWithBounds(undefined);

    await waitFor(() =>
      expect(screen.getByTestId("color-picker-row")).toBeInTheDocument(),
    );

    expect(
      screen.queryByTestId("tunable-bounds-panel"),
    ).not.toBeInTheDocument();
  });

  it("does not render any dialog / modal / overlay for the bounds panel", async () => {
    renderDetailWithBounds(TWO_BOUNDS);

    await waitFor(() =>
      expect(screen.getByTestId("tunable-bounds-panel")).toBeInTheDocument(),
    );

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("renders the kind badge for each bound", async () => {
    renderDetailWithBounds(TWO_BOUNDS);

    await waitFor(() =>
      expect(screen.getByTestId("tunable-bounds-panel")).toBeInTheDocument(),
    );

    const rows = screen.getAllByTestId("tunable-bound-row");
    expect(rows[0]).toHaveTextContent("int");
    expect(rows[1]).toHaveTextContent("float");
  });
});
