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

import { StrategiesFolderView } from "./strategies-folder";
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

/**
 * Render `StrategiesFolderView` — the reusable view body that is now
 * mounted inside `/strategies?view=folder`. Tests work identically
 * whether the component is rendered standalone here or via the toggle
 * on the strategies route.
 */
function renderRoute(initialEntry = "/strategies?view=folder") {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <StrategiesFolderView />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

// Topbar reads `useViewportMode()` which calls `window.matchMedia`.
// jsdom doesn't provide it; install a desktop-breakpoint stub.
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

describe("StrategiesFolderView", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    stubMatchMediaDesktop();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders empty state and the file picker", async () => {
    vi.mocked(folderApi.listStrategiesFolder).mockResolvedValue([]);
    renderRoute();
    expect(await screen.findByText(/Nothing here yet/i)).toBeTruthy();
    expect(
      screen.getByTestId("strategies-folder-file-input"),
    ).toBeTruthy();
  });

  it("groups entries by subfolder", async () => {
    vi.mocked(folderApi.listStrategiesFolder).mockResolvedValue([
      {
        rel_path: "notes/hello.md",
        kind: "markdown",
        size_bytes: 42,
        modified_at: "2026-05-21T00:00:00Z",
      },
      {
        rel_path: "docs/manual.pdf",
        kind: "pdf",
        size_bytes: 12345,
        modified_at: "2026-05-21T00:00:01Z",
      },
    ]);
    renderRoute();
    expect(await screen.findByText("notes/hello.md")).toBeTruthy();
    expect(screen.getByText("docs/manual.pdf")).toBeTruthy();
    expect(screen.getByText("Notes")).toBeTruthy();
    expect(screen.getByText("Docs")).toBeTruthy();
  });

  it("calls the importer when a file is selected and reflects success inline", async () => {
    vi.mocked(folderApi.listStrategiesFolder).mockResolvedValue([]);
    vi.mocked(folderApi.importStrategiesFolderFile).mockResolvedValue({
      entry: {
        rel_path: "notes/upload.md",
        kind: "markdown",
        size_bytes: 8,
        modified_at: "2026-05-21T00:00:00Z",
      },
      summary: null,
      findings: [],
    });

    renderRoute();
    const input = (await screen.findByTestId(
      "strategies-folder-file-input",
    )) as HTMLInputElement;
    const file = new File(["# upload\n"], "upload.md", { type: "text/markdown" });
    fireEvent.change(input, { target: { files: [file] } });

    await waitFor(() => {
      expect(folderApi.importStrategiesFolderFile).toHaveBeenCalledWith(file);
    });

    expect(await screen.findByText(/Imported → notes\/upload.md/i)).toBeTruthy();
  });

  it("renders inline error messages on import failure (no popup)", async () => {
    vi.mocked(folderApi.listStrategiesFolder).mockResolvedValue([]);
    vi.mocked(folderApi.importStrategiesFolderFile).mockRejectedValue(
      new Error("type_not_allowed: .exe is not in the importer allowlist"),
    );

    renderRoute();
    const input = (await screen.findByTestId(
      "strategies-folder-file-input",
    )) as HTMLInputElement;
    const file = new File(["MZ"], "nope.exe", {
      type: "application/octet-stream",
    });
    fireEvent.change(input, { target: { files: [file] } });

    expect(await screen.findByText(/type_not_allowed/i)).toBeTruthy();
    // Crucially: no role=dialog should appear (no-popups rule).
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("surfaces findings (e.g. summary_extractor_unavailable) from a successful import", async () => {
    vi.mocked(folderApi.listStrategiesFolder).mockResolvedValue([]);
    vi.mocked(folderApi.importStrategiesFolderFile).mockResolvedValue({
      entry: {
        rel_path: "docs/manual.pdf",
        kind: "pdf",
        size_bytes: 2048,
        modified_at: "2026-05-21T00:00:00Z",
      },
      summary: null,
      findings: [
        {
          code: "summary_extractor_unavailable",
          detail: "pdftotext not on PATH",
        },
      ],
    });

    renderRoute();
    const input = (await screen.findByTestId(
      "strategies-folder-file-input",
    )) as HTMLInputElement;
    const file = new File(["%PDF-1.4"], "manual.pdf", {
      type: "application/pdf",
    });
    fireEvent.change(input, { target: { files: [file] } });

    expect(
      await screen.findByText(/summary_extractor_unavailable/i),
    ).toBeTruthy();
  });
});
