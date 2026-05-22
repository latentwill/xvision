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
    listStrategies: vi.fn(),
    listStrategiesPaged: vi.fn(),
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
    expect(
      screen.queryByLabelText("Open inspector for 01TEST"),
    ).not.toBeInTheDocument();
    expect(
      screen.getAllByLabelText("Open inspector for Trend 4H").length,
    ).toBeGreaterThan(0);
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

  it("renders the empty state with a 'New strategy' call to action when the engine returns zero rows", async () => {
    vi.mocked(strategiesApi.listStrategiesPaged).mockResolvedValue({
      items: [],
      total: 0,
    });
    vi.mocked(strategiesApi.createStrategy).mockResolvedValue({ id: "st_new" });

    renderRoute();

    await waitFor(() =>
      expect(screen.getByText(/No strategies match these filters\./)).toBeInTheDocument(),
    );
    const ctas = screen.getAllByRole("button", { name: /New Strategy/i });
    expect(ctas.length).toBeGreaterThanOrEqual(1);
    fireEvent.click(ctas[0]);
    await waitFor(() => {
      expect(strategiesApi.createStrategy).toHaveBeenCalledWith({
        name: "Untitled strategy",
        creator: null,
      });
    });
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

    const shapeSelect = screen.getByLabelText(
      /Pipeline shape/i,
    ) as HTMLSelectElement;
    fireEvent.change(shapeSelect, { target: { value: "multi" } });

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
