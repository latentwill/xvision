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

import { ChatRail } from "./ChatRail";
import * as chatApi from "@/api/chat_rail";
import * as settingsApi from "@/api/settings";

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
