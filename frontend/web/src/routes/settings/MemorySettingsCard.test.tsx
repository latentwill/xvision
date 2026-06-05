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
    namespaces: [{ namespace: "chat", live_observations: 12n }],
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
  it("renders the embedder select with Off/Local/Auto + mocked provider option", async () => {
    const { findByLabelText } = renderCard();
    const select = (await findByLabelText("Embedder source")) as HTMLSelectElement;
    // The provider option is appended once listProviders resolves.
    await waitFor(() => {
      const values = Array.from(select.options).map((o) => o.value);
      expect(values).toContain("openai");
    });
    const optionValues = Array.from(select.options).map((o) => o.value);
    expect(optionValues).toContain("off");
    expect(optionValues).toContain("local");
    expect(optionValues).toContain("auto");
    // Pre-selected from the mocked report.
    expect(select.value).toBe("auto");
  });

  it("calls updateMemorySettings with the chosen embedder on change", async () => {
    const { findByLabelText } = renderCard();
    const select = await findByLabelText("Embedder source");
    fireEvent.change(select, { target: { value: "local" } });
    await waitFor(() => {
      expect(settingsApi.updateMemorySettings).toHaveBeenCalledWith({
        embedder: "local",
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

  it("renders the embedding-model picker with curated options when source is not off", async () => {
    const { findByLabelText } = renderCard();
    const modelSelect = (await findByLabelText(
      "Embedding model",
    )) as HTMLSelectElement;
    const values = Array.from(modelSelect.options).map((o) => o.value);
    expect(values).toContain("nomic-embed-text");
    expect(values).toContain("qwen3-embedding");
    // "Provider default" (empty) and a Custom escape hatch are offered.
    expect(values).toContain("");
    expect(values).toContain("__custom__");
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
    fireEvent.change(modelSelect, { target: { value: "nomic-embed-text" } });
    await waitFor(() => {
      expect(settingsApi.updateMemorySettings).toHaveBeenCalledWith({
        embedder: null,
        chat_enabled: null,
        optimizer_enabled: null,
        embedder_model: "nomic-embed-text",
        embedder_base_url: null,
      });
    });
  });

  it("offers a Custom endpoint option in the embedder source select", async () => {
    const { findByLabelText } = renderCard();
    const select = (await findByLabelText(
      "Embedder source",
    )) as HTMLSelectElement;
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).toContain("custom");
  });

  it("hides the custom base-URL input for non-custom sources", async () => {
    const { findByLabelText, queryByLabelText } = renderCard();
    await findByLabelText("Embedder source");
    // Source is "auto" in the mocked report → no custom base-URL input.
    expect(queryByLabelText("Custom endpoint base URL")).toBeNull();
  });

  it("reveals the custom base-URL input when the source is custom", async () => {
    vi.mocked(settingsApi.getMemorySettings).mockResolvedValue(
      memoryReport({ embedder: "custom", embedder_base_url: null }),
    );
    const { findByLabelText } = renderCard();
    const input = (await findByLabelText(
      "Custom endpoint base URL",
    )) as HTMLInputElement;
    expect(input).toBeInTheDocument();
  });

  it("selecting Custom endpoint sends embedder=custom", async () => {
    const { findByLabelText } = renderCard();
    const select = await findByLabelText("Embedder source");
    fireEvent.change(select, { target: { value: "custom" } });
    await waitFor(() => {
      expect(settingsApi.updateMemorySettings).toHaveBeenCalledWith({
        embedder: "custom",
        chat_enabled: null,
        optimizer_enabled: null,
        embedder_model: null,
        embedder_base_url: null,
      });
    });
  });

  it("typing a base URL sends embedder + embedder_base_url", async () => {
    vi.mocked(settingsApi.getMemorySettings).mockResolvedValue(
      memoryReport({ embedder: "custom", embedder_base_url: null }),
    );
    const { findByLabelText } = renderCard();
    const input = (await findByLabelText(
      "Custom endpoint base URL",
    )) as HTMLInputElement;
    fireEvent.change(input, {
      target: { value: "http://localhost:11434/v1" },
    });
    fireEvent.blur(input);
    await waitFor(() => {
      expect(settingsApi.updateMemorySettings).toHaveBeenCalledWith({
        embedder: "custom",
        embedder_base_url: "http://localhost:11434/v1",
        chat_enabled: null,
        optimizer_enabled: null,
        embedder_model: null,
      });
    });
  });

  it("reveals a custom model input that also sends embedder_model", async () => {
    const { findByLabelText, queryByLabelText } = renderCard();
    const modelSelect = await findByLabelText("Embedding model");
    // No custom input until "Custom…" is selected.
    expect(queryByLabelText("Custom embedding model")).toBeNull();
    fireEvent.change(modelSelect, { target: { value: "__custom__" } });
    const customInput = (await findByLabelText(
      "Custom embedding model",
    )) as HTMLInputElement;
    fireEvent.change(customInput, { target: { value: "my-local-embed" } });
    fireEvent.blur(customInput);
    await waitFor(() => {
      expect(settingsApi.updateMemorySettings).toHaveBeenCalledWith({
        embedder: null,
        chat_enabled: null,
        optimizer_enabled: null,
        embedder_model: "my-local-embed",
        embedder_base_url: null,
      });
    });
  });
});
