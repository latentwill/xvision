// modelMetadata table coverage. The per-slot `max_tokens` input was
// removed in 2026-05-17 (qa-remove-agent-max-tokens), so the old
// SlotForm-rendering UX tests are gone. The metadata table itself is
// still consumed (for provider-catalog tooling / labels), so the lookup
// behaviour stays under test.

import { describe, expect, it } from "vitest";
import {
  autoMaxTokens,
  hasModelMetadata,
  isReasoning,
  lookupModel,
} from "./modelMetadata";

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
    // `"anthropic.claude-sonnet-4.6"`. The dispatcher resolves this to
    // the model's real ceiling; the lookup table must agree so anywhere
    // else that consumes the metadata (tooling, labels) matches.
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
    expect(hasModelMetadata("claude-sonnet-4-6")).toBe(true);
    expect(hasModelMetadata("anthropic/claude-sonnet-4-6")).toBe(true);
    expect(hasModelMetadata("gpt-4.1")).toBe(true);
    expect(hasModelMetadata("acme-co/nightly-7b")).toBe(false);
    expect(hasModelMetadata("")).toBe(false);
  });
});
