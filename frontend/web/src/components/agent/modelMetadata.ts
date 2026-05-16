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

// Pre-sorted, longest-prefix-first view of TABLE, computed once at module
// load. Every SlotForm render and every placeholder recomputation used to
// rebuild and sort this from scratch — fine on its own, but the cost
// scaled with slot count × rerenders × providers query refetches. Hoist.
const PREFIX_MATCHES: Array<[string, ModelMetadata]> = [...TABLE].sort(
  (a, b) => b[0].length - a[0].length,
);

// Provider prefixes that the legacy `LLMSlot.model_requirement` form
// uses to qualify a model id (e.g. `"anthropic.claude-sonnet-4.6"`).
// Mirrors `KNOWN_PROVIDER_PREFIXES` in
// `crates/xvision-core/src/providers/model_metadata.rs`. Drift between
// these two lists silently regresses the placeholder UX for legacy
// strategies; keep them in sync.
const KNOWN_PROVIDER_PREFIXES = new Set<string>([
  "anthropic",
  "openai",
  "openai-compat",
  "openrouter",
  "deepseek",
  "groq",
  "together",
  "mistral",
  "meta",
  "xai",
  "local-candle",
  "ollama",
]);

function stripKnownProviderPrefix(key: string): string {
  const dot = key.indexOf(".");
  if (dot < 0) return key;
  const head = key.slice(0, dot);
  return KNOWN_PROVIDER_PREFIXES.has(head) ? key.slice(dot + 1) : key;
}

function lookupExactOrPrefix(key: string): ModelMetadata | null {
  for (const [id, meta] of TABLE) {
    if (id === key) return meta;
  }
  for (const [id, meta] of PREFIX_MATCHES) {
    if (key.startsWith(id)) return meta;
  }
  if (key.startsWith("llama-3") || key.startsWith("llama3")) return LLAMA3_META;
  return null;
}

function lookupModelOptional(modelId: string): ModelMetadata | null {
  const trimmed = modelId.trim().toLowerCase();
  if (!trimmed) return null;
  // OpenRouter-style `vendor/model` — keep only the trailing segment.
  const lastSlash = trimmed.lastIndexOf("/");
  const afterSlash = lastSlash >= 0 ? trimmed.slice(lastSlash + 1) : trimmed;
  // Pre-agent templates write `LLMSlot.model_requirement` as
  // `provider.model-x.y`; strip the prefix when it matches a known
  // provider name.
  const tail = stripKnownProviderPrefix(afterSlash);

  const direct = lookupExactOrPrefix(tail);
  if (direct) return direct;

  // Legacy version-separator form (`claude-sonnet-4.6`). Retry once
  // with `.` normalized to `-` so the canonical dashed id wins.
  if (tail.indexOf(".") >= 0) {
    const normalized = tail.replace(/\./g, "-");
    const dashed = lookupExactOrPrefix(normalized);
    if (dashed) return dashed;
  }

  return null;
}

export function lookupModel(modelId: string): ModelMetadata {
  return lookupModelOptional(modelId) ?? UNKNOWN_DEFAULT;
}

// True iff the editorial table knows this model. SlotForm uses this to
// decide whether the "Provider default" placeholder applies — the
// fallback copy is for OpenAI-compat models the editorial table has
// never heard of, not for known models that happen to be missing from
// the live catalog fetch.
export function hasModelMetadata(modelId: string): boolean {
  return lookupModelOptional(modelId) !== null;
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
