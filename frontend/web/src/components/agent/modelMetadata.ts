// Frontend mirror of `xvision_core::providers::model_metadata`. Kept in
// the components folder (not under `api/`) because it's a UX-only lookup
// for the "Auto from model" placeholder — the server is the source of
// truth at dispatch time.
//
// When you add a model on the Rust side, mirror it here so the SlotForm
// placeholder stays accurate. The shape intentionally matches the Rust
// `ModelMetadata` so this file is easy to diff against the canonical
// table at `crates/xvision-core/src/providers/model_metadata.rs`.

export type ModelClass = "standard" | "reasoning";

export type ModelMetadata = {
  output_token_ceiling: number;
  reasoning_token_default: number;
  recommended_visible_output: number;
  class: ModelClass;
};

const UNKNOWN_DEFAULT: ModelMetadata = {
  output_token_ceiling: 4096,
  reasoning_token_default: 0,
  recommended_visible_output: 4096,
  class: "standard",
};

// Exact-id entries first; the loop below also matches `starts_with` so
// date-stamped variants ("claude-sonnet-4-6-20260101") resolve to the
// canonical row.
const TABLE: Array<[string, ModelMetadata]> = [
  [
    "claude-opus-4-7",
    { output_token_ceiling: 8192, reasoning_token_default: 0, recommended_visible_output: 4096, class: "standard" },
  ],
  [
    "claude-opus-4-6",
    { output_token_ceiling: 8192, reasoning_token_default: 0, recommended_visible_output: 4096, class: "standard" },
  ],
  [
    "claude-sonnet-4-6",
    { output_token_ceiling: 8192, reasoning_token_default: 0, recommended_visible_output: 4096, class: "standard" },
  ],
  [
    "claude-sonnet-4-5",
    { output_token_ceiling: 8192, reasoning_token_default: 0, recommended_visible_output: 4096, class: "standard" },
  ],
  [
    "claude-haiku-4-5",
    { output_token_ceiling: 8192, reasoning_token_default: 0, recommended_visible_output: 2048, class: "standard" },
  ],
  [
    "o3",
    { output_token_ceiling: 16384, reasoning_token_default: 10000, recommended_visible_output: 4096, class: "reasoning" },
  ],
  [
    "o3-mini",
    { output_token_ceiling: 16384, reasoning_token_default: 10000, recommended_visible_output: 4096, class: "reasoning" },
  ],
  [
    "o4-mini",
    { output_token_ceiling: 16384, reasoning_token_default: 10000, recommended_visible_output: 4096, class: "reasoning" },
  ],
  [
    "o1",
    { output_token_ceiling: 16384, reasoning_token_default: 10000, recommended_visible_output: 4096, class: "reasoning" },
  ],
  [
    "o1-mini",
    { output_token_ceiling: 16384, reasoning_token_default: 10000, recommended_visible_output: 4096, class: "reasoning" },
  ],
  [
    "o1-preview",
    { output_token_ceiling: 16384, reasoning_token_default: 10000, recommended_visible_output: 4096, class: "reasoning" },
  ],
  [
    "gpt-4o",
    { output_token_ceiling: 16384, reasoning_token_default: 0, recommended_visible_output: 4096, class: "standard" },
  ],
  [
    "gpt-4o-mini",
    { output_token_ceiling: 16384, reasoning_token_default: 0, recommended_visible_output: 4096, class: "standard" },
  ],
  [
    "gpt-4.1",
    { output_token_ceiling: 32768, reasoning_token_default: 0, recommended_visible_output: 4096, class: "standard" },
  ],
  [
    "gpt-4.1-mini",
    { output_token_ceiling: 32768, reasoning_token_default: 0, recommended_visible_output: 4096, class: "standard" },
  ],
  [
    "gpt-4.1-nano",
    { output_token_ceiling: 32768, reasoning_token_default: 0, recommended_visible_output: 4096, class: "standard" },
  ],
  [
    "deepseek-chat",
    { output_token_ceiling: 8192, reasoning_token_default: 0, recommended_visible_output: 4096, class: "standard" },
  ],
  [
    "deepseek-v3",
    { output_token_ceiling: 8192, reasoning_token_default: 0, recommended_visible_output: 4096, class: "standard" },
  ],
  [
    "deepseek-v3.1",
    { output_token_ceiling: 8192, reasoning_token_default: 0, recommended_visible_output: 4096, class: "standard" },
  ],
  [
    "deepseek-reasoner",
    { output_token_ceiling: 16384, reasoning_token_default: 10000, recommended_visible_output: 4096, class: "reasoning" },
  ],
  [
    "deepseek-r1",
    { output_token_ceiling: 16384, reasoning_token_default: 10000, recommended_visible_output: 4096, class: "reasoning" },
  ],
];

const LLAMA3_META: ModelMetadata = {
  output_token_ceiling: 4096,
  reasoning_token_default: 0,
  recommended_visible_output: 2048,
  class: "standard",
};

export function lookupModel(modelId: string): ModelMetadata {
  const trimmed = modelId.trim().toLowerCase();
  if (!trimmed) return UNKNOWN_DEFAULT;
  // Strip OpenRouter-style vendor prefix so "anthropic/claude-sonnet-4-6"
  // matches "claude-sonnet-4-6".
  const lastSlash = trimmed.lastIndexOf("/");
  const key = lastSlash >= 0 ? trimmed.slice(lastSlash + 1) : trimmed;

  // Exact match wins.
  for (const [id, meta] of TABLE) {
    if (id === key) return meta;
  }
  // Prefix match for date-stamped or fine-grained variants. Iterate
  // longest-id-first so claude-sonnet-4-6 wins over claude-sonnet-4.
  const sorted = [...TABLE].sort((a, b) => b[0].length - a[0].length);
  for (const [id, meta] of sorted) {
    if (key.startsWith(id)) return meta;
  }
  if (key.startsWith("llama-3") || key.startsWith("llama3")) return LLAMA3_META;
  return UNKNOWN_DEFAULT;
}

/// Mirrors `ModelMetadata::auto_max_tokens` on the Rust side: visible
/// budget + reasoning budget, clamped to the provider ceiling. Used as
/// the placeholder shown when the slot's max_tokens is unset.
export function autoMaxTokens(meta: ModelMetadata): number {
  const target = meta.recommended_visible_output + meta.reasoning_token_default;
  return Math.min(target, meta.output_token_ceiling);
}

export function isReasoning(meta: ModelMetadata): boolean {
  return meta.class === "reasoning";
}
