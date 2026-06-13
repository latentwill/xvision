import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

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
  it("renders the embedder select with Off/Auto + mocked OpenAI provider option", async () => {
    const { findByLabelText } = renderCard();
    const select = (await findByLabelText("Embedder source")) as HTMLSelectElement;
    // The provider option is appended once listProviders resolves.
    await waitFor(() => {
      const values = Array.from(select.options).map((o) => o.value);
      expect(values).toContain("openai");
    });
    const optionValues = Array.from(select.options).map((o) => o.value);
    expect(optionValues).toContain("off");
    expect(optionValues).toContain("auto");
    // Pre-selected from the mocked report.
    expect(select.value).toBe("auto");
  });

  it("limits memory embedding controls to OpenAI-backed choices", async () => {
    const { findByLabelText, findByText, queryByText } = renderCard();
    const select = (await findByLabelText("Embedder source")) as HTMLSelectElement;
    await findByText(/Memory currently uses OpenAI embedding providers only/i);

    const labels = Array.from(select.options).map((o) => o.textContent ?? "");
    expect(labels).toContain("Off");
    expect(labels).toContain("Auto (OpenAI embeddings)");
    expect(labels).toContain("openai (OpenAI)");
    expect(labels).not.toContain("Local (offline, lexical)");
    expect(labels).not.toContain("Custom endpoint (OpenAI-compatible)");
    expect(queryByText(/Ollama/i)).not.toBeInTheDocument();
  });

  it("calls updateMemorySettings with the chosen embedder on change", async () => {
    const { findByLabelText } = renderCard();
    const select = (await findByLabelText("Embedder source")) as HTMLSelectElement;
    await waitFor(() => {
      const values = Array.from(select.options).map((o) => o.value);
      expect(values).toContain("openai");
    });
    fireEvent.change(select, { target: { value: "openai" } });
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
    const { findByLabelText } = renderCard();
    const modelSelect = (await findByLabelText(
      "Embedding model",
    )) as HTMLSelectElement;
    const values = Array.from(modelSelect.options).map((o) => o.value);
    expect(values).toContain("text-embedding-3-small");
    expect(values).toContain("text-embedding-3-large");
    // "Provider default" (empty) is offered.
    expect(values).toContain("");
    expect(values).not.toContain("__custom__");
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
    const { findByLabelText } = renderCard();
    const modelSelect = await findByLabelText("Embedding model");
    fireEvent.change(modelSelect, { target: { value: "text-embedding-3-large" } });
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
    const { findByLabelText, queryByLabelText } = renderCard();
    const select = (await findByLabelText(
      "Embedder source",
    )) as HTMLSelectElement;
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).not.toContain("custom");
    expect(values).not.toContain("local");
    expect(queryByLabelText("Custom endpoint base URL")).toBeNull();
    expect(queryByLabelText("Custom embedding model")).toBeNull();
  });
});
