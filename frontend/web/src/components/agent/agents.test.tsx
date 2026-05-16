// q15 §1 — agent max-tokens UX checks. Covers the SlotForm "Auto from
// model" pill and the placeholder that updates when the slot's model
// changes.

import { afterEach, describe, expect, it } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { AgentSlot } from "@/api/agents";
import { SlotForm } from "./SlotForm";
import {
  autoMaxTokens,
  isReasoning,
  lookupModel,
} from "./modelMetadata";

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
});

describe("SlotForm max_tokens UX", () => {
  it("shows the Auto pill and per-model placeholder when max_tokens is null", () => {
    renderSlotForm(makeSlot({ max_tokens: null }));
    const meta = lookupModel("claude-sonnet-4-6");
    const expected = `Auto: ${autoMaxTokens(meta)}`;
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
      screen.getByPlaceholderText(`Auto: ${autoMaxTokens(sonnet)}`),
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
      screen.getByPlaceholderText(`Auto: ${autoMaxTokens(haiku)}`),
    ).toBeTruthy();
  });
});
