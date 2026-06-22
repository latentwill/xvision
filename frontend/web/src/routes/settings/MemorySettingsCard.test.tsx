import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import userEvent from "@testing-library/user-event";

import { MemorySettingsCard } from "./MemorySettingsCard";
import * as settingsApi from "@/api/settings";
import type {
  MemoryReport,
  MemoryStatus,
  ProvidersReport,
} from "@/api/types.gen";

vi.mock("@/api/settings", async () => {
  const actual = await vi.importActual<typeof import("@/api/settings")>(
    "@/api/settings",
  );
  return {
    ...actual,
    getMemorySettings: vi.fn(),
    getMemoryStatus: vi.fn(),
    updateMemorySettings: vi.fn(),
    listProviders: vi.fn(),
  };
});

function memoryReport(overrides: Partial<MemoryReport> = {}): MemoryReport {
  return {
    embedder: "auto",
    chat_enabled: true,
    optimizer_enabled: false,
    embedder_model: null,
    embedder_base_url: null,
    persisted: true,
    ...overrides,
  };
}

function memoryStatus(overrides: Partial<MemoryStatus> = {}): MemoryStatus {
  return {
    store_path: "/home/user/.xvn/memory.db",
    writable: true,
    embedder_present: true,
    embedder_id: "openai:text-embedding-3-small",
    embedder_source: "openai-compat",
    grace_days: 7,
    // ts-rs types this as bigint, but JSON.parse delivers a plain number at
    // runtime — the fixture must match the wire reality, not the type
    // (regression: a 0n accumulator over numbers crashed /settings/general).
    namespaces: [
      { namespace: "chat", live_observations: 12 as unknown as bigint },
    ],
    ...overrides,
  };
}

function providersReport(): ProvidersReport {
  return {
    providers: [
      {
        name: "openai",
        kind: "openai-compat",
        base_url: "https://api.openai.com/v1",
        api_key_env: "OPENAI_API_KEY",
        api_key_set: true,
        synthetic: false,
        is_default: true,
        enabled_models: ["gpt-4.1-mini"],
      },
    ],
    default_model: null,
    invalid: [],
  };
}

function renderCard() {
  return render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <MemorySettingsCard />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.mocked(settingsApi.getMemorySettings).mockResolvedValue(memoryReport());
  vi.mocked(settingsApi.getMemoryStatus).mockResolvedValue(memoryStatus());
  vi.mocked(settingsApi.listProviders).mockResolvedValue(providersReport());
  vi.mocked(settingsApi.updateMemorySettings).mockResolvedValue(memoryReport());
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("MemorySettingsCard", () => {
  it("renders the embedder menu with Off/Auto + mocked OpenAI provider option", async () => {
    const user = userEvent.setup();
    renderCard();
    const trigger = await screen.findByRole("button", { name: "Embedder source" });
    await waitFor(() => expect(trigger).toHaveTextContent("Auto (OpenAI embeddings)"));
    await user.click(trigger);
    expect(await screen.findByRole("option", { name: "openai (OpenAI)" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Off" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Auto (OpenAI embeddings)" })).toBeInTheDocument();
  });

  it("limits memory embedding controls to OpenAI-backed choices", async () => {
    const user = userEvent.setup();
    renderCard();
    await screen.findByText(/Memory currently uses OpenAI embedding providers only/i);

    await user.click(await screen.findByRole("button", { name: "Embedder source" }));
    expect(screen.getByRole("option", { name: "Off" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Auto (OpenAI embeddings)" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "openai (OpenAI)" })).toBeInTheDocument();
    expect(screen.queryByText("Local (offline, lexical)")).not.toBeInTheDocument();
    expect(screen.queryByText("Custom endpoint (OpenAI-compatible)")).not.toBeInTheDocument();
    expect(screen.queryByText(/Ollama/i)).not.toBeInTheDocument();
  });

  it("calls updateMemorySettings with the chosen embedder on change", async () => {
    const user = userEvent.setup();
    renderCard();
    const trigger = await screen.findByRole("button", { name: "Embedder source" });
    await user.click(trigger);
    await user.click(await screen.findByRole("option", { name: "openai (OpenAI)" }));
    await waitFor(() => {
      expect(settingsApi.updateMemorySettings).toHaveBeenCalledWith({
        embedder: "openai",
        chat_enabled: null,
        optimizer_enabled: null,
        embedder_model: null,
        embedder_base_url: null,
      });
    });
  });

  it("calls updateMemorySettings with chat_enabled when toggling Chat memory", async () => {
    const { findByLabelText } = renderCard();
    // mocked report has chat_enabled: true → toggling sends false.
    const chat = (await findByLabelText("Chat memory")) as HTMLInputElement;
    // Wait for the loaded state (checked) before clicking, otherwise we
    // click the pre-load unchecked box and send the wrong value.
    await waitFor(() => expect(chat.checked).toBe(true));
    fireEvent.click(chat);
    await waitFor(() => {
      expect(settingsApi.updateMemorySettings).toHaveBeenCalledWith({
        embedder: null,
        chat_enabled: false,
        optimizer_enabled: null,
        embedder_model: null,
        embedder_base_url: null,
      });
    });
  });

  it("renders the resolved embedder id from the status query", async () => {
    const { findByText } = renderCard();
    expect(
      await findByText(/openai:text-embedding-3-small/),
    ).toBeInTheDocument();
  });

  it("sums live observations from plain-number JSON without throwing", async () => {
    // Regression: the wire payload carries numbers (despite the bigint type),
    // and a BigInt accumulator crashed the whole /settings/general route.
    vi.mocked(settingsApi.getMemoryStatus).mockResolvedValue(
      memoryStatus({
        namespaces: [
          { namespace: "chat", live_observations: 7 as unknown as bigint },
          { namespace: "optimizer", live_observations: 5 as unknown as bigint },
        ],
      }),
    );
    const { findByText } = renderCard();
    expect(await findByText("12")).toBeInTheDocument();
  });

  it("renders the embedding-model picker with curated options when source is not off", async () => {
    const user = userEvent.setup();
    renderCard();
    const modelTrigger = await screen.findByRole("button", { name: "Embedding model" });
    await user.click(modelTrigger);
    expect(screen.getByRole("option", { name: "text-embedding-3-small" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "text-embedding-3-large" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Provider default (OpenAI)" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "__custom__" })).not.toBeInTheDocument();
  });

  it("does not render the model picker when embedder source is off", async () => {
    vi.mocked(settingsApi.getMemorySettings).mockResolvedValue(
      memoryReport({ embedder: "off" }),
    );
    const { findByLabelText, queryByLabelText } = renderCard();
    // Embedder source select is still there.
    await findByLabelText("Embedder source");
    expect(queryByLabelText("Embedding model")).toBeNull();
  });

  it("calls updateMemorySettings with the chosen model on change", async () => {
    const user = userEvent.setup();
    renderCard();
    await user.click(await screen.findByRole("button", { name: "Embedding model" }));
    await user.click(await screen.findByRole("option", { name: "text-embedding-3-large" }));
    await waitFor(() => {
      expect(settingsApi.updateMemorySettings).toHaveBeenCalledWith({
        embedder: null,
        chat_enabled: null,
        optimizer_enabled: null,
        embedder_model: "text-embedding-3-large",
        embedder_base_url: null,
      });
    });
  });

  it("does not offer custom endpoints or non-OpenAI model controls", async () => {
    const user = userEvent.setup();
    renderCard();
    await user.click(await screen.findByRole("button", { name: "Embedder source" }));
    expect(screen.queryByRole("option", { name: /custom/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /local/i })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Custom endpoint base URL")).toBeNull();
    expect(screen.queryByLabelText("Custom embedding model")).toBeNull();
  });
});
