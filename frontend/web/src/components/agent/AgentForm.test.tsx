// Tests for <AgentForm>. Focused on the post-archive navigation target
// (BUG xvision-4wna) and the at-least-one-slot constraint visibility
// (BUG xvision-mkdd — covered in SlotForm.test.tsx).

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

import { AgentForm } from "./AgentForm";
import * as agentsApi from "@/api/agents";
import type { Agent } from "@/api/agents";

// ── Mocks ─────────────────────────────────────────────────────────────────────

const mockNavigate = vi.fn();
vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual<typeof import("react-router-dom")>(
    "react-router-dom",
  );
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

vi.mock("@/api/agents", async () => {
  const actual = await vi.importActual<typeof import("@/api/agents")>(
    "@/api/agents",
  );
  return {
    ...actual,
    getAgent: vi.fn(),
    archiveAgent: vi.fn(),
    validateAgent: vi.fn(),
    deployedInStrategies: vi.fn(),
    recentRuns: vi.fn(),
    updateAgent: vi.fn(),
    createAgent: vi.fn(),
  };
});

// SlotForm fetches providers and tools; stub them so SlotForm mounts cleanly.
vi.mock("@/api/settings", async () => {
  const actual = await vi.importActual<typeof import("@/api/settings")>(
    "@/api/settings",
  );
  return {
    ...actual,
    listProviders: vi.fn().mockResolvedValue({ providers: [], default_model: null }),
  };
});
vi.mock("@/api/tools", async () => {
  const actual = await vi.importActual<typeof import("@/api/tools")>(
    "@/api/tools",
  );
  return {
    ...actual,
    listTools: vi.fn().mockResolvedValue({ items: [] }),
  };
});

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeAgent(overrides: Partial<Agent> = {}): Agent {
  return {
    agent_id: "ag-test",
    name: "Test Agent",
    description: "A test agent",
    tags: [],
    slots: [
      {
        name: "main",
        provider: "anthropic",
        model: "claude-sonnet-4-6",
        system_prompt: "You are a test agent.",
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

function renderForm(agentId?: string) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={qc}>
        <AgentForm agentId={agentId} />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("AgentForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(agentsApi.deployedInStrategies).mockResolvedValue([]);
    vi.mocked(agentsApi.recentRuns).mockResolvedValue([]);
    vi.mocked(agentsApi.validateAgent).mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
  });

  // BUG xvision-4wna: archiveAgent previously navigated to "/agents",
  // which defaults the archived filter to "exclude", making the
  // just-archived agent appear to vanish. The fix navigates to
  // "/agents?archived=include" so archived agents are immediately visible.
  it("navigates to /agents?archived=include after a successful archive", async () => {
    vi.mocked(agentsApi.getAgent).mockResolvedValue(makeAgent());
    vi.mocked(agentsApi.archiveAgent).mockResolvedValue(undefined as never);

    renderForm("ag-test");

    // Wait for the agent to load and the Archive button to render.
    const archiveBtn = await screen.findByRole("button", { name: /archive/i });
    fireEvent.click(archiveBtn);

    await waitFor(() => {
      expect(agentsApi.archiveAgent).toHaveBeenCalledWith("ag-test");
      expect(mockNavigate).toHaveBeenCalledWith("/agents?archived=include");
    });
  });
});
