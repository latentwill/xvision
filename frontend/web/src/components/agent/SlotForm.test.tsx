// Focused tests for `<SlotForm>` provider/model interaction —
// specifically the `changeProvider` invariant that clears
// `slot.model` when the new provider doesn't enable the current
// model. Closes clawpatch B-10
// (`fnd_sig-feat-ui-flow-0e07bcd326-2bbe_8ce24d101a`).

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { SlotForm } from "./SlotForm";
import type { AgentSlot } from "@/api/agents";
import type { ProviderRow } from "@/api/types.gen";

vi.mock("@/api/settings", async () => {
  const actual = await vi.importActual<typeof import("@/api/settings")>(
    "@/api/settings",
  );
  return {
    ...actual,
    listProviders: vi.fn(),
  };
});

const settingsApi = await import("@/api/settings");

function row(
  name: string,
  kind: "anthropic" | "openai-compat",
  enabled: string[],
): ProviderRow {
  return {
    name,
    kind,
    base_url: kind === "anthropic" ? "https://api.anthropic.com" : "https://api.openai.com",
    api_key_env: `${name.toUpperCase()}_API_KEY`,
    enabled_models: enabled,
  } as ProviderRow;
}

function makeSlot(overrides: Partial<AgentSlot> = {}): AgentSlot {
  return {
    name: "trader",
    provider: "anthropic",
    model: "claude-sonnet-4-6",
    system_prompt: "you are a trader",
    skill_ids: [],
    max_tokens: null,
    ...overrides,
  };
}

function renderSlot({
  slot,
  onChange,
}: {
  slot: AgentSlot;
  onChange: (next: AgentSlot) => void;
}) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <SlotForm
        slot={slot}
        onChange={onChange}
        onRemove={() => {}}
        onDuplicate={() => {}}
        canRemove={false}
        index={0}
      />
    </QueryClientProvider>,
  );
}

describe("SlotForm.changeProvider", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });
  afterEach(() => cleanup());

  it("clears slot.model when the new provider does NOT include the current model", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        row("anthropic", "anthropic", ["claude-sonnet-4-6"]),
        row("openai", "openai-compat", ["gpt-4.1-mini"]),
      ],
    });

    const onChange = vi.fn();
    renderSlot({
      slot: makeSlot({ provider: "anthropic", model: "claude-sonnet-4-6" }),
      onChange,
    });

    // Wait for the providers query to resolve so the <select> renders.
    // There are TWO comboboxes (provider + model); the provider one is
    // the first child of the Field labelled "Provider".
    // Provider is the first <select> rendered by SlotForm; ModelPicker
    // may also render a combobox so findByRole would be ambiguous. Wait
    // for the providers query to settle, then pick index 0.
    await waitFor(() =>
      expect(screen.getAllByRole("combobox").length).toBeGreaterThan(0),
    );
    const providerSelect = screen.getAllByRole("combobox")[0] as HTMLSelectElement;
    fireEvent.change(providerSelect, { target: { value: "openai" } });

    expect(onChange).toHaveBeenCalled();
    const next = onChange.mock.calls[0]![0] as AgentSlot;
    expect(next.provider).toBe("openai");
    // claude-sonnet-4-6 is NOT in openai's enabled_models → model cleared.
    expect(next.model).toBe("");
  });

  it("preserves slot.model when the new provider DOES include the current model", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        row("openai-prod", "openai-compat", ["gpt-4.1-mini"]),
        row("openai-staging", "openai-compat", ["gpt-4.1-mini"]),
      ],
    });

    const onChange = vi.fn();
    renderSlot({
      slot: makeSlot({ provider: "openai-prod", model: "gpt-4.1-mini" }),
      onChange,
    });

    // Provider is the first <select> rendered by SlotForm; ModelPicker
    // may also render a combobox so findByRole would be ambiguous. Wait
    // for the providers query to settle, then pick index 0.
    await waitFor(() =>
      expect(screen.getAllByRole("combobox").length).toBeGreaterThan(0),
    );
    const providerSelect = screen.getAllByRole("combobox")[0] as HTMLSelectElement;
    fireEvent.change(providerSelect, { target: { value: "openai-staging" } });

    expect(onChange).toHaveBeenCalled();
    const next = onChange.mock.calls[0]![0] as AgentSlot;
    expect(next.provider).toBe("openai-staging");
    expect(next.model).toBe("gpt-4.1-mini");
  });

  it("renders bar_history_limit input empty when slot value is null", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [row("anthropic", "anthropic", ["claude-sonnet-4-6"])],
    });

    renderSlot({
      slot: makeSlot({ bar_history_limit: null }),
      onChange: vi.fn(),
    });

    await waitFor(() =>
      expect(screen.getAllByRole("combobox").length).toBeGreaterThan(0),
    );
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    expect(input.value).toBe("");
  });

  it("renders bar_history_limit input with the slot's stored value", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [row("anthropic", "anthropic", ["claude-sonnet-4-6"])],
    });

    renderSlot({
      slot: makeSlot({ bar_history_limit: 50 }),
      onChange: vi.fn(),
    });

    await waitFor(() =>
      expect(screen.getAllByRole("combobox").length).toBeGreaterThan(0),
    );
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    expect(input.value).toBe("50");
  });

  it("persists a valid bar_history_limit through onChange", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [row("anthropic", "anthropic", ["claude-sonnet-4-6"])],
    });

    const onChange = vi.fn();
    renderSlot({
      slot: makeSlot({ bar_history_limit: null }),
      onChange,
    });

    await waitFor(() =>
      expect(screen.getAllByRole("combobox").length).toBeGreaterThan(0),
    );
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "120" } });

    expect(onChange).toHaveBeenCalled();
    const next = onChange.mock.calls[0]![0] as AgentSlot;
    expect(next.bar_history_limit).toBe(120);
  });

  it("clears bar_history_limit when input is emptied", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [row("anthropic", "anthropic", ["claude-sonnet-4-6"])],
    });

    const onChange = vi.fn();
    renderSlot({
      slot: makeSlot({ bar_history_limit: 42 }),
      onChange,
    });

    await waitFor(() =>
      expect(screen.getAllByRole("combobox").length).toBeGreaterThan(0),
    );
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "" } });

    expect(onChange).toHaveBeenCalled();
    const next = onChange.mock.calls[0]![0] as AgentSlot;
    expect(next.bar_history_limit).toBeNull();
  });

  it("rejects zero / negative bar_history_limit values (maps to null)", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [row("anthropic", "anthropic", ["claude-sonnet-4-6"])],
    });

    const onChange = vi.fn();
    renderSlot({
      slot: makeSlot({ bar_history_limit: 50 }),
      onChange,
    });

    await waitFor(() =>
      expect(screen.getAllByRole("combobox").length).toBeGreaterThan(0),
    );
    const input = screen.getByRole("spinbutton") as HTMLInputElement;

    fireEvent.change(input, { target: { value: "0" } });
    expect(
      (onChange.mock.calls.at(-1)![0] as AgentSlot).bar_history_limit,
    ).toBeNull();

    fireEvent.change(input, { target: { value: "-5" } });
    expect(
      (onChange.mock.calls.at(-1)![0] as AgentSlot).bar_history_limit,
    ).toBeNull();
  });

  it("clamps bar_history_limit above the max bound", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [row("anthropic", "anthropic", ["claude-sonnet-4-6"])],
    });

    const onChange = vi.fn();
    renderSlot({
      slot: makeSlot({ bar_history_limit: null }),
      onChange,
    });

    await waitFor(() =>
      expect(screen.getAllByRole("combobox").length).toBeGreaterThan(0),
    );
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "999999" } });

    expect(onChange).toHaveBeenCalled();
    const next = onChange.mock.calls.at(-1)![0] as AgentSlot;
    expect(next.bar_history_limit).toBe(1000);
  });

  it("preserves an empty model when changing providers (no spurious change)", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        row("anthropic", "anthropic", ["claude-sonnet-4-6"]),
        row("openai", "openai-compat", ["gpt-4.1-mini"]),
      ],
    });

    const onChange = vi.fn();
    renderSlot({
      slot: makeSlot({ provider: "anthropic", model: "" }),
      onChange,
    });

    // Provider is the first <select> rendered by SlotForm; ModelPicker
    // may also render a combobox so findByRole would be ambiguous. Wait
    // for the providers query to settle, then pick index 0.
    await waitFor(() =>
      expect(screen.getAllByRole("combobox").length).toBeGreaterThan(0),
    );
    const providerSelect = screen.getAllByRole("combobox")[0] as HTMLSelectElement;
    fireEvent.change(providerSelect, { target: { value: "openai" } });

    expect(onChange).toHaveBeenCalled();
    const next = onChange.mock.calls[0]![0] as AgentSlot;
    expect(next.provider).toBe("openai");
    expect(next.model).toBe("");
  });
});
