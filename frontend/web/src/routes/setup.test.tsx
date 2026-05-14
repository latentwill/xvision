import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { SetupRoute } from "./setup";
import { listProviders } from "@/api/settings";
import { streamChat } from "@/api/chat_rail";

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
