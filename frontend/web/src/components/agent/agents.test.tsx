// modelMetadata table coverage. The per-slot `max_tokens` input was
// removed in 2026-05-17 (qa-remove-agent-max-tokens), so the old
// SlotForm-rendering UX tests are gone. The metadata table itself is
// still consumed (for provider-catalog tooling / labels), so the lookup
// behaviour stays under test.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import {
  autoMaxTokens,
  hasModelMetadata,
  isReasoning,
  lookupModel,
} from "./modelMetadata";
import { AgentForm } from "./AgentForm";
import * as agentsApi from "@/api/agents";
import * as settingsApi from "@/api/settings";

vi.mock("@/api/agents", async () => {
  const actual = await vi.importActual<typeof import("@/api/agents")>(
    "@/api/agents",
  );
  return {
    ...actual,
    getAgent: vi.fn(),
    deployedInStrategies: vi.fn(),
    recentRuns: vi.fn(),
    createAgent: vi.fn(),
    updateAgent: vi.fn(),
    archiveAgent: vi.fn(),
    validateAgent: vi.fn(),
  };
});

vi.mock("@/api/settings", () => ({
  settingsKeys: {
    providers: () => ["settings", "providers"],
  },
  listProviders: vi.fn(),
}));

const baseAgent = {
  agent_id: "agent-1",
  name: "Server name",
  description: "Server description",
  tags: ["server"],
  slots: [
    {
      name: "main",
      provider: "openrouter",
      model: "claude-sonnet-4-6",
      system_prompt: "Follow the plan.",
      skill_ids: [],
      max_tokens: null,
    },
  ],
  archived: false,
  created_at: "2026-05-13T14:52:21Z",
  updated_at: "2026-05-13T14:52:21Z",
} satisfies agentsApi.Agent;

function providerRow(name: string, enabledModels: string[]) {
  return {
    name,
    kind: "anthropic",
    base_url: "",
    api_key_env: `${name.toUpperCase()}_KEY`,
    api_key_set: true,
    synthetic: false,
    is_default: false,
    enabled_models: enabledModels,
  };
}

function renderAgentForm(agentId = "agent-1") {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const view = render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <AgentForm agentId={agentId} />
      </QueryClientProvider>
    </MemoryRouter>,
  );
  return { client, ...view };
}

beforeEach(() => {
  vi.mocked(agentsApi.getAgent).mockResolvedValue(baseAgent);
  vi.mocked(agentsApi.deployedInStrategies).mockResolvedValue([]);
  vi.mocked(agentsApi.recentRuns).mockResolvedValue([]);
  vi.mocked(agentsApi.createAgent).mockResolvedValue(baseAgent);
  vi.mocked(agentsApi.updateAgent).mockResolvedValue(baseAgent);
  vi.mocked(agentsApi.validateAgent).mockResolvedValue([]);
  vi.mocked(settingsApi.listProviders).mockResolvedValue({ providers: [] });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("modelMetadata table", () => {
  it("falls back to a non-reasoning default for unknown models", () => {
    const meta = lookupModel("acme-co/nightly-7b");
    expect(meta.class).toBe("standard");
    expect(meta.output_token_ceiling).toBeGreaterThanOrEqual(2048);
    expect(autoMaxTokens(meta)).toBeGreaterThanOrEqual(meta.recommended_visible_output);
  });

  it("strips OpenRouter vendor prefix", () => {
    const a = lookupModel("anthropic/claude-sonnet-4-6");
    const b = lookupModel("claude-sonnet-4-6");
    expect(a).toEqual(b);
  });

  it("flags reasoning-class models", () => {
    expect(isReasoning(lookupModel("deepseek-r1"))).toBe(true);
    expect(isReasoning(lookupModel("o3"))).toBe(true);
    expect(isReasoning(lookupModel("claude-haiku-4-5"))).toBe(false);
  });

  it("matches date-stamped variants by prefix", () => {
    const exact = lookupModel("claude-sonnet-4-6");
    const dated = lookupModel("claude-sonnet-4-6-20260101");
    expect(dated.output_token_ceiling).toBe(exact.output_token_ceiling);
  });

  it("resolves the legacy LLMSlot attested_with dotted form", () => {
    // Pre-agent templates store `attested_with` as
    // `"anthropic.claude-sonnet-4.6"`. The dispatcher resolves this to
    // the model's real ceiling; the lookup table must agree so anywhere
    // else that consumes the metadata (tooling, labels) matches.
    const legacy = lookupModel("anthropic.claude-sonnet-4.6");
    const canonical = lookupModel("claude-sonnet-4-6");
    expect(legacy.output_token_ceiling).toBe(canonical.output_token_ceiling);
    expect(legacy.recommended_visible_output).toBe(canonical.recommended_visible_output);
    expect(legacy.class).toBe(canonical.class);
  });

  it("does not misread version dots in real model ids as provider prefixes", () => {
    // `gpt-4.1` is an actual OpenAI model id; the lookup must not treat
    // `gpt-4` as a provider prefix and strip it.
    const m = lookupModel("gpt-4.1");
    expect(m.output_token_ceiling).toBe(32768);
    expect(m.class).toBe("standard");
  });

  it("hasModelMetadata distinguishes known models from the UNKNOWN fallback", () => {
    expect(hasModelMetadata("claude-sonnet-4-6")).toBe(true);
    expect(hasModelMetadata("anthropic/claude-sonnet-4-6")).toBe(true);
    expect(hasModelMetadata("gpt-4.1")).toBe(true);
    expect(hasModelMetadata("acme-co/nightly-7b")).toBe(false);
    expect(hasModelMetadata("")).toBe(false);
  });
});

describe("AgentForm edit hydration", () => {
  it("preserves dirty draft fields when the agent detail query is replaced", async () => {
    const user = userEvent.setup();
    const { client } = renderAgentForm();

    const name = await screen.findByLabelText(/^Name$/);
    expect(name).toHaveValue("Server name");

    await user.clear(name);
    await user.type(name, "Unsaved draft");

    client.setQueryData(agentsApi.agentKeys.detail("agent-1"), {
      ...baseAgent,
      name: "Refetched server name",
      updated_at: "2026-05-13T15:00:00Z",
    });

    await waitFor(() => {
      expect(screen.getByLabelText(/^Name$/)).toHaveValue("Unsaved draft");
    });
  });
});

describe("AgentForm slot editing", () => {
  it("clears hidden max_tokens when duplicating a legacy slot", async () => {
    const user = userEvent.setup();
    vi.mocked(agentsApi.getAgent).mockResolvedValue({
      ...baseAgent,
      slots: [{ ...baseAgent.slots[0], max_tokens: 4096 }],
    });

    renderAgentForm();

    await screen.findByLabelText(/^Name$/);
    await user.click(screen.getByTitle("Duplicate slot"));
    await user.click(screen.getByRole("button", { name: "Save changes" }));

    await waitFor(() => {
      const call = vi.mocked(agentsApi.updateAgent).mock.calls[0];
      if (!call) throw new Error("updateAgent was not called");
      const payload = call[1];
      if (!payload) throw new Error("updateAgent payload was not passed");
      const { slots } = payload;
      if (!slots) throw new Error("updateAgent payload did not include slots");
      expect(slots[1]?.max_tokens).toBeNull();
    });
  });

  it("clears a stale model when the provider changes", async () => {
    const user = userEvent.setup();
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        providerRow("openrouter", ["claude-sonnet-4-6"]),
        providerRow("anthropic", ["claude-haiku-4-5"]),
      ],
    });

    renderAgentForm();

    await waitFor(() => {
      expect(screen.getByLabelText("Provider").tagName).toBe("SELECT");
    });
    const provider = screen.getByLabelText("Provider");
    await user.selectOptions(provider, "anthropic");

    expect(screen.getByLabelText("Model")).toHaveValue("");
  });
});

describe("AgentForm memory selector (V2D)", () => {
  it("round-trips agent_scoped through updateAgent", async () => {
    const user = userEvent.setup();
    renderAgentForm();

    const select = (await screen.findByLabelText("Memory")) as HTMLSelectElement;
    await user.selectOptions(select, "agent_scoped");
    await user.click(screen.getByRole("button", { name: "Save changes" }));

    await waitFor(() => {
      const call = vi.mocked(agentsApi.updateAgent).mock.calls[0];
      if (!call) throw new Error("updateAgent was not called");
      const payload = call[1];
      if (!payload || !payload.slots) {
        throw new Error("updateAgent payload did not include slots");
      }
      expect(payload.slots[0]?.memory_mode).toBe("agent_scoped");
    });
  });
});

describe("AgentForm cross references", () => {
  it("renders strategy name and run id when both API mocks return one-item arrays", async () => {
    vi.mocked(agentsApi.deployedInStrategies).mockResolvedValue([
      { strategy_id: "strat-1", name: "My Alpha Strategy" },
    ]);
    vi.mocked(agentsApi.recentRuns).mockResolvedValue([
      { run_id: "01HZRUN123456", scenario_id: "crypto-bull-q1-2025", status: "completed" },
    ]);

    renderAgentForm();

    expect(await screen.findByText("My Alpha Strategy")).toBeInTheDocument();
    // The UI renders run_id.slice(0,12)+"… — "+status as separate adjacent text
    // nodes inside one <li>, so getByText(exact string) won't match the full
    // element text. Use a regex against the combined textContent instead.
    expect(screen.getByText((_, el) => {
      return !!el && el.tagName === "LI" && (el.textContent ?? "").includes("01HZRUN12345");
    })).toBeInTheDocument();
    expect(screen.queryByText(/Not deployed in any strategy yet/)).not.toBeInTheDocument();
    expect(screen.queryByText(/No runs yet/)).not.toBeInTheDocument();
  });

  it("shows loading states instead of empty states while cross-reference queries are pending", async () => {
    vi.mocked(agentsApi.deployedInStrategies).mockReturnValue(
      new Promise(() => {}),
    );
    vi.mocked(agentsApi.recentRuns).mockReturnValue(new Promise(() => {}));

    renderAgentForm();

    expect(await screen.findByText("Loading deployed strategies…")).toBeInTheDocument();
    expect(screen.getByText("Loading recent runs…")).toBeInTheDocument();
    expect(
      screen.queryByText(/Not deployed in any strategy yet/),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/No runs yet/)).not.toBeInTheDocument();
  });

  it("shows error states instead of empty states when cross-reference queries fail", async () => {
    vi.mocked(agentsApi.deployedInStrategies).mockRejectedValue(
      new Error("strategies unavailable"),
    );
    vi.mocked(agentsApi.recentRuns).mockRejectedValue(
      new Error("runs unavailable"),
    );

    renderAgentForm();

    expect(
      await screen.findByText(
        "Couldn't load deployed strategies: strategies unavailable",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Couldn't load recent runs: runs unavailable"),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(/Not deployed in any strategy yet/),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/No runs yet/)).not.toBeInTheDocument();
  });
});
