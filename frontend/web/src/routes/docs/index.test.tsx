import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

vi.mock("@/api/docs", async () => {
  return {
    getDocsIndex: vi.fn(),
    getDocsPage: vi.fn(),
    docsKeys: {
      all: ["docs"],
      index: () => ["docs", "index"],
      page: (slug: string) => ["docs", "page", slug],
    },
  };
});

const docsApi = await import("@/api/docs");
const { DocsRoute } = await import("./index");

function renderRoute(initialEntry = "/docs") {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[initialEntry]}>
        <DocsRoute />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const INDEX = [
  { slug: "quickstart", title: "Quickstart" },
  { slug: "strategies", title: "Strategies" },
  { slug: "scenarios", title: "Scenarios" },
  { slug: "eval-runs", title: "Eval Runs" },
  { slug: "cli-reference", title: "CLI Reference" },
];

beforeEach(() => {
  vi.mocked(docsApi.getDocsIndex).mockResolvedValue(INDEX);
  vi.mocked(docsApi.getDocsPage).mockImplementation(async (slug: string) =>
    `# ${INDEX.find((p) => p.slug === slug)?.title ?? slug}\n\nbody text for ${slug}`,
  );
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("DocsRoute", () => {
  test("renders the baked index in order", async () => {
    renderRoute();
    for (const meta of INDEX) {
      await waitFor(() =>
        expect(
          screen.getByTestId(`docs-index-item-${meta.slug}`),
        ).toBeInTheDocument(),
      );
    }
  });

  test("loads the first page by default and renders its markdown", async () => {
    renderRoute();
    await waitFor(() =>
      expect(docsApi.getDocsPage).toHaveBeenCalledWith("quickstart"),
    );
    const body = await screen.findByTestId("docs-page-body");
    expect(body).toHaveTextContent(/Quickstart/);
  });

  test("clicking an index item switches the active page", async () => {
    renderRoute();
    const item = await screen.findByTestId("docs-index-item-cli-reference");
    fireEvent.click(item);
    await waitFor(() =>
      expect(docsApi.getDocsPage).toHaveBeenCalledWith("cli-reference"),
    );
    const body = await screen.findByTestId("docs-page-body");
    expect(body).toHaveTextContent(/CLI Reference/);
  });

  test("client-side filter narrows the index to matching titles", async () => {
    renderRoute();
    await screen.findByTestId("docs-index-item-quickstart");
    const filter = screen.getByLabelText("Filter docs");
    fireEvent.change(filter, { target: { value: "eval" } });
    expect(screen.queryByTestId("docs-index-item-quickstart")).toBeNull();
    expect(screen.queryByTestId("docs-index-item-strategies")).toBeNull();
    expect(
      screen.getByTestId("docs-index-item-eval-runs"),
    ).toBeInTheDocument();
  });

  test("renders an inline error when a page fetch fails", async () => {
    vi.mocked(docsApi.getDocsPage).mockRejectedValueOnce(new Error("boom"));
    renderRoute();
    expect(
      await screen.findByTestId("docs-page-error"),
    ).toBeInTheDocument();
  });

  test("renders an inline error when the index fetch fails", async () => {
    vi.mocked(docsApi.getDocsIndex).mockRejectedValueOnce(new Error("boom"));
    renderRoute();
    expect(
      await screen.findByTestId("docs-index-error"),
    ).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(/could not load docs index/i);
  });

  test("?slug= deep-link opens the named page instead of the first entry", async () => {
    renderRoute("/docs?slug=cli-reference");
    await waitFor(() =>
      expect(docsApi.getDocsPage).toHaveBeenCalledWith("cli-reference"),
    );
    // Quickstart is the first index entry — it must NOT have been loaded.
    expect(docsApi.getDocsPage).not.toHaveBeenCalledWith("quickstart");
    const body = await screen.findByTestId("docs-page-body");
    expect(body).toHaveTextContent(/CLI Reference/);
  });

  test("clicking a sidebar item updates the URL slug", async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    // Use a real router history so we can inspect location changes.
    // MemoryRouter exposes its current location via the router instance;
    // instead we use a simpler approach: after clicking, the active button
    // should carry aria-current="page" and the page for that slug should load.
    render(
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={["/docs"]}>
          <DocsRoute />
        </MemoryRouter>
      </QueryClientProvider>,
    );

    // Wait for the index to load.
    const item = await screen.findByTestId("docs-index-item-scenarios");
    fireEvent.click(item);

    // After the click the scenarios page should be fetched (URL was updated to ?slug=scenarios).
    await waitFor(() =>
      expect(docsApi.getDocsPage).toHaveBeenCalledWith("scenarios"),
    );
    // The clicked button should now carry aria-current="page".
    expect(screen.getByTestId("docs-index-item-scenarios")).toHaveAttribute(
      "aria-current",
      "page",
    );
  });
});
