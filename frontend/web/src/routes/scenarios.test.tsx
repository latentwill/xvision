import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { ScenariosRoute } from "./scenarios";
import * as scenariosApi from "@/api/scenarios";
import type { Scenario } from "@/api/types.gen";

vi.mock("@/api/scenarios", async () => {
  const actual = await vi.importActual<typeof import("@/api/scenarios")>(
    "@/api/scenarios",
  );
  return {
    ...actual,
    listScenariosPaged: vi.fn(),
  };
});

function renderRoute(initialEntry = "/scenarios") {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <ScenariosRoute />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

function scenario(overrides: Partial<Scenario> = {}): Scenario {
  return {
    id: "scn-1",
    parent_scenario_id: null,
    source: "User",
    display_name: "BTC 4h sample",
    description: "Sample 4h window",
    tags: ["btc", "trend"],
    notes: null,
    asset_class: "Crypto",
    quote_currency: "Usd",
    time_window: {
      start: "2025-01-01T00:00:00Z",
      end: "2025-01-11T00:00:00Z",
    },
    timezone: "UTC",
    calendar: "Continuous24x7",
    data_source: { type: "AlpacaHistorical", feed: null, adjustment: "Raw" },
    venue: {
      venue: "Alpaca",
      fees: { maker_bps: 0, taker_bps: 0 },
      slippage: { model: "none" },
      latency: { decision_to_fill_ms: 0 },
      fill_model: {
        market_order_fill: "FullAtClose",
        limit_order_fill: "NeverFills",
        partial_fills: false,
        volume_constraints: null,
      },
      overrides: [],
    },
    replay_mode: { mode: "Continuous" },
    capital: { initial: 10000, currency: "USD" },
    bar_cache_policy: {
      cache_key: "scn-1",
      refresh_policy: { policy: "NeverRefresh" },
      data_fetched_at: null,
    },
    warmup_bars: 200,
    created_at: "2025-01-01T00:00:00Z",
    created_by: "test",
    archived_at: null,
    regime_label: null,
    volatility_label: null,
    trend_direction: null,
    regime_derived: false,
    venue_label: "paper",
    safety_limits: null,
    ...overrides,
  } as Scenario;
}

// `<ResponsiveListCard>` reads `useViewportMode()` which calls
// `window.matchMedia`. jsdom doesn't provide it; install a desktop-
// breakpoint stub so the route mounts without the runtime throwing.
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

describe("ScenariosRoute", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    stubMatchMediaDesktop();
    vi.mocked(scenariosApi.listScenariosPaged).mockResolvedValue({
      items: [],
      total: 0,
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("renders an empty state with a CTA to create a scenario", async () => {
    renderRoute();
    await waitFor(() =>
      expect(scenariosApi.listScenariosPaged).toHaveBeenCalled(),
    );
    expect(
      await screen.findByText(/No scenarios yet/i),
    ).toBeInTheDocument();
    expect(
      screen.getAllByRole("link", { name: /New scenario/ }).length,
    ).toBeGreaterThan(0);
  });

  it("renders display_name and source pill in the populated list", async () => {
    vi.mocked(scenariosApi.listScenariosPaged).mockResolvedValue({
      items: [scenario({ display_name: "BTC 4h sample" })],
      total: 1,
    });
    renderRoute();

    await screen.findByText("BTC 4h sample");
    // Source pill rendered as a Pill containing the source label.
    expect(screen.getAllByText("User").length).toBeGreaterThan(0);
    // Market cell shows scenario descriptors; traded assets come from strategies.
    expect(screen.getAllByText(/Crypto \/ Usd/).length).toBeGreaterThan(0);
  });

  it("forwards the include_archived filter to the backend listScenariosPaged call", async () => {
    vi.mocked(scenariosApi.listScenariosPaged).mockResolvedValue({
      items: [scenario()],
      total: 1,
    });
    renderRoute("/scenarios?archived=include");

    await waitFor(() => {
      const calls = vi.mocked(scenariosApi.listScenariosPaged).mock.calls;
      expect(
        calls.some(
          ([f]) => (f as { include_archived?: boolean }).include_archived === true,
        ),
      ).toBe(true);
    });
  });

  it("excludes optimizer scenarios by default and shows them from the Optimizer source folder", async () => {
    renderRoute();

    await waitFor(() => {
      const calls = vi.mocked(scenariosApi.listScenariosPaged).mock.calls;
      expect(
        calls.some(([f]) =>
          (f as { exclude_tags?: string[] }).exclude_tags?.includes(
            "source:autooptimizer",
          ),
        ),
      ).toBe(true);
    });

    // The Source filter is a SignalSelectMenu button (not a native <select>).
    // Click the trigger to open the listbox, then pick "Optimizer".
    const sourceTrigger = await screen.findByRole("button", {
      name: (name) => /Source/i.test(name) || /Any source/i.test(name),
    });
    fireEvent.click(sourceTrigger);

    const optimizerOption = await screen.findByRole("option", {
      name: /^Optimizer$/i,
    });
    fireEvent.click(optimizerOption);

    await waitFor(() => {
      const calls = vi.mocked(scenariosApi.listScenariosPaged).mock.calls;
      expect(
        calls.some(([f]) => {
          const filter = f as { source?: string | null; tags?: string[]; exclude_tags?: string[] };
          // W6 fix: optimizer filter uses tag-only (source: null), not source: "Generated"
          // so that ec-day-* DB rows (source != 'generated') are included.
          return (
            filter.source === null &&
            filter.tags?.includes("source:autooptimizer") &&
            (filter.exclude_tags?.length ?? 0) === 0
          );
        }),
      ).toBe(true);
    });
  });

  it("hydrates the search box from the ?q= URL param", async () => {
    vi.mocked(scenariosApi.listScenariosPaged).mockResolvedValue({
      items: [scenario({ display_name: "BTC 4h sample" })],
      total: 1,
    });
    renderRoute("/scenarios?q=btc");

    const search = (await screen.findByPlaceholderText(
      "Search scenarios…",
    )) as HTMLInputElement;
    await waitFor(() => expect(search.value).toBe("btc"));
  });

  it("filters the in-page rows by the live search query", async () => {
    vi.mocked(scenariosApi.listScenariosPaged).mockResolvedValue({
      items: [
        scenario({ id: "a", display_name: "BTC 4h" }),
        scenario({
          id: "b",
          display_name: "ETH 1h",
        }),
      ],
      total: 2,
    });
    renderRoute();

    await screen.findByText("BTC 4h");
    expect(screen.getAllByText("ETH 1h").length).toBeGreaterThan(0);

    const search = (await screen.findByPlaceholderText(
      "Search scenarios…",
    )) as HTMLInputElement;
    fireEvent.change(search, { target: { value: "btc" } });
    expect(search.value).toBe("btc");
    // Give the URL-writeback effect a tick to settle so the next
    // assertion sees the filtered DOM. The useListUrlState hook writes
    // the search to the URL on the next microtask which can race the
    // first render of the filtered rows.
    await new Promise((r) => setTimeout(r, 50));

    expect(screen.queryByText("ETH 1h")).not.toBeInTheDocument();
    expect(screen.getAllByText("BTC 4h").length).toBeGreaterThan(0);
  });
});
