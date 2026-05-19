import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { SetupRoute } from "./setup";
import { listProviders } from "@/api/settings";
import { resolveSession, streamChat } from "@/api/chat_rail";

vi.mock("@/api/settings", () => ({
  settingsKeys: {
    providers: () => ["settings", "providers"],
  },
  listProviders: vi.fn().mockResolvedValue({
    providers: [
      {
        name: "openrouter",
        enabled_models: ["anthropic/claude-sonnet-4"],
        api_key_set: true,
        synthetic: false,
      },
    ],
  }),
}));

vi.mock("@/api/chat_rail", () => ({
  resolveSession: vi.fn().mockResolvedValue({
    session_id: "setup-session",
    history: [],
  }),
  streamChat: vi.fn(),
}));

function renderRoute() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <SetupRoute />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("SetupRoute", () => {
  beforeEach(() => {
    vi.mocked(resolveSession).mockResolvedValue({
      session_id: "setup-session",
      history: [],
    });
    vi.mocked(listProviders).mockResolvedValue({
      providers: [
        {
          name: "openrouter",
          enabled_models: ["anthropic/claude-sonnet-4"],
          api_key_set: true,
          synthetic: false,
        },
      ],
    } as never);
    vi.mocked(streamChat).mockImplementation(async function* () {});
  });

  afterEach(() => {
    cleanup();
  });

  it("renders a multiline composer and enables send when text is entered", async () => {
    Object.defineProperty(HTMLElement.prototype, "scrollTo", {
      value: vi.fn(),
      configurable: true,
    });

    renderRoute();

    const composer = await screen.findByPlaceholderText(
      "Describe your strategy or ask the wizard…",
    );
    expect(composer.tagName).toBe("TEXTAREA");

    const send = screen.getByRole("button", { name: "Send" });
    expect(send).toBeDisabled();

    fireEvent.change(composer, { target: { value: "Trend-follow BTC on 4h." } });
    expect(send).toBeEnabled();
  });

  it("makes tool success the boundary for saved setup changes", async () => {
    renderRoute();

    expect(
      await screen.findByText(
        "Only completed tool calls change the saved draft. Open the Inspector to verify the manifest before eval.",
      ),
    ).toBeInTheDocument();
  });

  it("narrates failed history tools as failures even without string errors", async () => {
    vi.mocked(resolveSession).mockResolvedValue({
      session_id: "setup-session",
      history: [
        {
          id: "assistant-1",
          session_id: "setup-session",
          seq: 1,
          role: "assistant",
          ts: "2026-05-19T00:00:00.000Z",
          content_blocks: [
            {
              type: "tool_use",
              id: "tool-1",
              name: "some_tool",
              input: {},
            },
          ],
        },
        {
          id: "user-1",
          session_id: "setup-session",
          seq: 2,
          role: "user",
          ts: "2026-05-19T00:00:01.000Z",
          content_blocks: [
            {
              type: "tool_result",
              tool_use_id: "tool-1",
              content: JSON.stringify({
                error: { message: "permission denied" },
              }),
            },
          ],
        },
      ],
    } as never);

    renderRoute();

    expect(await screen.findByText("permission denied")).toBeInTheDocument();
    expect(screen.getByText(/some_tool failed:/)).toBeInTheDocument();
    expect(screen.queryByText(/some_tool complete/)).not.toBeInTheDocument();
  });

  it("blocks wizard sends when no configured provider model is available", async () => {
    vi.mocked(listProviders).mockResolvedValue({
      providers: [],
      default_model: null,
    } as never);

    renderRoute();

    const composer = await screen.findByPlaceholderText(
      "Describe your strategy or ask the wizard…",
    );
    fireEvent.change(composer, { target: { value: "Trend-follow BTC on 4h." } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    expect(
      await screen.findByText("Pick provider models in Settings → Providers before the wizard can run."),
    ).toBeInTheDocument();
    expect(streamChat).not.toHaveBeenCalled();
  });
});
