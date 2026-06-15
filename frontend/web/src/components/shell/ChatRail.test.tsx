import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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

import {
  ChatRail,
  computeUserAnchor,
  invalidateForToolResult,
  mergeUnifiedRows,
} from "./ChatRail";
import * as chatApi from "@/api/chat_rail";
import * as settingsApi from "@/api/settings";
import { strategyKeys } from "@/api/strategies";
import { scenarioKeys } from "@/api/scenarios";
import { agentKeys } from "@/api/agents";
import { evalKeys } from "@/api/eval";
import type { WizardEvent } from "@/api/chat_rail";
import type { Bubble } from "@/components/chat/types";
import type { MessageRow } from "@/stores/message-row-reducer";

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
    setSessionMode: vi.fn(),
    scopeFromPath: vi.fn(),
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
  vi.mocked(chatApi.scopeFromPath).mockReturnValue(workspaceScope);
  vi.mocked(settingsApi.listProviders).mockResolvedValue({
    providers: [],
    default_model: null,
  });
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
  vi.mocked(chatApi.deleteSession).mockResolvedValue(undefined);
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
  it("merges a fresh canonical assistant reply into the latest placeholder", () => {
    const rows: MessageRow[] = [
      {
        type: "assistant",
        id: "assistant:old-session:0",
        seq: 10,
        streamId: "old-session",
        appliedEventIds: new Set(["old"]),
        actor: "agent",
        text: "old answer",
        blocks: [],
        done: true,
        draftId: null,
        messageIndex: 0,
      },
      {
        type: "assistant",
        id: "assistant:old-session:1",
        seq: 20,
        streamId: "old-session",
        appliedEventIds: new Set(["reply"]),
        actor: "agent",
        text: "fresh answer",
        blocks: [],
        done: true,
        draftId: null,
        messageIndex: 0,
      },
    ];

    const merged = mergeUnifiedRows(
      [
        { role: "user", text: "old question", assistantAnchor: 0 },
        { role: "user", text: "new question", assistantAnchor: 1 },
        {
          role: "assistant",
          blocks: [{ kind: "text", text: "" }],
          tools: [],
        },
      ],
      rows,
    );

    expect(merged.map((b) => b.role)).toEqual([
      "user",
      "assistant",
      "user",
      "assistant",
    ]);
    expect(merged[1]).toMatchObject({ role: "assistant" });
    if (merged[1].role !== "assistant" || merged[3].role !== "assistant") {
      throw new Error("expected assistant bubbles");
    }
    expect(merged[1].blocks[0]).toMatchObject({
      kind: "text",
      text: "old answer",
    });
    expect(merged[3].blocks[0]).toMatchObject({
      kind: "text",
      text: "fresh answer",
    });
  });

  it("does not move replayed historical assistant rows under the latest user placeholder", () => {
    const rows: MessageRow[] = [
      {
        type: "assistant",
        id: "assistant:old-session:0",
        seq: 10,
        streamId: "old-session",
        appliedEventIds: new Set(["old"]),
        actor: "agent",
        text: "old answer",
        blocks: [],
        done: true,
        draftId: null,
        messageIndex: 0,
      },
    ];

    const merged = mergeUnifiedRows(
      [
        { role: "user", text: "old question", assistantAnchor: 0 },
        { role: "user", text: "new question", assistantAnchor: 1 },
        {
          role: "assistant",
          blocks: [{ kind: "text", text: "" }],
          tools: [],
        },
      ],
      rows,
    );

    if (merged[1].role !== "assistant" || merged[3].role !== "assistant") {
      throw new Error("expected assistant bubbles");
    }
    expect(merged[1].blocks[0]).toMatchObject({
      kind: "text",
      text: "old answer",
    });
    expect(merged[3].blocks[0]).toMatchObject({
      kind: "text",
      text: "",
    });
  });

  it("hides legacy user turns inside a checkpoint rollback window", () => {
    const rows: MessageRow[] = [
      {
        type: "assistant",
        id: "assistant:old-session:0",
        seq: 10,
        streamId: "old-session",
        appliedEventIds: new Set(["old"]),
        actor: "agent",
        text: "before checkpoint",
        blocks: [],
        done: true,
        draftId: null,
        messageIndex: 0,
      },
      {
        type: "checkpoint",
        id: "checkpoint:old-session:cp1:created",
        seq: 15,
        streamId: "old-session",
        appliedEventIds: new Set(["cp-created"]),
        actor: "system",
        status: "created",
        checkpointId: "cp1",
        restored: [],
        code: null,
        message: null,
      },
      {
        type: "assistant",
        id: "assistant:old-session:1",
        seq: 30,
        streamId: "old-session",
        appliedEventIds: new Set(["rolled"]),
        actor: "agent",
        text: "rolled back answer",
        blocks: [],
        done: true,
        draftId: null,
        messageIndex: 1,
      },
      {
        type: "checkpoint",
        id: "checkpoint:old-session:cp1:restored",
        seq: 40,
        streamId: "old-session",
        appliedEventIds: new Set(["cp-restored"]),
        actor: "system",
        status: "restored",
        checkpointId: "cp1",
        restored: [],
        code: null,
        message: null,
      },
    ];

    const merged = mergeUnifiedRows(
      [
        { role: "user", text: "before checkpoint question", assistantAnchor: 0 },
        { role: "user", text: "rolled back user turn", assistantAnchor: 1 },
        { role: "user", text: "after restore question", assistantAnchor: 2 },
      ],
      rows,
    );

    expect(merged.map((b) => (b.role === "user" ? b.text : b.role))).toEqual([
      "before checkpoint question",
      "assistant",
      "checkpoint",
      "checkpoint",
      "after restore question",
    ]);
    expect(
      merged.some((b) => b.role === "user" && b.text === "rolled back user turn"),
    ).toBe(false);
    expect(
      merged.some(
        (b) =>
          b.role === "assistant" &&
          b.blocks.some(
            (block) => block.kind === "text" && block.text === "rolled back answer",
          ),
      ),
    ).toBe(false);
  });

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

  it("self-heals a failed session resolve without a manual refresh", async () => {
    // A backend deploy/restart leaves `tailscale serve` with no upstream for
    // a few seconds → resolveSession 502s. Before the self-heal, this one-shot
    // effect would never retry and the rail stayed sessionless until the user
    // refreshed. Now it must retry on a backoff and recover on its own.
    vi.mocked(chatApi.resolveSession)
      .mockRejectedValueOnce(new Error("502 Bad Gateway"))
      .mockResolvedValue({ session_id: "old-session", history: [] });

    renderRail();

    // First attempt fails.
    await waitFor(() =>
      expect(chatApi.resolveSession).toHaveBeenCalledTimes(1),
    );

    // Without any remount, the backoff retry fires (~500ms) and resolves.
    await waitFor(
      () => expect(chatApi.resolveSession).toHaveBeenCalledTimes(2),
      { timeout: 3000 },
    );
    expect(chatApi.resolveSession).toHaveBeenLastCalledWith(workspaceScope);
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

  it("aborts the active chat request when the desktop rail is collapsed", async () => {
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

    await waitFor(() => {
      expect(chatApi.streamChat).toHaveBeenCalled();
    });
    fireEvent.click(screen.getByTitle("Collapse rail"));

    await waitFor(() => {
      expect(capturedSignal?.aborted).toBe(true);
    });
  });

  it("aborts and ignores stale stream events when selecting another conversation", async () => {
    vi.mocked(chatApi.listSessions).mockResolvedValue([
      {
        id: "old-session",
        scope: workspaceScope,
        started_at: "2026-05-13T00:00:00Z",
        last_activity_at: "2026-05-13T00:05:00Z",
      },
      {
        id: "next-session",
        scope: workspaceScope,
        started_at: "2026-05-14T00:00:00Z",
        last_activity_at: "2026-05-14T00:05:00Z",
      },
    ]);
    vi.mocked(chatApi.loadSessionHistory).mockResolvedValue([
      {
        id: "m2",
        session_id: "next-session",
        seq: 0,
        role: "user",
        content_blocks: [{ type: "text", text: "selected question" }],
        ts: "2026-05-14T00:01:00Z",
      },
      {
        id: "m3",
        session_id: "next-session",
        seq: 1,
        role: "assistant",
        content_blocks: [{ type: "text", text: "selected answer" }],
        ts: "2026-05-14T00:02:00Z",
      },
    ]);
    let capturedSignal: AbortSignal | undefined;
    let releaseStream: ((ev: WizardEvent) => void) | undefined;
    vi.mocked(chatApi.streamChat).mockImplementation(async function* (
      _req,
      signal,
    ) {
      capturedSignal = signal;
      const ev = await new Promise<WizardEvent>((resolve) => {
        releaseStream = resolve;
      });
      yield ev;
    });
    renderRail();

    const composer = await screen.findByPlaceholderText(
      /ask anything about your workspace/i,
    );
    fireEvent.change(composer, {
      target: { value: "start long request" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send message" }));

    await waitFor(() => {
      expect(chatApi.streamChat).toHaveBeenCalled();
    });
    fireEvent.click(await screen.findByTestId("chat-history-item-next-session"));

    await waitFor(() => {
      expect(chatApi.loadSessionHistory).toHaveBeenCalledWith("next-session");
    });
    expect(capturedSignal?.aborted).toBe(true);
    expect(await screen.findByText("selected answer")).toBeInTheDocument();

    await act(async () => {
      releaseStream?.({ type: "token", text: "late token" });
    });

    expect(screen.queryByText(/late token/)).not.toBeInTheDocument();
  });

  /**
   * Regression — `chat_session_missing` self-heal (2026-05-26 QA).
   *
   * After a workspace reset / factory reset / fresh deploy, the rail
   * still holds the prior session id in component state. Pre-fix, the
   * next send POSTed against the dead id, got a generic 404, and
   * surfaced "chat session not found" to the operator with no recovery
   * path. The backend now emits a typed `chat_session_missing` code on
   * 404; the rail catches it, resolves a fresh session for the current
   * scope, and retries the message once. The test asserts both halves:
   * resolveSession is called for recovery AND the retry hits the new
   * session id.
   */
  it("self-heals when the backend reports chat_session_missing on send", async () => {
    const { ApiError } = await import("@/api/client");
    let resolveCallCount = 0;
    vi.mocked(chatApi.resolveSession).mockImplementation(async () => {
      resolveCallCount += 1;
      return {
        session_id:
          resolveCallCount === 1 ? "stale-session" : "fresh-session-after-reset",
        history: [],
      };
    });
    const seenSessions: string[] = [];
    vi.mocked(chatApi.streamChat).mockImplementation(async function* (req) {
      seenSessions.push(req.session_id);
      if (req.session_id === "stale-session") {
        throw new ApiError(
          404,
          "chat_session_missing",
          "chat session 'stale-session' no longer exists",
        );
      }
      yield { type: "token", text: "ok after recovery" };
    });

    renderRail();
    const composer = await screen.findByPlaceholderText(
      /ask anything about your workspace/i,
    );
    fireEvent.change(composer, { target: { value: "first message after reset" } });
    fireEvent.click(screen.getByRole("button", { name: "Send message" }));

    await waitFor(() => {
      expect(seenSessions).toEqual(["stale-session", "fresh-session-after-reset"]);
    });
    expect(resolveCallCount).toBeGreaterThanOrEqual(2);
    expect(await screen.findByText("ok after recovery")).toBeInTheDocument();
  });

  /**
   * Regression — Act-mode override of pending composer text
   * (2026-05-26 QA item #2). Pre-fix, switching to Act with text
   * already typed in the composer would send the hardcoded
   * "Continue in Act mode." over the top of the operator's intent
   * whenever any blocked tool call was visible in the thread.
   */
  it("submits the pending composer text when switching to Act mode", async () => {
    vi.mocked(chatApi.setSessionMode).mockResolvedValue({
      session_id: "old-session",
      mode: "act",
    });
    const seenMessages: string[] = [];
    vi.mocked(chatApi.streamChat).mockImplementation(async function* (req) {
      seenMessages.push(req.message);
      yield { type: "token", text: "act ack" };
    });

    renderRail();
    const composer = await screen.findByPlaceholderText(
      /ask anything about your workspace/i,
    );
    fireEvent.change(composer, {
      target: { value: "do the thing now" },
    });
    // Switch the rail into Act mode (button is labeled "Act" in the
    // RailModelBar mode toggle). When the operator has unsent
    // composer text, that text is what should be sent — never the
    // hardcoded continuation prompt.
    fireEvent.click(await screen.findByRole("button", { name: /^Act$/i }));

    await waitFor(() => {
      expect(seenMessages).toContain("do the thing now");
    });
    expect(seenMessages).not.toContain("Continue in Act mode.");
  });

  /**
   * Regression — provider/model auto-pick tiebreak (2026-05-26 QA
   * item #9). Pre-fix, the rail auto-picked `candidates[0]` from the
   * providers list, which made the catalog order load-bearing and
   * silently picked OpenRouter+deepseek-v4-pro over a workspace
   * default the operator had explicitly set elsewhere. The chat
   * dispatch then ran on the wrong model, the wizard's
   * `resolve_agent_runtime` (silently) inherited that wrong model
   * for every spawned strategy agent, and the assistant
   * synthesized the long "no Gemini models on OpenRouter"
   * explanation that confused multiple QA cycles.
   *
   * After the fix, the workspace default (`is_default: true` +
   * matching `default_model`) wins over catalog order. The first
   * candidate fallback is reserved for the case where no provider
   * is marked default at all.
   */
  it("prefers the workspace-default provider over catalog order when auto-picking", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        {
          name: "openrouter",
          kind: "openai-compat",
          base_url: "https://openrouter.ai/api/v1",
          api_key_env: "OPENROUTER_API_KEY",
          api_key_set: true,
          synthetic: false,
          is_default: false,
          enabled_models: ["deepseek/deepseek-v4-pro"],
        },
        {
          name: "google",
          kind: "openai-compat",
          base_url: "https://generativelanguage.googleapis.com/v1beta",
          api_key_env: "GOOGLE_API_KEY",
          api_key_set: true,
          synthetic: false,
          is_default: true,
          enabled_models: ["gemini-2.5-flash"],
        },
      ],
      default_model: "gemini-2.5-flash",
    });
    const seenDispatches: Array<{ provider?: string; model?: string }> = [];
    vi.mocked(chatApi.streamChat).mockImplementation(async function* (req) {
      seenDispatches.push({ provider: req.provider, model: req.model });
      yield { type: "token", text: "hi" };
    });

    renderRail();
    const composer = await screen.findByPlaceholderText(
      /ask anything about your workspace/i,
    );
    fireEvent.change(composer, { target: { value: "ping" } });
    fireEvent.click(screen.getByRole("button", { name: "Send message" }));

    await waitFor(() => {
      expect(seenDispatches.length).toBeGreaterThan(0);
    });
    expect(
      await screen.findByText("Workspace default: google / gemini-2.5-flash"),
    ).toBeInTheDocument();
    // The workspace-default provider (google) wins even though
    // openrouter appears first in the catalog array.
    expect(seenDispatches[0]).toEqual({
      provider: "google",
      model: "gemini-2.5-flash",
    });
  });

  /**
   * Regression — no-auth providers (e.g. Ollama, where api_key_env=""
   * and api_key_set=false) must appear in the model picker. Pre-fix,
   * the candidates filter required api_key_set=true, which silently
   * excluded every local endpoint that needs no key.
   */
  it("includes a no-auth Ollama provider in the model picker", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        {
          name: "ollama",
          kind: "openai-compat",
          base_url: "http://localhost:11434/v1",
          api_key_env: "",
          api_key_set: false,
          synthetic: false,
          is_default: false,
          enabled_models: ["llama3"],
        },
      ],
      default_model: null,
    });

    renderRail();

    // Open the Signal model dropdown, then assert the no-auth model is listed.
    const trigger = await screen.findByRole("button", { name: /model/i });
    fireEvent.click(trigger);
    expect(
      await screen.findByRole("option", { name: /llama3/ }),
    ).toBeInTheDocument();
  });

  it("does not dispatch stale stored chat picker values absent from the picker", async () => {
    localStorage.setItem("xvn.chat_rail.provider", "openrouter");
    localStorage.setItem("xvn.chat_rail.model", "stale/model");
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        {
          name: "ollama",
          kind: "openai-compat",
          base_url: "http://localhost:11434/v1",
          api_key_env: "",
          api_key_set: false,
          synthetic: false,
          is_default: false,
          enabled_models: ["qwen2.5-coder:7b"],
        },
      ],
      default_model: null,
    });
    const seenDispatches: Array<{ provider?: string; model?: string }> = [];
    vi.mocked(chatApi.streamChat).mockImplementation(async function* (req) {
      seenDispatches.push({ provider: req.provider, model: req.model });
      yield { type: "token", text: "ok" };
    });

    renderRail();

    expect(
      await screen.findByText("ollama / qwen2.5-coder:7b"),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(localStorage.getItem("xvn.chat_rail.provider")).toBe("ollama");
      expect(localStorage.getItem("xvn.chat_rail.model")).toBe(
        "qwen2.5-coder:7b",
      );
    });
    const composer = await screen.findByPlaceholderText(
      /ask anything about your workspace/i,
    );
    fireEvent.change(composer, { target: { value: "use visible model" } });
    fireEvent.click(screen.getByRole("button", { name: "Send message" }));

    await waitFor(() => {
      expect(seenDispatches).toEqual([
        { provider: "ollama", model: "qwen2.5-coder:7b" },
      ]);
    });
  });

  /**
   * Regression — mergeUnifiedRows must NOT use `users.length` as the
   * unanchored-user fallback (wrong unit — user count vs assistant
   * count). The new fallback is `projectedAssistantCount`, which
   * pushes unanchored bubbles past the entire projection so they
   * never visually overlap an already-rendered assistant row.
   */
  it("places unanchored user bubbles after every projected assistant row", () => {
    const rows: MessageRow[] = [
      {
        type: "assistant",
        id: "a1",
        seq: 1,
        streamId: "s",
        appliedEventIds: new Set(["e1"]),
        actor: "agent",
        text: "first",
        blocks: [],
        done: true,
        draftId: null,
        messageIndex: 0,
      },
      {
        type: "assistant",
        id: "a2",
        seq: 2,
        streamId: "s",
        appliedEventIds: new Set(["e2"]),
        actor: "agent",
        text: "second",
        blocks: [],
        done: true,
        draftId: null,
        messageIndex: 0,
      },
    ];
    const merged = mergeUnifiedRows(
      // Legacy bubble without `assistantAnchor` set (mimics a
      // hand-built fixture or a pre-anchor snapshot reloaded from
      // disk). Pre-fix, this user would have anchor=0 (users.length
      // at that point) and end up sorted to the FRONT — visually
      // before the projected assistants, which the rail's column
      // layout would render on top of the first assistant bubble.
      [{ role: "user", text: "unanchored question" }],
      rows,
    );
    expect(merged.map((b) => b.role)).toEqual([
      "assistant",
      "assistant",
      "user",
    ]);
  });

  /**
   * Regression for the "user message appears above the agent message" bug.
   *
   * Repro: an existing session with one historical assistant in `bubbles`
   * (hydrated synchronously from `resolveSession.history`) and a unified
   * SSE replay that hasn't caught up yet (`unifiedRows` still empty).
   * Pre-fix, `send` derived the new user's `assistantAnchor` solely from
   * `unifiedRows`, so it stamped anchor=0 — the SAME anchor we use for
   * "user spoke before any assistant existed." Once the SSE replay
   * landed, `mergeUnifiedRows` then dutifully sorted that user bubble
   * BEFORE the historical assistant row.
   */
  it("anchors new user past assistants already in bubbles even when unified is empty", () => {
    const bubbles: Bubble[] = [
      {
        role: "assistant",
        blocks: [{ kind: "text", text: "historical answer" }],
        tools: [],
      },
    ];
    const unifiedRows: MessageRow[] = [];
    expect(computeUserAnchor(bubbles, unifiedRows)).toBe(1);
  });

  it("anchors new user past projected assistants when unified leads bubbles", () => {
    const bubbles: Bubble[] = [];
    const unifiedRows: MessageRow[] = [
      {
        type: "assistant",
        id: "a1",
        seq: 1,
        streamId: "s",
        appliedEventIds: new Set(["e1"]),
        actor: "agent",
        text: "fanned-out step 1",
        blocks: [],
        done: true,
        draftId: null,
        messageIndex: 0,
      },
      {
        type: "assistant",
        id: "a2",
        seq: 2,
        streamId: "s",
        appliedEventIds: new Set(["e2"]),
        actor: "agent",
        text: "fanned-out step 2",
        blocks: [],
        done: true,
        draftId: null,
        messageIndex: 1,
      },
    ];
    expect(computeUserAnchor(bubbles, unifiedRows)).toBe(2);
  });

  it("anchors to zero when neither bubbles nor unified rows have an assistant", () => {
    expect(computeUserAnchor([], [])).toBe(0);
  });

  it("places the new user after the historical assistant once unified catches up", () => {
    // Full bug scenario: bubbles started with a historical assistant,
    // user sent a message during the SSE replay window, and the
    // unified projection then caught up to that same assistant. With
    // the corrected anchor (1), the merge must produce
    // [historical assistant, new user, optimistic placeholder] — never
    // [new user, historical assistant, ...].
    const bubblesAfterSend: Bubble[] = [
      {
        role: "assistant",
        blocks: [{ kind: "text", text: "historical answer" }],
        tools: [],
      },
      { role: "user", text: "new question", assistantAnchor: 1 },
      { role: "assistant", blocks: [{ kind: "text", text: "" }], tools: [] },
    ];
    const unifiedRowsAfterReplay: MessageRow[] = [
      {
        type: "assistant",
        id: "a1",
        seq: 1,
        streamId: "s",
        appliedEventIds: new Set(["e1"]),
        actor: "agent",
        text: "historical answer",
        blocks: [],
        done: true,
        draftId: null,
        messageIndex: 0,
      },
    ];
    const merged = mergeUnifiedRows(bubblesAfterSend, unifiedRowsAfterReplay);
    expect(merged.map((b) => b.role)).toEqual([
      "assistant",
      "user",
      "assistant",
    ]);
  });

  it("renders historical tool results with error:null as successful", async () => {
    vi.mocked(chatApi.resolveSession).mockResolvedValue({
      session_id: "old-session",
      history: [
        {
          id: "m1",
          session_id: "old-session",
          seq: 0,
          role: "assistant",
          content_blocks: [
            {
              type: "tool_use",
              id: "tool-1",
              name: "create_strategy",
              input: { name: "Alpha", template: "momentum" },
            },
          ],
          ts: "2026-05-13T00:01:00Z",
        },
        {
          id: "m2",
          session_id: "old-session",
          seq: 1,
          role: "user",
          content_blocks: [
            {
              type: "tool_result",
              tool_use_id: "tool-1",
              content: JSON.stringify({ id: "01OK", error: null }),
            },
          ],
          ts: "2026-05-13T00:02:00Z",
        },
      ],
    });

    renderRail();

    expect(await screen.findByText(/01OK/)).toBeInTheDocument();
    expect(screen.queryByText(/Create strategy failed/i)).not.toBeInTheDocument();
  });

  /**
   * Context switcher (2026-05-28). Header shows a dropdown letting the
   * operator pick "Active page" (scopeFromPath result) or "Whole
   * workspace" ({ scope: "workspace" }). Defaults to "Active page" and
   * persists via localStorage.
   */
  describe("context switcher", () => {
    const routeScope = { scope: "route" as const, route: "/strategies" };

    it("defaults to 'Active page' and resolves with the scopeFromPath result", async () => {
      vi.mocked(chatApi.scopeFromPath).mockReturnValue(routeScope);
      renderRail("/strategies");

      expect(
        await screen.findByRole("button", { name: /Active page/i }),
      ).toBeInTheDocument();
      await waitFor(() => {
        expect(chatApi.resolveSession).toHaveBeenCalledWith(routeScope);
      });
    });

    it("uses workspace scope when 'Whole workspace' is selected on a non-workspace route", async () => {
      vi.mocked(chatApi.scopeFromPath).mockReturnValue(routeScope);
      renderRail("/strategies");

      await waitFor(() => {
        expect(chatApi.resolveSession).toHaveBeenCalledWith(routeScope);
      });
      vi.mocked(chatApi.resolveSession).mockClear();

      fireEvent.click(
        await screen.findByRole("button", { name: /Active page/i }),
      );
      fireEvent.click(
        await screen.findByRole("menuitemradio", { name: /Whole workspace/i }),
      );

      await waitFor(() => {
        expect(chatApi.resolveSession).toHaveBeenCalledWith(workspaceScope);
      });
      expect(localStorage.getItem("xvn.chat_rail.context_mode")).toBe(
        "workspace",
      );
    });

    it("restores the persisted selection on remount", async () => {
      vi.mocked(chatApi.scopeFromPath).mockReturnValue(routeScope);
      localStorage.setItem("xvn.chat_rail.context_mode", "workspace");

      renderRail("/strategies");

      expect(
        await screen.findByRole("button", { name: /Whole workspace/i }),
      ).toBeInTheDocument();
      await waitFor(() => {
        expect(chatApi.resolveSession).toHaveBeenCalledWith(workspaceScope);
      });
    });
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
