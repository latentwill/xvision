import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { AgentsRoute } from "./agents";
import * as agentsApi from "@/api/agents";
import type { Agent, AgentSlot } from "@/api/agents";
import * as toolsApi from "@/api/tools";

vi.mock("@/api/agents", async () => {
  const actual = await vi.importActual<typeof import("@/api/agents")>(
    "@/api/agents",
  );
  return {
    ...actual,
    listAgentsPaged: vi.fn(),
    updateAgent: vi.fn(),
  };
});

vi.mock("@/api/tools", async () => {
  const actual = await vi.importActual<typeof import("@/api/tools")>(
    "@/api/tools",
  );
  return {
    ...actual,
    listTools: vi.fn(),
  };
});

function renderRoute(initialEntry = "/agents") {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <AgentsRoute />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

function agent(overrides: Partial<Agent> = {}): Agent {
  return {
    agent_id: "ag-1",
    name: "Trend Trader",
    description: "Follows the trend",
    tags: ["trend"],
    slots: [
      {
        name: "trader",
        provider: "openai",
        model: "gpt-4.1-mini",
        system_prompt: "you are a trader",
        skill_ids: [],
    allowed_tools: [],
        max_tokens: null,
      },
    ],
    archived: false,
    created_at: "2025-01-01T00:00:00Z",
    updated_at: "2025-01-01T00:00:00Z",
    ...overrides,
  };
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

describe("AgentsRoute", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    stubMatchMediaDesktop();
    vi.mocked(agentsApi.listAgentsPaged).mockResolvedValue({
      items: [],
      total: 0,
    });
    vi.mocked(agentsApi.updateAgent).mockImplementation(async (agentId, body) =>
      agent({
        agent_id: agentId,
        name: body.name,
        description: body.description,
        tags: body.tags,
        slots: body.slots,
      }),
    );
    vi.mocked(toolsApi.listTools).mockResolvedValue({
      items: [
        {
          name: "indicator_panel",
          description: "Read indicator panel data",
          input_schema: {},
          built_in: true,
        },
        {
          name: "web_search",
          description: "Search the web",
          input_schema: {},
          built_in: true,
        },
      ],
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("renders an empty state with a CTA to create an agent", async () => {
    renderRoute();
    await waitFor(() =>
      expect(agentsApi.listAgentsPaged).toHaveBeenCalled(),
    );
    expect(
      await screen.findByText(/No agents yet/i),
    ).toBeInTheDocument();
    expect(
      screen.getAllByRole("link", { name: /New agent/ }).length,
    ).toBeGreaterThan(0);
  });

  it("surfaces a Memory link pointing at /agents/memory", async () => {
    renderRoute();
    await waitFor(() =>
      expect(agentsApi.listAgentsPaged).toHaveBeenCalled(),
    );
    const memoryLink = await screen.findByRole("link", { name: /^Memory$/i });
    expect(memoryLink).toHaveAttribute("href", "/agents/memory");
  });

  it("renders agent name and slot count in the populated list", async () => {
    vi.mocked(agentsApi.listAgentsPaged).mockResolvedValue({
      items: [
        agent({
          agent_id: "ag-1",
          name: "Trend Trader",
          description: "Follows the trend",
        }),
      ],
      total: 1,
    });
    renderRoute();

    await screen.findByText("Trend Trader");
    expect(screen.getAllByText("Follows the trend").length).toBeGreaterThan(0);
    // Single-slot label includes the slot name.
    expect(screen.getByText(/1 \(trader\)/)).toBeInTheDocument();
  });

  it("forwards the include_archived filter to the backend listAgentsPaged call", async () => {
    vi.mocked(agentsApi.listAgentsPaged).mockResolvedValue({
      items: [agent({ archived: true })],
      total: 1,
    });
    renderRoute("/agents?archived=include");

    await waitFor(() => {
      const calls = vi.mocked(agentsApi.listAgentsPaged).mock.calls;
      expect(
        calls.some(
          ([f]) => (f as { include_archived?: boolean }).include_archived === true,
        ),
      ).toBe(true);
    });
  });

  it("hydrates the search box from the ?q= URL param", async () => {
    vi.mocked(agentsApi.listAgentsPaged).mockResolvedValue({
      items: [agent({ name: "Trend Trader" })],
      total: 1,
    });
    renderRoute("/agents?q=trend");

    const search = (await screen.findByPlaceholderText(
      "Search agents by name…",
    )) as HTMLInputElement;
    await waitFor(() => expect(search.value).toBe("trend"));
  });

  it("filters the in-page rows by the live search query", async () => {
    vi.mocked(agentsApi.listAgentsPaged).mockResolvedValue({
      items: [
        agent({
          agent_id: "a",
          name: "Trend Trader",
          description: "Follows the trend",
          tags: ["trend"],
        }),
        agent({
          agent_id: "b",
          name: "Mean Reverter",
          description: "Buys dips",
          tags: ["meanrev"],
        }),
      ],
      total: 2,
    });
    renderRoute();

    await screen.findByText("Trend Trader");
    expect(screen.getAllByText("Mean Reverter").length).toBeGreaterThan(0);

    const search = (await screen.findByPlaceholderText(
      "Search agents by name…",
    )) as HTMLInputElement;
    fireEvent.change(search, { target: { value: "trend" } });

    await waitFor(() =>
      expect(screen.queryByText("Mean Reverter")).not.toBeInTheDocument(),
    );
    expect(screen.getAllByText("Trend Trader").length).toBeGreaterThan(0);
  });

  // BUG xvision-4wna: after archiving an agent, the user navigates to
  // /agents which defaults the archived filter to "exclude", making the
  // just-archived agent appear to vanish. The fix navigates to
  // /agents?archived=include so the archived filter is pre-set to "include"
  // and the agent remains visible.
  it("renders include_archived=true when route mounts with ?archived=include", async () => {
    const archivedSlot: AgentSlot = {
      name: "main",
      provider: "anthropic",
      model: "claude-sonnet-4-6",
      system_prompt: "",
      skill_ids: [],
      allowed_tools: [],
      max_tokens: null,
    };
    vi.mocked(agentsApi.listAgentsPaged).mockResolvedValue({
      items: [
        agent({
          agent_id: "archived-1",
          name: "Old Strategy",
          archived: true,
          slots: [archivedSlot],
        }),
      ],
      total: 1,
    });

    // Simulate landing on /agents?archived=include (what the post-archive
    // redirect produces).
    renderRoute("/agents?archived=include");

    await waitFor(() => {
      const calls = vi.mocked(agentsApi.listAgentsPaged).mock.calls;
      expect(
        calls.some(
          ([f]) =>
            (f as { include_archived?: boolean }).include_archived === true,
        ),
      ).toBe(true);
    });
  });

  it("filters by shape=single via the toolbar filter", async () => {
    vi.mocked(agentsApi.listAgentsPaged).mockResolvedValue({
      items: [
        agent({ agent_id: "a", name: "Single Slot" }),
        agent({
          agent_id: "b",
          name: "Two Slot",
          slots: [
            {
              name: "trader",
              provider: "openai",
              model: "gpt-4.1-mini",
              system_prompt: "",
              skill_ids: [],
    allowed_tools: [],
              max_tokens: null,
            },
            {
              name: "risk",
              provider: "openai",
              model: "gpt-4.1-mini",
              system_prompt: "",
              skill_ids: [],
    allowed_tools: [],
              max_tokens: null,
            },
          ],
        }),
      ],
      total: 2,
    });

    renderRoute("/agents?shape=single");

    await screen.findByText("Single Slot");
    await waitFor(() =>
      expect(screen.queryByText("Two Slot")).not.toBeInTheDocument(),
    );
    expect(screen.getAllByText("Single Slot").length).toBeGreaterThan(0);
  });

  it("lets operators change an agent's tools from the list row", async () => {
    const user = userEvent.setup();
    vi.mocked(agentsApi.listAgentsPaged).mockResolvedValue({
      items: [
        agent({
          agent_id: "ag-1",
          name: "Trend Trader",
          slots: [
            {
              name: "trader",
              provider: "openai",
              model: "gpt-4.1-mini",
              system_prompt: "you are a trader",
              skill_ids: [],
              allowed_tools: [],
              max_tokens: null,
            },
          ],
        }),
      ],
      total: 1,
    });
    renderRoute();

    const toolsButton = await screen.findByRole("button", {
      name: "Tools for Trend Trader",
    });
    await user.click(toolsButton);
    await user.click(await screen.findByRole("option", { name: /indicator_panel/ }));

    await waitFor(() =>
      expect(agentsApi.updateAgent).toHaveBeenCalledWith("ag-1", {
        slots: [
          expect.objectContaining({
            name: "trader",
            allowed_tools: ["indicator_panel"],
          }),
        ],
      }),
    );
  });
});
