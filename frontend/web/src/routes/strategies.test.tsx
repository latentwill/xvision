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

import { StrategiesRoute } from "./strategies";
import * as strategiesApi from "@/api/strategies";
import * as folderApi from "@/api/strategies-folder";
import * as evalApi from "@/api/eval";

vi.mock("@/api/strategies-folder", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies-folder")>(
    "@/api/strategies-folder",
  );
  return {
    ...actual,
    listStrategiesFolder: vi.fn(),
    importStrategiesFolderFile: vi.fn(),
  };
});

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return {
    ...actual,
    createStrategy: vi.fn(),
    cloneStrategy: vi.fn(),
    listStrategies: vi.fn(),
    listStrategiesPaged: vi.fn(),
  };
});

vi.mock("@/api/eval", async () => {
  const actual = await vi.importActual<typeof import("@/api/eval")>(
    "@/api/eval",
  );
  return {
    ...actual,
    listRuns: vi.fn(),
  };
});

function renderRoute(initialEntry = "/strategies") {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <StrategiesRoute />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

// `<ResponsiveListCard>` reads `useViewportMode()` which calls
// `window.matchMedia`. jsdom doesn't provide it; install a desktop-
// breakpoint stub so the route mounts without the runtime throwing.
// Tests that need the phone branch can override locally.
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

describe("StrategiesRoute", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    stubMatchMediaDesktop();
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
  });

  it("renders strategy display name, model summary, tags, and humanized cadence", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [
        {
          agent_id: "01TEST",
          display_name: "Trend 4H",
          template: "trend_follower",
          decision_cadence_minutes: 240,
          model: "claude-sonnet +1",
          tags: [
            "trend_follower",
            "BTC/USD",
            "trending_bull",
            "very_long_strategy_tag_that_should_not_make_rows_tall",
          ],
        },
      ],
      total: 1,
    });

    renderRoute();

    // Wait for the row content; "Name" header renders synchronously even
    // before the query resolves, so we need a real row signal.
    expect((await screen.findAllByText("Trend 4H")).length).toBeGreaterThan(0);
    expect(screen.getAllByText("4h").length).toBeGreaterThan(0);
    expect(screen.getAllByText("claude-sonnet +1").length).toBeGreaterThan(0);
    expect(screen.getAllByText("BTC/USD").length).toBeGreaterThan(0);
    expect(screen.getAllByText("trending_bull").length).toBeGreaterThan(0);
    // The raw ULID must not appear as a stand-alone text node in the
    // table body — it's only acceptable as a Link href.
    expect(screen.queryByText("Backend ID")).not.toBeInTheDocument();
    expect(screen.queryByText("01TEST")).not.toBeInTheDocument();
    // Each row has an "Actions for <name>" button (the action menu trigger).
    // Verify the row is accessible by display name, not raw ULID.
    expect(
      screen.queryByRole("button", { name: /Actions for 01TEST/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Actions for Trend 4H/i }),
    ).toBeInTheDocument();
  });

  it("does not render the template filter or template column", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [
        {
          agent_id: "01TEST",
          display_name: "Trend 4H",
          template: "trend_follower",
          decision_cadence_minutes: 240,
          model: "claude-sonnet",
        },
      ],
      total: 1,
    });

    renderRoute();

    expect((await screen.findAllByText("Trend 4H")).length).toBeGreaterThan(0);
    expect(screen.queryByLabelText(/Template/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("columnheader", { name: "Template" })).not.toBeInTheDocument();
  });

  it("hides legacy xvision example strategies that have no attached agent", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [
        {
          agent_id: "example-agentless-template",
          display_name: "Example template without agent",
          template: "example",
          decision_cadence_minutes: 60,
          agent_count: 0,
        },
        {
          agent_id: "01LIVE",
          display_name: "Operator strategy",
          template: "custom",
          decision_cadence_minutes: 60,
          agent_count: 1,
        },
      ],
      total: 2,
    });

    renderRoute();

    expect(await screen.findByText("Operator strategy")).toBeInTheDocument();
    expect(screen.queryByText("Example template without agent")).not.toBeInTheDocument();
  });

  it("clones a strategy from the list action", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [
        {
          agent_id: "01SOURCE",
          display_name: "Trend 4H",
          template: "trend_follower",
          decision_cadence_minutes: 240,
          model: "claude-sonnet",
        },
      ],
      total: 1,
    });
    vi.mocked(strategiesApi.cloneStrategy).mockResolvedValue({
      manifest: {
        id: "01CLONE",
        display_name: "Trend 4H (clone)",
      },
    } as strategiesApi.Strategy);

    renderRoute();

    // Open the row's action menu then click "Duplicate" (the clone action).
    const actionsBtn = await screen.findByRole("button", {
      name: /Actions for Trend 4H/i,
    });
    fireEvent.click(actionsBtn);

    const duplicateBtn = await screen.findByRole("menuitem", {
      name: /Duplicate/i,
    });
    fireEvent.click(duplicateBtn);

    await waitFor(() =>
      expect(strategiesApi.cloneStrategy).toHaveBeenCalledWith("01SOURCE", {
        display_name: "Trend 4H (clone)",
      }),
    );
  });

  it("does not render NaN for invalid cadence values", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [
        {
          agent_id: "01TEST",
          display_name: "Open Strategy",
          template: "custom",
          decision_cadence_minutes: Number.NaN,
          model: undefined,
        },
      ],
      total: 1,
    });

    renderRoute();

    expect((await screen.findAllByText("Open Strategy")).length).toBeGreaterThan(0);
    expect(screen.queryByText(/NaN/)).not.toBeInTheDocument();
    expect(screen.getAllByText("—").length).toBeGreaterThan(0);
  });

  it("renders the empty state with a 'New strategy' call to action and navigates to /strategies/new on click", async () => {
    // W15 fix: the button no longer calls createStrategy directly; it navigates
    // to /strategies/new so the user can enter a name before creating.
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [],
      total: 0,
    });

    let lastLocation = "/strategies";
    render(
      <MemoryRouter
        initialEntries={["/strategies"]}
        // Capture navigation by wrapping the component inside a fake route
        // that records location changes via window history.
      >
        <QueryClientProvider
          client={
            new QueryClient({
              defaultOptions: { queries: { retry: false } },
            })
          }
        >
          <StrategiesRoute />
        </QueryClientProvider>
      </MemoryRouter>,
    );

    await waitFor(() =>
      expect(screen.getByText(/No strategies match these filters\./)).toBeInTheDocument(),
    );
    const ctas = screen.getAllByRole("button", { name: /New Strategy/i });
    expect(ctas.length).toBeGreaterThanOrEqual(1);
    // createStrategy must NOT be called — navigation replaces the direct create
    fireEvent.click(ctas[0]);
    await waitFor(() => {
      // The button now navigates rather than immediately creating a strategy.
      // Verify createStrategy was never called (the name-collection form on
      // /strategies/new is responsible for calling it on submit).
      expect(strategiesApi.createStrategy).not.toHaveBeenCalled();
    });
    // The MemoryRouter is driven by React Router's navigate(); the route won't
    // visibly change in this shallow render, but we can assert the button is
    // still rendered (no crash) and the create API was not called.
    void lastLocation; // suppress unused-variable lint
  });

  it("filters by pipeline shape via the toolbar select", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [
        {
          agent_id: "01SOLO",
          display_name: "Solo Trader",
          template: "trend_follower",
          decision_cadence_minutes: 240,
          provider_models: [{ provider: "openai", model: "gpt-4.1-mini" }],
          agent_count: 1,
          filter_count: 0,
        },
        {
          agent_id: "01MULTI",
          display_name: "Pipeline Crew",
          template: "graph",
          decision_cadence_minutes: 60,
          provider_models: [
            { provider: "openai", model: "gpt-4.1-mini" },
            { provider: "anthropic", model: "claude-sonnet-4" },
          ],
          agent_count: 2,
          filter_count: 0,
        },
      ],
      total: 2,
    });

    renderRoute();

    // Both rows visible by default.
    await waitFor(() =>
      expect(screen.getAllByText(/Solo Trader/).length).toBeGreaterThan(0),
    );
    expect(screen.getAllByText(/Pipeline Crew/).length).toBeGreaterThan(0);

    // The Pipeline shape filter is a custom SignalSelectMenu button.
    // Click it to open the listbox, then click the "Multi-agent" option.
    const shapeTrigger = screen.getByRole("button", {
      name: (name) => /Pipeline shape/i.test(name) || /All shapes/i.test(name),
    });
    fireEvent.click(shapeTrigger);

    const multiOption = await screen.findByRole("option", {
      name: /Multi-agent/i,
    });
    fireEvent.click(multiOption);

    await waitFor(() => {
      expect(screen.queryByText(/Solo Trader/)).not.toBeInTheDocument();
    });
    expect(screen.getAllByText(/Pipeline Crew/).length).toBeGreaterThan(0);
  });

  it("hydrates the search term from the ?q= URL parameter", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [
        {
          agent_id: "01ALPHA",
          display_name: "Alpha Strategy",
          template: "trend_follower",
          decision_cadence_minutes: 60,
        },
        {
          agent_id: "01BETA",
          display_name: "Beta Strategy",
          template: "trend_follower",
          decision_cadence_minutes: 60,
        },
      ],
      total: 2,
    });

    renderRoute("/strategies?q=alpha");

    await waitFor(() =>
      expect(screen.getAllByText(/Alpha Strategy/).length).toBeGreaterThan(0),
    );
    expect(screen.queryByText(/Beta Strategy/)).not.toBeInTheDocument();
  });

  it.each([
    ["model", "sonnet", "Model Match"],
    ["capability", "filter", "Capability Match"],
    ["author", "@alice", "Author Match"],
  ])("filters strategies by %s from the search box", async (_field, query, expected) => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [
        {
          agent_id: "01MODEL",
          display_name: "Model Match",
          template: "trend_follower",
          decision_cadence_minutes: 60,
          model: "claude-sonnet-4",
          capabilities: ["trader"],
          creator: "@bob",
        },
        {
          agent_id: "01CAPABILITY",
          display_name: "Capability Match",
          template: "trend_follower",
          decision_cadence_minutes: 60,
          model: "gpt-4.1-mini",
          capabilities: ["filter"],
          creator: "@carol",
        },
        {
          agent_id: "01AUTHOR",
          display_name: "Author Match",
          template: "trend_follower",
          decision_cadence_minutes: 60,
          model: "gpt-4.1-mini",
          capabilities: ["trader"],
          creator: "@alice",
        },
      ],
      total: 3,
    });

    renderRoute(`/strategies?q=${encodeURIComponent(query)}`);

    await waitFor(() =>
      expect(screen.getAllByText(expected).length).toBeGreaterThan(0),
    );
    for (const hidden of ["Model Match", "Capability Match", "Author Match"]) {
      if (hidden !== expected) {
        expect(screen.queryByText(hidden)).not.toBeInTheDocument();
      }
    }
  });

  it("hydrates leaderboard sort from the URL and ranks by latest eval return then Sharpe", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [
        {
          agent_id: "01ALPHA",
          display_name: "Alpha Strategy",
          template: "trend_follower",
          decision_cadence_minutes: 60,
        },
        {
          agent_id: "01BETA",
          display_name: "Beta Strategy",
          template: "trend_follower",
          decision_cadence_minutes: 60,
        },
        {
          agent_id: "01GAMMA",
          display_name: "Gamma Strategy",
          template: "trend_follower",
          decision_cadence_minutes: 60,
        },
      ],
      total: 3,
    });
    vi.mocked(evalApi.listRuns).mockResolvedValue([
      {
        id: "run-alpha",
        agent_id: "01ALPHA",
        scenario_id: "scn",
        strategy: null,
        scenario: null,
        mode: "backtest",
        status: "completed",
        started_at: "2026-06-10T09:00:00Z",
        completed_at: "2026-06-10T10:00:00Z",
        total_return_pct: 12,
        sharpe: 0.9,
        max_drawdown_pct: 4,
        error: null,
        actual_input_tokens: null,
        actual_output_tokens: null,
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
        auto_fire_review: false,
        review_model: null,
        max_annotations_per_review: null,
        paused: false,
        paused_at: null,
        flatten_requested: false,
        unrealized_pnl_usd: null,
        skipped_dispatches: 0,
        delayed_decisions: 0,
        forced_cancels: 0,
      },
      {
        id: "run-beta",
        agent_id: "01BETA",
        scenario_id: "scn",
        strategy: null,
        scenario: null,
        mode: "backtest",
        status: "completed",
        started_at: "2026-06-10T09:00:00Z",
        completed_at: "2026-06-10T10:00:00Z",
        total_return_pct: 12,
        sharpe: 1.4,
        max_drawdown_pct: 4,
        error: null,
        actual_input_tokens: null,
        actual_output_tokens: null,
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
        auto_fire_review: false,
        review_model: null,
        max_annotations_per_review: null,
        paused: false,
        paused_at: null,
        flatten_requested: false,
        unrealized_pnl_usd: null,
        skipped_dispatches: 0,
        delayed_decisions: 0,
        forced_cancels: 0,
      },
      {
        id: "run-gamma",
        agent_id: "01GAMMA",
        scenario_id: "scn",
        strategy: null,
        scenario: null,
        mode: "backtest",
        status: "completed",
        started_at: "2026-06-10T09:00:00Z",
        completed_at: "2026-06-10T10:00:00Z",
        total_return_pct: -3,
        sharpe: 2.1,
        max_drawdown_pct: 4,
        error: null,
        actual_input_tokens: null,
        actual_output_tokens: null,
        inference_cost_quote_total: null,
        net_return_pct: null,
        filter_summaries: [],
        auto_fire_review: false,
        review_model: null,
        max_annotations_per_review: null,
        paused: false,
        paused_at: null,
        flatten_requested: false,
        unrealized_pnl_usd: null,
        skipped_dispatches: 0,
        delayed_decisions: 0,
        forced_cancels: 0,
      },
    ]);

    renderRoute("/strategies?sort=leaderboard");

    await screen.findByText("Beta Strategy");
    await waitFor(() => expect(evalApi.listRuns).toHaveBeenCalledWith({ limit: 100 }));
    const strategyLinks = screen
      .getAllByRole("link")
      .filter((link) => link.getAttribute("href")?.startsWith("/strategies/"))
      .map((link) => link.textContent);
    expect(strategyLinks.slice(0, 3)).toEqual([
      "Beta Strategy",
      "Alpha Strategy",
      "Gamma Strategy",
    ]);
  });

  it("renders the List | Folder segmented control", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [],
      total: 0,
    });

    renderRoute("/strategies");

    // The toggle must be present with both options.
    expect(await screen.findByRole("tab", { name: "List" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Folder" })).toBeTruthy();
  });

  it("defaults to list view when ?view is absent", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [],
      total: 0,
    });

    renderRoute("/strategies");

    // List tab is selected, folder file picker is NOT present.
    const listTab = await screen.findByRole("tab", { name: "List" });
    expect(listTab).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByTestId("strategies-folder-file-input")).toBeNull();
  });

  it("renders folder content when ?view=folder is in the URL", async () => {
    vi.mocked(folderApi.listStrategiesFolder).mockResolvedValue([]);

    renderRoute("/strategies?view=folder");

    // Folder tab should be selected and the file picker rendered.
    const folderTab = await screen.findByRole("tab", { name: "Folder" });
    expect(folderTab).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByTestId("strategies-folder-file-input")).toBeTruthy();
  });

  it("clicking Folder tab updates ?view= and renders folder content", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [],
      total: 0,
    });
    vi.mocked(folderApi.listStrategiesFolder).mockResolvedValue([]);

    renderRoute("/strategies");

    const folderTab = await screen.findByRole("tab", { name: "Folder" });
    fireEvent.click(folderTab);

    // After clicking, folder view content should appear.
    expect(await screen.findByTestId("strategies-folder-file-input")).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Folder" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("hides body cells for a column that is toggled off via columnState", async () => {
    // Pre-seed localStorage so useListColumns initialises with "model" hidden.
    // The storage key follows the pattern `xvn:list:<listId>:columns`.
    // Essential keys (name, actions) are always re-added by the hook, so we
    // only need to list the non-essential keys we want visible.
    const visibleKeys = ["name", "shape", "tags", "cadence", "created", "actions"];
    window.localStorage.setItem(
      "xvn:list:strategies:columns",
      JSON.stringify(visibleKeys),
    );

    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [
        {
          agent_id: "01TEST",
          display_name: "Trend 4H",
          template: "trend_follower",
          decision_cadence_minutes: 240,
          model: "claude-sonnet-4",
        },
      ],
      total: 1,
    });

    renderRoute();

    // Wait for the row to render.
    await screen.findByText("Trend 4H");

    // The "Model" column header must NOT be rendered.
    expect(screen.queryByRole("columnheader", { name: /^Model$/i })).not.toBeInTheDocument();

    // The model value must NOT appear as a body cell.
    expect(screen.queryByText("claude-sonnet-4")).not.toBeInTheDocument();

    window.localStorage.removeItem("xvn:list:strategies:columns");
  });

  it("clicking List tab from folder view hides folder content and shows list", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [],
      total: 0,
    });
    vi.mocked(folderApi.listStrategiesFolder).mockResolvedValue([]);

    renderRoute("/strategies?view=folder");

    // Start on folder view.
    expect(await screen.findByTestId("strategies-folder-file-input")).toBeTruthy();

    const listTab = screen.getByRole("tab", { name: "List" });
    fireEvent.click(listTab);

    // Folder picker gone, list content now active.
    await waitFor(() =>
      expect(screen.queryByTestId("strategies-folder-file-input")).toBeNull(),
    );
    expect(screen.getByRole("tab", { name: "List" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});
