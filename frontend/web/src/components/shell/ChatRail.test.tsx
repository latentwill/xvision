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

import { ChatRail, invalidateForToolResult } from "./ChatRail";
import * as chatApi from "@/api/chat_rail";
import * as settingsApi from "@/api/settings";
import { strategyKeys } from "@/api/strategies";
import { scenarioKeys } from "@/api/scenarios";
import { agentKeys } from "@/api/agents";
import { evalKeys } from "@/api/eval";
import type { WizardEvent } from "@/api/chat_rail";

const defaultStorage = globalThis.localStorage;

vi.mock("@/api/chat_rail", async () => {
  const actual = await vi.importActual<typeof import("@/api/chat_rail")>(
    "@/api/chat_rail",
  );
  return {
    ...actual,
    createSession: vi.fn(),
    deleteSession: vi.fn(),
    listSessions: vi.fn(),
    loadSessionHistory: vi.fn(),
    resolveSession: vi.fn(),
    streamChat: vi.fn(),
  };
});

vi.mock("@/api/settings", async () => {
  const actual = await vi.importActual<typeof import("@/api/settings")>(
    "@/api/settings",
  );
  return {
    ...actual,
    listProviders: vi.fn(),
  };
});

function renderRail(path = "/strategies") {
  try {
    localStorage.setItem("xvn.chat_rail.open", "1");
  } catch {
    // Safari private or blocked storage should not prevent app startup.
  }
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter initialEntries={[path]}>
      <QueryClientProvider client={client}>
        <ChatRail />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

const workspaceScope = { scope: "workspace" } as const;

beforeEach(() => {
  localStorage.clear();
  vi.mocked(settingsApi.listProviders).mockResolvedValue({ providers: [] });
  vi.mocked(chatApi.listSessions).mockResolvedValue([
    {
      id: "old-session",
      scope: workspaceScope,
      started_at: "2026-05-13T00:00:00Z",
      last_activity_at: "2026-05-13T00:05:00Z",
    },
  ]);
  vi.mocked(chatApi.resolveSession).mockResolvedValue({
    session_id: "old-session",
    history: [
      {
        id: "m1",
        session_id: "old-session",
        seq: 0,
        role: "user",
        content_blocks: [{ type: "text", text: "previous question" }],
        ts: "2026-05-13T00:01:00Z",
      },
    ],
  });
  vi.mocked(chatApi.createSession).mockResolvedValue({
    session_id: "new-session",
    history: [],
  });
  vi.mocked(chatApi.loadSessionHistory).mockResolvedValue([]);
});

afterEach(() => {
  Object.defineProperty(globalThis, "localStorage", {
    value: defaultStorage,
    writable: true,
    configurable: true,
  });
  Object.defineProperty(window, "localStorage", {
    value: defaultStorage,
    writable: true,
    configurable: true,
  });
  cleanup();
  vi.restoreAllMocks();
});

describe("ChatRail", () => {
  it("creates a new chat without deleting the previous conversation", async () => {
    renderRail();

    expect(await screen.findByText("previous question")).toBeInTheDocument();
    const composer = screen.getByPlaceholderText(
      /ask anything about your workspace/i,
    );
    fireEvent.change(composer, {
      target: { value: "draft text" },
    });

    fireEvent.click(screen.getByRole("button", { name: "New chat" }));

    await waitFor(() => {
      expect(chatApi.createSession).toHaveBeenCalledWith(workspaceScope);
    });
    expect(chatApi.deleteSession).not.toHaveBeenCalled();
    expect(
      screen.getByPlaceholderText(/ask anything about your workspace/i),
    ).toHaveValue("");
    expect(await screen.findByText(/No messages yet/i)).toBeInTheDocument();
  });

  it("uses one shared workspace session on list and Inspector routes", async () => {
    renderRail("/authoring/01TEST");

    await waitFor(() => {
      expect(chatApi.resolveSession).toHaveBeenCalledWith(workspaceScope);
    });

    cleanup();
    vi.mocked(chatApi.resolveSession).mockClear();

    renderRail("/strategies");

    await waitFor(() => {
      expect(chatApi.resolveSession).toHaveBeenCalledWith(workspaceScope);
    });
  });

  it("does not block app startup when Safari storage access is unavailable", () => {
    const blockedStorage = {
      getItem() {
        throw new DOMException("Blocked", "SecurityError");
      },
      setItem() {
        throw new DOMException("Blocked", "SecurityError");
      },
      removeItem() {
        throw new DOMException("Blocked", "SecurityError");
      },
      clear() {
        throw new DOMException("Blocked", "SecurityError");
      },
    };
    Object.defineProperty(globalThis, "localStorage", {
      value: blockedStorage,
      writable: true,
      configurable: true,
    });
    Object.defineProperty(window, "localStorage", {
      value: blockedStorage,
      writable: true,
      configurable: true,
    });

    renderRail();

    expect(screen.getByLabelText("Chat rail")).toBeInTheDocument();
  });

  it("keeps the composer editable while a chat response is in flight", async () => {
    vi.mocked(chatApi.streamChat).mockImplementation(async function* () {
      await new Promise(() => {});
    });
    renderRail();

    const composer = await screen.findByPlaceholderText(
      /ask anything about your workspace/i,
    );
    fireEvent.change(composer, {
      target: { value: "first request" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send message" }));

    await waitFor(() => {
      expect(chatApi.streamChat).toHaveBeenCalled();
    });
    fireEvent.change(composer, {
      target: { value: "next draft while the agent works" },
    });

    expect(composer).toBeEnabled();
    expect(composer).toHaveValue("next draft while the agent works");
  });

  it("aborts the active chat request without clearing draft text", async () => {
    let capturedSignal: AbortSignal | undefined;
    vi.mocked(chatApi.streamChat).mockImplementation(async function* (
      _req,
      signal,
    ) {
      capturedSignal = signal;
      await new Promise<void>((resolve) => {
        signal?.addEventListener("abort", () => resolve(), { once: true });
      });
      throw Object.assign(new Error("aborted"), { name: "AbortError" });
    });
    renderRail();

    const composer = await screen.findByPlaceholderText(
      /ask anything about your workspace/i,
    );
    fireEvent.change(composer, {
      target: { value: "start long request" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send message" }));

    const stop = await screen.findByRole("button", { name: "Stop response" });
    fireEvent.change(composer, {
      target: { value: "keep this draft" },
    });
    fireEvent.click(stop);

    await waitFor(() => {
      expect(capturedSignal?.aborted).toBe(true);
    });
    expect(composer).toHaveValue("keep this draft");
  });
});

/**
 * Regression coverage for `chat-rail-strategy-list-refresh` (operator
 * 2026-05-18): creating a strategy via the chat rail must invalidate
 * the strategies list query so the row appears without a manual
 * refresh, and the same must hold for every mutating wizard tool.
 *
 * Tested in isolation rather than through the full ChatRail render
 * because the SSE event loop is mocked at the network layer in the
 * other tests above — wiring a fake event into `streamChat` and
 * waiting for the TanStack effect would be flaky. The pure helper is
 * the source of truth for what gets invalidated per tool name.
 */
describe("invalidateForToolResult", () => {
  function spyClient() {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const spy = vi.spyOn(qc, "invalidateQueries");
    return { qc, spy };
  }

  function toolResult(tool: string, result: unknown = { ok: true }): WizardEvent {
    return { type: "tool_result", tool, result };
  }

  it("ignores non-tool_result events", () => {
    const { qc, spy } = spyClient();
    invalidateForToolResult(qc, { type: "token", text: "hi" });
    invalidateForToolResult(qc, {
      type: "tool_call",
      tool: "create_strategy",
      args: {},
    });
    invalidateForToolResult(qc, { type: "done" });
    expect(spy).not.toHaveBeenCalled();
  });

  it("ignores failed tool results (no mutation happened)", () => {
    const { qc, spy } = spyClient();
    invalidateForToolResult(qc, toolResult("create_strategy", { error: "boom" }));
    expect(spy).not.toHaveBeenCalled();
  });

  it("does NOT treat success payloads with `error: null` / `error: \"\"` as failures", () => {
    // Rust API responses serialized from `Option<String>` ship
    // `error: null` on the wire when there is no real error. The old
    // `"error" in result` check bailed on those payloads, which left
    // the strategies list stale after a successful create — exactly
    // the operator-reported regression. The truthy-error gate fixes it.
    const { qc, spy } = spyClient();
    invalidateForToolResult(
      qc,
      toolResult("create_strategy", { id: "01OK", error: null }),
    );
    invalidateForToolResult(
      qc,
      toolResult("create_strategy", { id: "01OK2", error: "" }),
    );
    expect(spy).toHaveBeenCalledTimes(2);
    expect(spy).toHaveBeenCalledWith({ queryKey: strategyKeys.all });
  });

  it("ignores read-only validate_draft", () => {
    const { qc, spy } = spyClient();
    invalidateForToolResult(qc, toolResult("validate_draft"));
    expect(spy).not.toHaveBeenCalled();
  });

  it("ignores unknown tools (new mutating tools must opt in explicitly)", () => {
    const { qc, spy } = spyClient();
    invalidateForToolResult(qc, toolResult("future_unknown_tool"));
    expect(spy).not.toHaveBeenCalled();
  });

  it("invalidates the strategies list on create_strategy", () => {
    const { qc, spy } = spyClient();
    invalidateForToolResult(qc, toolResult("create_strategy"));
    expect(spy).toHaveBeenCalledWith({ queryKey: strategyKeys.all });
  });

  it.each([
    "update_slot",
    "update_manifest",
    "set_mechanical_param",
    "set_risk_config",
    "attach_agent",
  ])("invalidates the strategies list on %s", (tool) => {
    const { qc, spy } = spyClient();
    invalidateForToolResult(qc, toolResult(tool));
    expect(spy).toHaveBeenCalledWith({ queryKey: strategyKeys.all });
  });

  it("invalidates BOTH strategies and agents when create_strategy ships an `agent` payload", () => {
    // The wizard_loop calls `create_default_strategy_agent` after a
    // successful `create_strategy` when a provider/model is selected
    // and folds the new agent under `agent` in the tool result. The
    // agents list must refetch in that case — invalidating only
    // strategies left /agents stale (PR #276 review).
    const { qc, spy } = spyClient();
    invalidateForToolResult(
      qc,
      toolResult("create_strategy", {
        id: "01STRAT",
        agent: { agent_id: "01AGENT", provider: "anthropic" },
      }),
    );
    expect(spy).toHaveBeenCalledWith({ queryKey: strategyKeys.all });
    expect(spy).toHaveBeenCalledWith({ queryKey: agentKeys.all });
  });

  it("does NOT invalidate agents on bare create_strategy (no `agent` in result)", () => {
    // No provider/model selected → no default-agent creation → agents
    // list is not stale → skip the second invalidation.
    const { qc, spy } = spyClient();
    invalidateForToolResult(qc, toolResult("create_strategy", { id: "01STRAT" }));
    expect(spy).toHaveBeenCalledWith({ queryKey: strategyKeys.all });
    expect(spy).not.toHaveBeenCalledWith({ queryKey: agentKeys.all });
  });

  it("invalidates BOTH strategies and agents on create_strategy_agent", () => {
    // create_strategy_agent creates an agent row in the agents library
    // AND attaches it to a strategy (strategies list reflects the new
    // AgentRef count). Both query keys must invalidate.
    const { qc, spy } = spyClient();
    invalidateForToolResult(qc, toolResult("create_strategy_agent"));
    expect(spy).toHaveBeenCalledWith({ queryKey: strategyKeys.all });
    expect(spy).toHaveBeenCalledWith({ queryKey: agentKeys.all });
  });

  it("invalidates the scenarios list on create_scenario", () => {
    const { qc, spy } = spyClient();
    invalidateForToolResult(qc, toolResult("create_scenario"));
    expect(spy).toHaveBeenCalledWith({ queryKey: scenarioKeys.all });
  });

  it("invalidates the eval list on run_eval", () => {
    const { qc, spy } = spyClient();
    invalidateForToolResult(qc, toolResult("run_eval"));
    expect(spy).toHaveBeenCalledWith({ queryKey: evalKeys.all });
  });
});
