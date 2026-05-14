import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { SetupRoute } from "./setup";

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
});
