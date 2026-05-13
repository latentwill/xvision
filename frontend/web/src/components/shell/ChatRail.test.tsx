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
  localStorage.setItem("xvn.chat_rail.open", "1");
  return render(
    <MemoryRouter initialEntries={[path]}>
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <ChatRail />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

const routeScope = { scope: "route", route: "/strategies" } as const;

beforeEach(() => {
  localStorage.clear();
  vi.mocked(settingsApi.listProviders).mockResolvedValue({ providers: [] });
  vi.mocked(chatApi.listSessions).mockResolvedValue([
    {
      id: "old-session",
      scope: routeScope,
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
  cleanup();
  vi.restoreAllMocks();
});

describe("ChatRail", () => {
  it("creates a new chat without deleting the previous conversation", async () => {
    renderRail();

    expect(await screen.findByText("previous question")).toBeInTheDocument();
    const composer = screen.getByPlaceholderText(/ask about this page/i);
    fireEvent.change(composer, {
      target: { value: "draft text" },
    });

    fireEvent.click(screen.getByRole("button", { name: "New chat" }));

    await waitFor(() => {
      expect(chatApi.createSession).toHaveBeenCalledWith(routeScope);
    });
    expect(chatApi.deleteSession).not.toHaveBeenCalled();
    expect(screen.getByPlaceholderText(/ask about this page/i)).toHaveValue("");
    expect(await screen.findByText(/No messages yet/i)).toBeInTheDocument();
  });
});
