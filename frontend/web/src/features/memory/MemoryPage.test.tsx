// MemoryPage — workspace-level memory surface scoped to namespace="global".
//
// Mirrors the per-agent MemoryTab UX but with no agent context: the
// Patterns sub-tab defaults "+ Add Pattern" to namespace="global", and
// the bottom "Forget all global memory" button bulk-deletes via
// DELETE /api/memory?namespace=global.
//
// Mock layer: stubs `@/api/memory` directly, same pattern as
// MemoryTab.test.tsx.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { MemoryPage } from "./MemoryPage";
import * as memoryApi from "@/api/memory";

vi.mock("@/api/memory", async () => {
  const actual = await vi.importActual<typeof import("@/api/memory")>(
    "@/api/memory",
  );
  return {
    ...actual,
    listMemory: vi.fn(),
    createPattern: vi.fn(),
    deleteMemoryItem: vi.fn(),
    forgetMemory: vi.fn(),
  };
});

function emptyList(): memoryApi.MemoryListResponse {
  return { items: [], total: 0 };
}

function pattern(
  id: string,
  text: string,
  namespace: string,
): memoryApi.MemoryItem {
  return {
    id,
    namespace,
    tier: "pattern",
    text,
    created_at: "2026-05-21T12:00:00Z",
    run_id: null,
    scenario_id: null,
    cycle_idx: null,
    training_window_end: null,
  };
}

function observation(
  id: string,
  text: string,
  namespace: string,
): memoryApi.MemoryItem {
  return {
    id,
    namespace,
    tier: "observation",
    text,
    created_at: "2026-05-21T12:00:00Z",
    run_id: null,
    scenario_id: null,
    cycle_idx: null,
    training_window_end: null,
  };
}

function renderPage(initialEntries: string[] = ["/memory"]) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <QueryClientProvider client={client}>
        <MemoryPage />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.mocked(memoryApi.listMemory).mockResolvedValue(emptyList());
  vi.mocked(memoryApi.createPattern).mockResolvedValue(
    pattern("pat-new", "global wisdom", "global"),
  );
  vi.mocked(memoryApi.deleteMemoryItem).mockResolvedValue();
  vi.mocked(memoryApi.forgetMemory).mockResolvedValue({ deleted: 0 });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("MemoryPage — empty state", () => {
  it("renders without crashing when the global namespace has no memory", async () => {
    renderPage();
    expect(
      await screen.findByText(/No patterns yet/i),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Add Pattern/i }),
    ).toBeInTheDocument();
  });

  it("scopes the patterns list query to namespace=global", async () => {
    renderPage();
    await screen.findByText(/No patterns yet/i);
    await waitFor(() => {
      const calls = vi.mocked(memoryApi.listMemory).mock.calls;
      const patternCall = calls.find(
        (c) => c[0]?.tier === "pattern" && c[0]?.namespace === "global",
      );
      expect(patternCall).toBeTruthy();
    });
  });
});

describe("MemoryPage — Add Pattern defaults to global", () => {
  it("opens the modal with namespace=global preselected and POSTs that body", async () => {
    const user = userEvent.setup();
    renderPage();

    await screen.findByText(/No patterns yet/i);
    await user.click(screen.getByRole("button", { name: /Add Pattern/i }));

    const dialog = await screen.findByRole("dialog", {
      name: /Add Pattern/i,
    });
    const ns = within(dialog).getByLabelText(/^Namespace/i) as HTMLSelectElement;
    expect(ns.value).toBe("global");

    await user.type(
      within(dialog).getByLabelText(/^Text/i),
      "Operator-attested wisdom.",
    );
    await user.click(
      within(dialog).getByRole("button", { name: /^Add Pattern$/i }),
    );

    await waitFor(() => {
      expect(vi.mocked(memoryApi.createPattern)).toHaveBeenCalledTimes(1);
    });
    const body = vi.mocked(memoryApi.createPattern).mock.calls[0]?.[0];
    expect(body?.namespace).toBe("global");
    expect(body?.text).toBe("Operator-attested wisdom.");
  });
});

describe("MemoryPage — Observations sub-tab", () => {
  it("renders global-namespace observations on the Observations sub-tab", async () => {
    const user = userEvent.setup();
    vi.mocked(memoryApi.listMemory).mockImplementation(async (q) => {
      if (q?.tier === "observation" && q?.namespace === "global") {
        return {
          items: [
            observation("obs-1", "global observation row", "global"),
          ],
          total: 1,
        };
      }
      return emptyList();
    });

    renderPage();

    await screen.findByText(/No patterns yet/i);
    await user.click(screen.getByRole("tab", { name: /Observations/i }));

    expect(
      await screen.findByText(/global observation row/),
    ).toBeInTheDocument();
  });
});

describe("MemoryPage — Forget all global memory", () => {
  it("opens an AlertDialog and calls forgetMemory({ namespace: 'global' }) on confirm", async () => {
    const user = userEvent.setup();
    renderPage();

    await screen.findByText(/No patterns yet/i);
    await user.click(
      screen.getByRole("button", { name: /Forget all global memory/i }),
    );

    const dialog = await screen.findByRole("alertdialog", {
      name: /Forget all global memory/i,
    });
    expect(
      within(dialog).getByRole("button", { name: /^Cancel$/i }),
    ).toBeInTheDocument();
    const confirm = within(dialog).getByRole("button", {
      name: /Confirm/i,
    });
    await user.click(confirm);

    await waitFor(() => {
      expect(vi.mocked(memoryApi.forgetMemory)).toHaveBeenCalledWith({
        namespace: "global",
      });
    });
  });
});

describe("MemoryPage — deep-link highlight", () => {
  it("highlights the pattern row matching ?pattern=<id>", async () => {
    vi.mocked(memoryApi.listMemory).mockImplementation(async (q) => {
      if (q?.tier === "pattern" && q?.namespace === "global") {
        return {
          items: [
            pattern("pat-a", "first", "global"),
            pattern("pat-b", "second target", "global"),
          ],
          total: 2,
        };
      }
      return emptyList();
    });

    renderPage(["/memory?pattern=pat-b"]);

    const target = await screen.findByText(/second target/);
    // Walk up to the LI wrapper and check the highlight marker attribute.
    const li = target.closest("li");
    expect(li).not.toBeNull();
    expect(li?.getAttribute("data-highlighted")).toBe("true");
  });
});
