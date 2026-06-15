import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { SettingsBrokersRoute } from "./index";
import * as settingsApi from "@/api/settings";
import type { BrokerEntry, BrokersReport } from "@/api/types.gen";

// `<ResponsiveListCard>`/viewport hooks read `window.matchMedia`; jsdom lacks
// it. Install a desktop stub (same pattern as providers.test.tsx).
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

vi.mock("@/api/settings", async () => {
  const actual =
    await vi.importActual<typeof import("@/api/settings")>("@/api/settings");
  return {
    ...actual,
    getBrokers: vi.fn(),
    setDegenArenaCredentials: vi.fn(),
    clearDegenArenaCredentials: vi.fn(),
  };
});

function broker(overrides: Partial<BrokerEntry> = {}): BrokerEntry {
  return {
    name: "Alpaca",
    kind: "alpaca",
    credentials: [],
    configured: false,
    stored: false,
    stored_key_id_suffix: null,
    base_url: null,
    note: null,
    ...overrides,
  };
}

function mockBrokers(degenOverrides: Partial<BrokerEntry> = {}) {
  const report: BrokersReport = {
    alpaca: broker({ name: "Alpaca", kind: "alpaca" }),
    orderly: broker({ name: "Orderly Network", kind: "orderly" }),
    byreal: broker({ name: "Byreal", kind: "byreal" }),
    degen_arena: broker({
      name: "Degen Arena",
      kind: "degen_arena",
      note: "Virtuals Degen Arena (Hyperliquid perps).",
      ...degenOverrides,
    }),
    hyperliquid: broker({ name: "Hyperliquid", kind: "hyperliquid" }),
  };
  vi.mocked(settingsApi.getBrokers).mockResolvedValue(report);
}

function renderRoute() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <SettingsBrokersRoute />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

const VALID_KEY = `0x${"a".repeat(64)}`;
const VALID_ADDR = `0x${"b".repeat(40)}`;

describe("DegenArenaBrokerCard", () => {
  beforeEach(() => {
    stubMatchMediaDesktop();
    vi.clearAllMocks();
  });
  afterEach(() => cleanup());

  it("renders a Degen Arena card with the key entry form when not configured", async () => {
    mockBrokers();
    renderRoute();

    expect(await screen.findByText("Degen Arena")).toBeInTheDocument();
    // The form fields that were previously absent from Settings entirely.
    expect(screen.getByLabelText("Trade-only HL API key")).toBeInTheDocument();
    expect(screen.getByLabelText("Account address")).toBeInTheDocument();
    expect(screen.getByLabelText("Network")).toBeInTheDocument();
  });

  it("saves valid credentials to the deploy route", async () => {
    mockBrokers();
    vi.mocked(settingsApi.setDegenArenaCredentials).mockResolvedValue({
      ok: true,
      stored_key_suffix: "aaaa",
      network: "testnet",
    });
    renderRoute();
    await screen.findByText("Degen Arena");

    const keyInput = screen.getByLabelText("Trade-only HL API key");
    fireEvent.change(keyInput, { target: { value: VALID_KEY } });
    fireEvent.change(screen.getByLabelText("Account address"), {
      target: { value: VALID_ADDR },
    });
    // Scope to the Degen Arena form — the Alpaca/Byreal cards also render "Save".
    const form = keyInput.closest("form") as HTMLFormElement;
    fireEvent.click(within(form).getByRole("button", { name: /^Save$/ }));

    await waitFor(() =>
      expect(vi.mocked(settingsApi.setDegenArenaCredentials)).toHaveBeenCalledWith(
        { apiKey: VALID_KEY, accountAddress: VALID_ADDR, network: "testnet" },
      ),
    );
  });

  it("rejects a malformed key inline without calling the API", async () => {
    mockBrokers();
    renderRoute();
    await screen.findByText("Degen Arena");

    const keyInput = screen.getByLabelText("Trade-only HL API key");
    fireEvent.change(keyInput, { target: { value: "not-a-key" } });
    fireEvent.change(screen.getByLabelText("Account address"), {
      target: { value: VALID_ADDR },
    });
    const form = keyInput.closest("form") as HTMLFormElement;
    fireEvent.click(within(form).getByRole("button", { name: /^Save$/ }));

    expect(await screen.findByText(/0x \+ 64 hex/)).toBeInTheDocument();
    expect(
      vi.mocked(settingsApi.setDegenArenaCredentials),
    ).not.toHaveBeenCalled();
  });

  it("shows the stored suffix and clears when configured", async () => {
    mockBrokers({
      configured: true,
      stored: true,
      stored_key_id_suffix: "9f3c",
      base_url: "testnet",
    });
    vi.mocked(settingsApi.clearDegenArenaCredentials).mockResolvedValue();
    renderRoute();
    await screen.findByText("Degen Arena");

    expect(screen.getByText(/9f3c/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /clear/i }));

    await waitFor(() =>
      expect(
        vi.mocked(settingsApi.clearDegenArenaCredentials),
      ).toHaveBeenCalled(),
    );
  });
});
