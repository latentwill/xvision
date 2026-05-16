// q15 §1 — agent max-tokens UX checks. Covers the SlotForm "Auto from
// model" pill and the placeholder that updates when the slot's model
// changes.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { AgentSlot } from "@/api/agents";
import { SlotForm } from "./SlotForm";
import {
  autoMaxTokens,
  hasModelMetadata,
  isReasoning,
  lookupModel,
} from "./modelMetadata";
import * as settingsApi from "@/api/settings";

vi.mock("@/api/settings", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/api/settings")>();
  return {
    ...actual,
    listProviders: vi.fn(),
    getProviderCatalog: vi.fn(),
  };
});

function makeProviderRow(overrides: Record<string, unknown> = {}) {
  return {
    name: "anthropic",
    kind: "anthropic",
    base_url: "",
    api_key_env: "ANTHROPIC_API_KEY",
    api_key_set: true,
    synthetic: false,
    is_default: false,
    enabled_models: [],
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(settingsApi.listProviders).mockResolvedValue({
    providers: [],
  } as never);
  vi.mocked(settingsApi.getProviderCatalog).mockResolvedValue({
    provider: "",
    models: [],
    fetched_at: null,
    source: "live",
  } as never);
});

function makeSlot(overrides: Partial<AgentSlot> = {}): AgentSlot {
  return {
    name: "main",
    provider: "anthropic",
    model: "claude-sonnet-4-6",
    system_prompt: "Trade carefully.",
    skill_ids: [],
    max_tokens: null,
    ...overrides,
  };
}

function renderSlotForm(
  slot: AgentSlot,
  onChange: (next: AgentSlot) => void = () => {},
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
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

afterEach(() => {
  cleanup();
});

describe("modelMetadata table", () => {
  it("falls back to a non-reasoning default for unknown models", () => {
    const meta = lookupModel("acme-co/nightly-7b");
    expect(meta.class).toBe("standard");
    expect(meta.output_token_ceiling).toBeGreaterThanOrEqual(2048);
    expect(autoMaxTokens(meta)).toBeGreaterThanOrEqual(meta.recommended_visible_output);
  });

  it("strips OpenRouter vendor prefix", () => {
    const a = lookupModel("anthropic/claude-sonnet-4-6");
    const b = lookupModel("claude-sonnet-4-6");
    expect(a).toEqual(b);
  });

  it("flags reasoning-class models", () => {
    expect(isReasoning(lookupModel("deepseek-r1"))).toBe(true);
    expect(isReasoning(lookupModel("o3"))).toBe(true);
    expect(isReasoning(lookupModel("claude-haiku-4-5"))).toBe(false);
  });

  it("matches date-stamped variants by prefix", () => {
    const exact = lookupModel("claude-sonnet-4-6");
    const dated = lookupModel("claude-sonnet-4-6-20260101");
    expect(dated.output_token_ceiling).toBe(exact.output_token_ceiling);
  });

  it("resolves the legacy LLMSlot model_requirement dotted form", () => {
    // Pre-agent templates store `model_requirement` as
    // `"anthropic.claude-sonnet-4.6"`. The placeholder UX must match
    // what the engine resolves so operators don't see a 4096 default
    // for legacy strategies that the dispatcher actually budgets at the
    // model's real ceiling.
    const legacy = lookupModel("anthropic.claude-sonnet-4.6");
    const canonical = lookupModel("claude-sonnet-4-6");
    expect(legacy.output_token_ceiling).toBe(canonical.output_token_ceiling);
    expect(legacy.recommended_visible_output).toBe(canonical.recommended_visible_output);
    expect(legacy.class).toBe(canonical.class);
  });

  it("does not misread version dots in real model ids as provider prefixes", () => {
    // `gpt-4.1` is an actual OpenAI model id; the lookup must not treat
    // `gpt-4` as a provider prefix and strip it.
    const m = lookupModel("gpt-4.1");
    expect(m.output_token_ceiling).toBe(32768);
    expect(m.class).toBe("standard");
  });

  it("hasModelMetadata distinguishes known models from the UNKNOWN fallback", () => {
    // SlotForm relies on this to decide whether the "Provider default"
    // copy applies — known OpenAI-compat models that are missing from a
    // live catalog response must NOT collapse to the vaguer copy.
    expect(hasModelMetadata("claude-sonnet-4-6")).toBe(true);
    expect(hasModelMetadata("anthropic/claude-sonnet-4-6")).toBe(true);
    expect(hasModelMetadata("gpt-4.1")).toBe(true);
    expect(hasModelMetadata("acme-co/nightly-7b")).toBe(false);
    expect(hasModelMetadata("")).toBe(false);
  });
});

describe("SlotForm max_tokens UX", () => {
  it("shows the Auto pill and per-model placeholder when max_tokens is null", () => {
    renderSlotForm(makeSlot({ max_tokens: null }));
    const meta = lookupModel("claude-sonnet-4-6");
    // Number is locale-formatted (so 384,000 reads as a number, not a
    // git sha). Build the expected placeholder the same way the
    // component does.
    const expected = `Auto: ${autoMaxTokens(meta).toLocaleString()}`;
    expect(screen.getByText("Auto from model")).toBeTruthy();
    expect(screen.getByPlaceholderText(expected)).toBeTruthy();
  });

  it("hides the Auto pill and offers Reset when max_tokens is set", () => {
    renderSlotForm(makeSlot({ max_tokens: 6000 }));
    expect(screen.queryByText("Auto from model")).toBeNull();
    expect(screen.getByText("Reset")).toBeTruthy();
  });

  it("clears the override to null on Reset", () => {
    let received: AgentSlot | null = null;
    renderSlotForm(makeSlot({ max_tokens: 6000 }), (next) => {
      received = next;
    });
    fireEvent.click(screen.getByText("Reset"));
    expect(received).not.toBeNull();
    expect(received!.max_tokens).toBeNull();
  });

  it("treats empty input as auto (null) so operators can blank the field", () => {
    let received: AgentSlot | null = null;
    renderSlotForm(makeSlot({ max_tokens: 6000 }), (next) => {
      received = next;
    });
    const input = screen.getByDisplayValue("6000") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "" } });
    expect(received).not.toBeNull();
    expect(received!.max_tokens).toBeNull();
  });

  it("updates the placeholder live when the model changes", () => {
    const sonnet = lookupModel("claude-sonnet-4-6");
    const haiku = lookupModel("claude-haiku-4-5");
    expect(sonnet.recommended_visible_output).not.toBe(
      haiku.recommended_visible_output,
    );

    const { rerender } = renderSlotForm(makeSlot({ model: "claude-sonnet-4-6" }));
    expect(
      screen.getByPlaceholderText(
        `Auto: ${autoMaxTokens(sonnet).toLocaleString()}`,
      ),
    ).toBeTruthy();

    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    rerender(
      <QueryClientProvider client={client}>
        <SlotForm
          slot={makeSlot({ model: "claude-haiku-4-5" })}
          onChange={() => {}}
          onRemove={() => {}}
          onDuplicate={() => {}}
          canRemove={false}
          index={0}
        />
      </QueryClientProvider>,
    );
    expect(
      screen.getByPlaceholderText(
        `Auto: ${autoMaxTokens(haiku).toLocaleString()}`,
      ),
    ).toBeTruthy();
  });

  it("keeps the editorial number for a known model on an openai-compat provider with a catalog miss", async () => {
    // Regression: previously, any OpenAI-compat catalog miss collapsed to
    // "Provider default", even when the editorial table knew the model.
    // The fallback copy is reserved for true editorial misses.
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        makeProviderRow({
          name: "openrouter",
          kind: "openai-compat",
          base_url: "https://openrouter.ai/api/v1",
          api_key_env: "OPENROUTER_API_KEY",
        }),
      ],
    } as never);
    vi.mocked(settingsApi.getProviderCatalog).mockResolvedValue({
      provider: "openrouter",
      models: [],
      fetched_at: null,
      source: "live",
    } as never);

    renderSlotForm(
      makeSlot({
        provider: "openrouter",
        model: "claude-sonnet-4-6",
        max_tokens: null,
      }),
    );

    const meta = lookupModel("claude-sonnet-4-6");
    const expected = `Auto: ${autoMaxTokens(meta).toLocaleString()}`;
    await waitFor(() => {
      expect(screen.getByText("Auto from model")).toBeTruthy();
    });
    expect(screen.getByPlaceholderText(expected)).toBeTruthy();
    expect(screen.queryByText("Provider default")).toBeNull();
  });

  it("shows Provider default only when both catalog and editorial miss on openai-compat", async () => {
    vi.mocked(settingsApi.listProviders).mockResolvedValue({
      providers: [
        makeProviderRow({
          name: "openrouter",
          kind: "openai-compat",
          base_url: "https://openrouter.ai/api/v1",
          api_key_env: "OPENROUTER_API_KEY",
        }),
      ],
    } as never);
    vi.mocked(settingsApi.getProviderCatalog).mockResolvedValue({
      provider: "openrouter",
      models: [],
      fetched_at: null,
      source: "live",
    } as never);

    renderSlotForm(
      makeSlot({
        provider: "openrouter",
        model: "acme-co/nightly-7b",
        max_tokens: null,
      }),
    );

    await waitFor(() => {
      expect(screen.getByText("Provider default")).toBeTruthy();
    });
    expect(screen.getByPlaceholderText("Provider default")).toBeTruthy();
  });
});
