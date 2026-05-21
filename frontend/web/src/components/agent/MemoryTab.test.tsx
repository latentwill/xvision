// MemoryTab — vitest coverage for the per-agent Memory tab UI surface.
//
// Mock layer: we stub `@/api/memory` directly (same pattern as
// `agents.test.tsx` uses for `@/api/agents`). No msw — the codebase
// has no msw dependency and the API helpers are thin enough that
// module-level mocks give us the same coverage with less setup.

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

import { MemoryTab } from "./MemoryTab";
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

function pattern(id: string, text: string, namespace: string): memoryApi.MemoryItem {
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
  extra: Partial<memoryApi.MemoryItem> = {},
): memoryApi.MemoryItem {
  return {
    id,
    namespace,
    tier: "observation",
    text,
    created_at: "2026-05-21T12:00:00Z",
    run_id: extra.run_id ?? null,
    scenario_id: extra.scenario_id ?? null,
    cycle_idx: extra.cycle_idx ?? null,
    training_window_end: null,
  };
}

function renderTab(agentId = "agent-1") {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <MemoryTab agentId={agentId} />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.mocked(memoryApi.listMemory).mockResolvedValue(emptyList());
  vi.mocked(memoryApi.createPattern).mockResolvedValue(
    pattern("pat-new", "fresh wisdom", "agent:agent-1"),
  );
  vi.mocked(memoryApi.deleteMemoryItem).mockResolvedValue();
  vi.mocked(memoryApi.forgetMemory).mockResolvedValue({ deleted: 0 });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("MemoryTab — empty state", () => {
  it("renders without crashing when the agent has no memory", async () => {
    renderTab();
    // Patterns is the default sub-tab. Wait for the empty-state copy.
    expect(
      await screen.findByText(/No patterns yet/i),
    ).toBeInTheDocument();
    // "+ Add Pattern" button is always present on the Patterns sub-tab.
    expect(
      screen.getByRole("button", { name: /Add Pattern/i }),
    ).toBeInTheDocument();
  });
});

describe("MemoryTab — Add Pattern modal", () => {
  it("opens a modal with text, training-end, and namespace fields", async () => {
    const user = userEvent.setup();
    renderTab();

    await screen.findByText(/No patterns yet/i);
    await user.click(screen.getByRole("button", { name: /Add Pattern/i }));

    const dialog = await screen.findByRole("dialog", { name: /Add Pattern/i });
    expect(within(dialog).getByLabelText(/^Text/i)).toBeInTheDocument();
    expect(
      within(dialog).getByLabelText(/training data ends/i),
    ).toBeInTheDocument();
    expect(within(dialog).getByLabelText(/^Namespace/i)).toBeInTheDocument();
  });

  it("POSTs to createPattern with the form's body", async () => {
    const user = userEvent.setup();
    renderTab();

    await screen.findByText(/No patterns yet/i);
    await user.click(screen.getByRole("button", { name: /Add Pattern/i }));

    const dialog = await screen.findByRole("dialog", { name: /Add Pattern/i });
    await user.type(
      within(dialog).getByLabelText(/^Text/i),
      "Trust the indicator regime.",
    );
    await user.type(
      within(dialog).getByLabelText(/training data ends/i),
      "2025-12-31",
    );
    await user.click(
      within(dialog).getByRole("button", { name: /^Add Pattern$/i }),
    );

    await waitFor(() => {
      expect(vi.mocked(memoryApi.createPattern)).toHaveBeenCalledTimes(1);
    });
    const body = vi.mocked(memoryApi.createPattern).mock.calls[0]?.[0];
    expect(body).toBeTruthy();
    expect(body?.text).toBe("Trust the indicator regime.");
    expect(body?.namespace).toBe("agent:agent-1");
    // Date inputs serialize as YYYY-MM-DD; the UI normalises to RFC3339
    // by appending an end-of-day timestamp so the server's leakage
    // filter (a chrono::DateTime parse) accepts it.
    expect(body?.training_window_end).toMatch(/^2025-12-31/);
  });
});

describe("MemoryTab — Observations sub-tab", () => {
  it("shows a read-only observation list with no per-item delete", async () => {
    const user = userEvent.setup();
    vi.mocked(memoryApi.listMemory).mockImplementation(async (q) => {
      if (q?.tier === "observation") {
        return {
          items: [
            observation("obs-1", "regime broke at 09:32", "agent:agent-1", {
              run_id: "01HZRUN1",
              scenario_id: "btc-bull-q1",
              cycle_idx: 7,
            }),
          ],
          total: 1,
        };
      }
      return emptyList();
    });

    renderTab();

    await screen.findByText(/No patterns yet/i);
    await user.click(
      screen.getByRole("tab", { name: /Observations/i }),
    );

    expect(
      await screen.findByText(/regime broke at 09:32/),
    ).toBeInTheDocument();
    // No per-row delete button anywhere in the Observations panel.
    const panel = screen.getByRole("tabpanel", { name: /Observations/i });
    expect(
      within(panel).queryByRole("button", { name: /^Delete$/i }),
    ).not.toBeInTheDocument();
    expect(
      within(panel).queryByRole("button", { name: /^Remove$/i }),
    ).not.toBeInTheDocument();
  });

  it("filters observations by scenario_id and run_id", async () => {
    const user = userEvent.setup();
    vi.mocked(memoryApi.listMemory).mockResolvedValue(emptyList());
    renderTab();

    await screen.findByText(/No patterns yet/i);
    await user.click(screen.getByRole("tab", { name: /Observations/i }));

    const scenarioInput = await screen.findByLabelText(/Scenario id/i);
    await user.type(scenarioInput, "btc-bull-q1");
    const runInput = screen.getByLabelText(/Run id/i);
    await user.type(runInput, "01HZRUN1");

    await waitFor(() => {
      const calls = vi.mocked(memoryApi.listMemory).mock.calls;
      const obsCall = calls.find(
        (c) =>
          c[0]?.tier === "observation" &&
          c[0]?.scenario_id === "btc-bull-q1" &&
          c[0]?.run_id === "01HZRUN1",
      );
      expect(obsCall).toBeTruthy();
    });
  });
});

describe("MemoryTab — Forget all memory", () => {
  it("opens an AlertDialog with the item count + Cancel/Confirm", async () => {
    const user = userEvent.setup();
    vi.mocked(memoryApi.listMemory).mockImplementation(async (q) => {
      // For the forget summary the tab calls listMemory(agent=) without
      // a tier filter. Return three items so the count surfaces.
      if (q?.agent === "agent-1" && !q.tier) {
        return {
          items: [
            pattern("p-1", "p1", "agent:agent-1"),
            observation("o-1", "o1", "agent:agent-1"),
            observation("o-2", "o2", "agent:agent-1"),
          ],
          total: 3,
        };
      }
      return emptyList();
    });
    renderTab();

    // Wait for initial Patterns load so the count query has run.
    await screen.findByText(/No patterns yet/i);
    await user.click(
      screen.getByRole("button", { name: /Forget all memory/i }),
    );

    const dialog = await screen.findByRole("alertdialog", {
      name: /Forget all memory/i,
    });
    expect(within(dialog).getByText(/3/)).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", { name: /^Cancel$/i }),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", { name: /Confirm/i }),
    ).toBeInTheDocument();
  });

  it("calls forgetMemory({ agent }) on Confirm", async () => {
    const user = userEvent.setup();
    vi.mocked(memoryApi.listMemory).mockResolvedValue({
      items: [pattern("p-1", "p1", "agent:agent-1")],
      total: 1,
    });
    renderTab();

    await screen.findByText(/p1/);
    await user.click(
      screen.getByRole("button", { name: /Forget all memory/i }),
    );

    const dialog = await screen.findByRole("alertdialog", {
      name: /Forget all memory/i,
    });
    await user.click(within(dialog).getByRole("button", { name: /Confirm/i }));

    await waitFor(() => {
      expect(vi.mocked(memoryApi.forgetMemory)).toHaveBeenCalledWith({
        agent: "agent-1",
      });
    });
  });
});
