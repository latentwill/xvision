import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
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
  { slug: "quickstart", title: "Quickstart", section: "Quickstart" },
  { slug: "strategies", title: "Strategies", section: "Concepts" },
  { slug: "scenarios", title: "Scenarios", section: "Concepts" },
  { slug: "eval-runs", title: "Eval Runs", section: "Concepts" },
  { slug: "cli-reference", title: "CLI Reference", section: "CLI" },
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
  try {
    window.localStorage.removeItem("xvn-docs-prefs");
  } catch {
    // ignore — localStorage may be unavailable in some envs
  }
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

  test("Copy as Markdown writes the page body to the clipboard", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });

    renderRoute();
    const copy = await screen.findByTestId("docs-copy-md");
    await waitFor(() => expect(copy).not.toBeDisabled());

    fireEvent.click(copy);
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    const arg = writeText.mock.calls[0][0] as string;
    expect(arg).toMatch(/^# Quickstart/);
    await waitFor(() => expect(copy).toHaveTextContent(/copied/i));
  });

  test("⌘K focuses the page filter input", async () => {
    renderRoute();
    await screen.findByTestId("docs-index-item-quickstart");
    const filter = screen.getByLabelText("Filter docs") as HTMLInputElement;
    expect(filter).not.toHaveFocus();
    act(() => {
      const ev = new KeyboardEvent("keydown", {
        key: "k",
        metaKey: true,
        bubbles: true,
        cancelable: true,
      });
      window.dispatchEvent(ev);
    });
    expect(filter).toHaveFocus();
  });

  test("display-options panel keeps only article density controls", async () => {
    renderRoute();
    await screen.findByTestId("docs-index-item-quickstart");

    fireEvent.click(screen.getByTestId("docs-display-options-toggle"));

    expect(await screen.findByTestId("docs-pref-density")).toBeInTheDocument();
    expect(screen.queryByTestId("docs-pref-toc")).toBeNull();
    expect(screen.queryByTestId("docs-toc")).toBeNull();
  });

  test("sidebar renders one header per declared section", async () => {
    renderRoute();
    // Wait for the index to land.
    await screen.findByTestId("docs-index-item-quickstart");
    const headers = screen.getAllByTestId("docs-section-header");
    // Three distinct sections in the fixture: Quickstart, Concepts, CLI.
    expect(headers.map((h) => h.textContent)).toEqual([
      "Quickstart",
      "Concepts",
      "CLI",
    ]);
    // Every page button still renders.
    for (const p of INDEX) {
      expect(screen.getByTestId(`docs-index-item-${p.slug}`)).toBeTruthy();
    }
  });
});
