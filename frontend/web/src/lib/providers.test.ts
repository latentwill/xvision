import { describe, expect, it } from "vitest";
import type { ProviderRow } from "@/api/types.gen";
import { isProviderConfigured } from "./providers";

function row(overrides: Partial<ProviderRow> = {}): ProviderRow {
  return {
    name: "openai",
    kind: "openai-compat",
    base_url: "https://api.openai.com/v1",
    api_key_env: "OPENAI_API_KEY",
    api_key_set: true,
    synthetic: false,
    is_default: false,
    enabled_models: ["gpt-4o"],
    ...overrides,
  };
}

describe("isProviderConfigured", () => {
  it("returns true for a keyed provider with key set", () => {
    expect(isProviderConfigured(row({ api_key_env: "OPENAI_KEY", api_key_set: true }))).toBe(true);
  });

  it("returns false for a keyed provider missing its key", () => {
    expect(isProviderConfigured(row({ api_key_env: "OPENAI_KEY", api_key_set: false }))).toBe(false);
  });

  it("returns true for a no-auth provider (Ollama/local) even though api_key_set is false", () => {
    expect(isProviderConfigured(row({ api_key_env: "", api_key_set: false }))).toBe(true);
  });

  it("returns false for synthetic providers", () => {
    expect(isProviderConfigured(row({ synthetic: true, api_key_set: true }))).toBe(false);
  });

  it("returns false for synthetic no-auth providers", () => {
    expect(isProviderConfigured(row({ synthetic: true, api_key_env: "", api_key_set: false }))).toBe(false);
  });
});
