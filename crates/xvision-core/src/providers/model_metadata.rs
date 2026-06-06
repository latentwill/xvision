//! Per-model metadata used by agent-slot dispatch to derive a safe
//! `max_tokens` when the operator hasn't pinned one, and to surface a
//! reasoning-class-aware truncation hint when a thinking model exhausts
//! its budget before emitting visible text.
//!
//! See `docs/superpowers/specs/2026-05-16-q15-eval-resilience-and-contracts.md`
//! §1 for the rationale.
//!
//! The metadata is a canonical table keyed by lowercase model id. Unknown
//! model ids fall back to a conservative default
//! (`ModelMetadata::unknown_default`) that matches the legacy hardcoded
//! `max_tokens = 4096` while marking the model non-reasoning. The
//! ProviderRegistry can layer additional metadata on top of this canonical
//! table; this module owns the defaults only.
//!
//! Adding a new known model is intentionally cheap: append a `case` arm in
//! `lookup_model_inner` with the model id and the three numbers. Renames
//! should be added as aliases rather than replacements so existing agents
//! that still reference the old id keep resolving.

use serde::{Deserialize, Serialize};

/// Coarse class signal — `Reasoning` means the model spends hidden
/// reasoning tokens before any visible text emerges, so a small
/// `max_tokens` budget will truncate the run with `raw_excerpt="<empty>"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelClass {
    /// Standard non-reasoning model. Visible text starts streaming as
    /// soon as the model begins emitting tokens.
    Standard,
    /// Reasoning / thinking model — DeepSeek R1, OpenAI o-series,
    /// Anthropic Sonnet under extended-thinking. Hidden reasoning eats
    /// budget before visible text, so `reasoning_token_default` needs to
    /// be added to the visible budget.
    Reasoning,
}

/// Per-model metadata used at dispatch time.
///
/// All three token numbers are upper-bound budgets, not guarantees. They
/// shape the default `max_tokens` an agent slot resolves to when the
/// operator hasn't pinned one, and the clamp applied to explicit values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelMetadata {
    /// Canonical lowercase model id (e.g., `"claude-sonnet-4-6"`,
    /// `"deepseek-r1"`, `"gpt-4o-mini"`).
    pub id: String,
    /// Hard provider cap. An explicit `max_tokens` value is clamped down
    /// to this; the resolved auto value never exceeds it.
    pub output_token_ceiling: u32,
    /// Budget allocated for hidden reasoning on reasoning-class models.
    /// 0 for `Standard` models.
    pub reasoning_token_default: u32,
    /// Target visible-text budget — the "real" answer the model emits
    /// after reasoning (or the entire response for `Standard` models).
    pub recommended_visible_output: u32,
    /// Coarse reasoning-class signal used to gate the truncation hint and
    /// the auto-tokens math.
    pub class: ModelClass,
}

impl ModelMetadata {
    /// Conservative fallback for an unknown model id. Treats the model as
    /// `Standard` with a 4096 visible budget — matches the legacy default
    /// across the codebase, so unknown-model agents keep working.
    pub fn unknown_default(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            output_token_ceiling: 4096,
            reasoning_token_default: 0,
            recommended_visible_output: 4096,
            class: ModelClass::Standard,
        }
    }

    /// True iff the model spends hidden reasoning tokens before any
    /// visible text — surfaces the actionable hint on truncation.
    pub fn is_reasoning(&self) -> bool {
        matches!(self.class, ModelClass::Reasoning)
    }

    /// Auto-resolved `max_tokens` when an agent slot leaves the value
    /// unset. Adds `reasoning_token_default` for reasoning models so the
    /// model has room to think before it has to start emitting text, then
    /// clamps to the provider cap.
    pub fn auto_max_tokens(&self) -> u32 {
        let target = self
            .recommended_visible_output
            .saturating_add(self.reasoning_token_default);
        target.min(self.output_token_ceiling)
    }

    /// Apply the provider-cap clamp to an explicit `max_tokens` value.
    pub fn clamp_explicit(&self, requested: u32) -> u32 {
        requested.min(self.output_token_ceiling)
    }

    /// Resolve an agent slot's `max_tokens` field — `None` or `Some(0)`
    /// (the SQLite storage sentinel for unset) auto-derive from the
    /// model's metadata; any other `Some(n)` is clamped to the model
    /// ceiling. This is the single source of truth for the q15 §1
    /// resolution rule.
    pub fn resolve(&self, explicit: Option<u32>) -> u32 {
        match explicit {
            Some(n) if n > 0 => self.clamp_explicit(n),
            _ => self.auto_max_tokens(),
        }
    }
}

/// Known provider-name prefixes that the legacy `LLMSlot.attested_with`
/// path uses to qualify a model id (e.g. `"anthropic.claude-sonnet-4.6"`).
/// Lookup strips one of these prefixes when it would otherwise prevent a
/// hit. Order doesn't matter; the comparison is exact on the segment
/// before the first `.`.
const KNOWN_PROVIDER_PREFIXES: &[&str] = &[
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
    "vllm",
];

/// Look up metadata for a model id, falling back to
/// `ModelMetadata::unknown_default` when the id isn't in the canonical
/// table. The match is case-insensitive, whitespace-trimmed, and
/// normalizes three legacy spellings:
///
/// - OpenRouter `vendor/model` is reduced to `model`.
/// - Pre-refactor `LLMSlot.attested_with` values qualify the id with
///   a provider prefix and a dot — `"anthropic.claude-sonnet-4.6"`. When
///   the prefix matches a known provider, it's stripped.
/// - The same legacy form also writes version separators with `.`
///   (`"claude-sonnet-4.6"`) where the canonical table uses `-`
///   (`"claude-sonnet-4-6"`). When the initial lookup misses, the tail
///   is retried with dots normalized to dashes.
///
/// Date-stamped variants (`"claude-sonnet-4-6-20260101"`) keep
/// resolving via the `starts_with` arms in `lookup_model_inner`.
pub fn lookup_model(id: &str) -> ModelMetadata {
    let trimmed = id.trim().to_lowercase();
    // OpenRouter-style `vendor/model` — keep only the trailing segment.
    let after_slash = trimmed.rsplit('/').next().unwrap_or(trimmed.as_str());
    // `provider.model-x.y` — strip the prefix when the first segment is
    // a known provider. The remaining tail can still contain dots, which
    // is the legacy version-separator convention handled below.
    let tail = strip_known_provider_prefix(after_slash);

    if let Some(meta) = lookup_model_inner(tail) {
        return meta;
    }
    // Legacy dotted version form: `claude-sonnet-4.6` → `claude-sonnet-4-6`.
    // Only retry when there is at least one dot to normalize — keeps the
    // happy path a single match call.
    if tail.contains('.') {
        let normalized: String = tail.chars().map(|c| if c == '.' { '-' } else { c }).collect();
        if let Some(meta) = lookup_model_inner(&normalized) {
            return meta;
        }
    }
    ModelMetadata::unknown_default(id)
}

fn strip_known_provider_prefix(key: &str) -> &str {
    let Some((head, tail)) = key.split_once('.') else {
        return key;
    };
    if KNOWN_PROVIDER_PREFIXES.contains(&head) {
        tail
    } else {
        key
    }
}

fn lookup_model_inner(key: &str) -> Option<ModelMetadata> {
    // Try the exact-id match first; date-stamped variants
    // (`claude-sonnet-4-6-20260101`) fall through to the `starts_with`
    // arms at the bottom so the canonical class wins.
    let meta = match key {
        // --- Anthropic Claude 4.x ------------------------------------
        "claude-opus-4-7" | "claude-opus-4-6" => ModelMetadata {
            id: key.to_string(),
            output_token_ceiling: 8192,
            reasoning_token_default: 0,
            recommended_visible_output: 4096,
            class: ModelClass::Standard,
        },
        "claude-sonnet-4-6" | "claude-sonnet-4-5" => ModelMetadata {
            id: key.to_string(),
            // Sonnet 4.x supports up to 8192 output tokens via standard
            // messages. Extended-thinking is a separate request param and
            // is not exposed through the q15 surface — q15 just needs a
            // generous default so the QA reproducer stops truncating.
            output_token_ceiling: 8192,
            reasoning_token_default: 0,
            recommended_visible_output: 4096,
            class: ModelClass::Standard,
        },
        "claude-haiku-4-5" => ModelMetadata {
            id: key.to_string(),
            output_token_ceiling: 8192,
            reasoning_token_default: 0,
            recommended_visible_output: 2048,
            class: ModelClass::Standard,
        },

        // --- OpenAI o-series (reasoning) -----------------------------
        // o3 / o4-mini / o1 spend significant budget on hidden reasoning
        // before any visible text streams; treat them as `Reasoning`.
        "o3" | "o3-mini" | "o4-mini" | "o1" | "o1-mini" | "o1-preview" => ModelMetadata {
            id: key.to_string(),
            output_token_ceiling: 16_384,
            reasoning_token_default: 10_000,
            recommended_visible_output: 4096,
            class: ModelClass::Reasoning,
        },

        // --- OpenAI GPT (standard) -----------------------------------
        "gpt-4o" | "gpt-4o-mini" => ModelMetadata {
            id: key.to_string(),
            output_token_ceiling: 16_384,
            reasoning_token_default: 0,
            recommended_visible_output: 4096,
            class: ModelClass::Standard,
        },
        "gpt-4.1" | "gpt-4.1-mini" | "gpt-4.1-nano" => ModelMetadata {
            id: key.to_string(),
            output_token_ceiling: 32_768,
            reasoning_token_default: 0,
            recommended_visible_output: 4096,
            class: ModelClass::Standard,
        },

        // --- DeepSeek -----------------------------------------------
        "deepseek-chat" | "deepseek-v3" | "deepseek-v3.1" => ModelMetadata {
            id: key.to_string(),
            output_token_ceiling: 8192,
            reasoning_token_default: 0,
            recommended_visible_output: 4096,
            class: ModelClass::Standard,
        },
        "deepseek-reasoner" | "deepseek-r1" => ModelMetadata {
            id: key.to_string(),
            output_token_ceiling: 16_384,
            reasoning_token_default: 10_000,
            recommended_visible_output: 4096,
            class: ModelClass::Reasoning,
        },

        // --- Open-weights (Ollama / OpenRouter routes) --------------
        s if s.starts_with("llama-3") || s.starts_with("llama3") => ModelMetadata {
            id: key.to_string(),
            output_token_ceiling: 4096,
            reasoning_token_default: 0,
            recommended_visible_output: 2048,
            class: ModelClass::Standard,
        },

        // --- Date-stamped variants (e.g. claude-sonnet-4-6-20260101) ---
        // Order matters: longer prefix arms come first so a hypothetical
        // `claude-sonnet-4-6-20260101` matches `claude-sonnet-4-6` and
        // not `claude-sonnet`.
        s if s.starts_with("claude-sonnet-4-6") || s.starts_with("claude-sonnet-4-5") => ModelMetadata {
            id: key.to_string(),
            output_token_ceiling: 8192,
            reasoning_token_default: 0,
            recommended_visible_output: 4096,
            class: ModelClass::Standard,
        },
        s if s.starts_with("claude-opus-4-7") || s.starts_with("claude-opus-4-6") => ModelMetadata {
            id: key.to_string(),
            output_token_ceiling: 8192,
            reasoning_token_default: 0,
            recommended_visible_output: 4096,
            class: ModelClass::Standard,
        },
        s if s.starts_with("claude-haiku-4-5") => ModelMetadata {
            id: key.to_string(),
            output_token_ceiling: 8192,
            reasoning_token_default: 0,
            recommended_visible_output: 2048,
            class: ModelClass::Standard,
        },

        _ => return None,
    };
    Some(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_model_falls_back_to_safe_default() {
        let m = lookup_model("not-a-real-model");
        assert_eq!(m.output_token_ceiling, 4096);
        assert_eq!(m.recommended_visible_output, 4096);
        assert_eq!(m.reasoning_token_default, 0);
        assert!(!m.is_reasoning(), "unknown should not claim reasoning class");
    }

    #[test]
    fn sonnet_4_6_metadata_unblocks_qa15_reproducer() {
        // QA15 item 5: Sonnet 4.6 + max_tokens=1000 + scenario triggers
        // thinking → output_tokens=1000, raw_excerpt="<empty>".
        // With the q15 default in place, auto_max_tokens must be well
        // above 1000.
        let m = lookup_model("claude-sonnet-4-6");
        assert!(
            m.auto_max_tokens() >= 4096,
            "Sonnet 4.6 auto-resolved max_tokens must be at least 4096 to clear the QA15 truncation case; got {}",
            m.auto_max_tokens()
        );
        // Auto resolution must never overshoot the provider cap.
        assert!(m.auto_max_tokens() <= m.output_token_ceiling);
    }

    #[test]
    fn reasoning_models_carry_reasoning_token_default() {
        for id in ["o3", "o3-mini", "o1", "deepseek-r1", "deepseek-reasoner"] {
            let m = lookup_model(id);
            assert!(m.is_reasoning(), "{id} should be reasoning class");
            assert!(
                m.reasoning_token_default >= 10_000,
                "{id} should reserve ≥10k reasoning tokens; got {}",
                m.reasoning_token_default
            );
            // Visible budget + reasoning budget, clamped to ceiling, is
            // what the slot will request when max_tokens is unset.
            let auto = m.auto_max_tokens();
            assert!(auto > 0, "{id} auto must be non-zero");
        }
    }

    #[test]
    fn standard_models_have_zero_reasoning_default() {
        for id in ["claude-sonnet-4-6", "claude-haiku-4-5", "gpt-4o", "deepseek-chat"] {
            let m = lookup_model(id);
            assert!(!m.is_reasoning(), "{id} should be standard class");
            assert_eq!(
                m.reasoning_token_default, 0,
                "{id} standard class must not reserve reasoning tokens"
            );
        }
    }

    #[test]
    fn clamp_explicit_caps_at_ceiling() {
        let m = lookup_model("claude-sonnet-4-6");
        assert_eq!(m.clamp_explicit(2048), 2048, "below cap stays unchanged");
        assert_eq!(
            m.clamp_explicit(99_999),
            m.output_token_ceiling,
            "values above the cap clamp to ceiling"
        );
    }

    #[test]
    fn lookup_is_case_and_whitespace_insensitive() {
        let a = lookup_model("Claude-Sonnet-4-6");
        let b = lookup_model("  claude-sonnet-4-6  ");
        assert_eq!(a.output_token_ceiling, b.output_token_ceiling);
        assert_eq!(a.class, b.class);
    }

    #[test]
    fn llama3_family_pattern_matches() {
        let m = lookup_model("llama-3.1-70b-instruct");
        assert!(!m.is_reasoning());
        assert!(m.output_token_ceiling >= 4096);
    }

    #[test]
    fn openrouter_vendor_prefix_is_stripped() {
        // OpenRouter routes models as `vendor/model`. The lookup strips
        // the vendor prefix so the canonical row wins.
        let m = lookup_model("anthropic/claude-sonnet-4-6");
        assert_eq!(m.output_token_ceiling, 8192);
        assert!(!m.is_reasoning());
    }

    #[test]
    fn date_stamped_variant_resolves_to_canonical_class() {
        // Anthropic occasionally publishes dated ids. They should resolve
        // to the same row as the canonical id, not the unknown fallback.
        let m = lookup_model("claude-sonnet-4-6-20260101");
        assert_eq!(m.output_token_ceiling, 8192);
        assert_eq!(m.recommended_visible_output, 4096);
    }

    #[test]
    fn legacy_dotted_attested_with_resolves_to_canonical_row() {
        // Pre-agent templates carry `LLMSlot.attested_with` strings
        // like `"anthropic.claude-sonnet-4.6"` (see e.g. the mean-reversion
        // template). The lookup must strip the provider prefix and
        // normalize the dotted version separator so legacy strategies
        // get the new per-model budget instead of falling through to
        // `unknown_default` and keeping the old 4096.
        let m = lookup_model("anthropic.claude-sonnet-4.6");
        assert_eq!(m.output_token_ceiling, 8192);
        assert_eq!(m.recommended_visible_output, 4096);
        let canonical = lookup_model("claude-sonnet-4-6");
        assert_eq!(m.output_token_ceiling, canonical.output_token_ceiling);
        assert_eq!(m.recommended_visible_output, canonical.recommended_visible_output);
        assert_eq!(m.class, canonical.class);
    }

    #[test]
    fn legacy_dotted_form_works_for_other_providers() {
        // The same legacy convention applies across providers; cover the
        // common ones so a future addition doesn't silently regress.
        let openai = lookup_model("openai.gpt-4o");
        assert_eq!(
            openai.output_token_ceiling,
            lookup_model("gpt-4o").output_token_ceiling
        );

        let deepseek = lookup_model("deepseek.deepseek-r1");
        let r1 = lookup_model("deepseek-r1");
        assert!(deepseek.is_reasoning());
        assert_eq!(deepseek.output_token_ceiling, r1.output_token_ceiling);
        assert_eq!(deepseek.reasoning_token_default, r1.reasoning_token_default);
    }

    #[test]
    fn dotted_id_with_unknown_prefix_is_left_alone() {
        // `gpt-4.1` is itself a real model id — the lookup must not
        // mistake `gpt-4` for a provider prefix and strip it. The dot
        // here is a version separator, not a provider split.
        let m = lookup_model("gpt-4.1");
        assert_eq!(m.output_token_ceiling, 32_768);
        assert!(!m.is_reasoning());
    }

    #[test]
    fn legacy_dotted_form_still_returns_unknown_for_unknown_models() {
        // A provider prefix shouldn't turn an otherwise-unknown model
        // into a known one. The stripped tail should miss the table and
        // fall back to the safe default.
        let m = lookup_model("anthropic.totally-fake-9000");
        assert_eq!(m.output_token_ceiling, 4096);
        assert_eq!(m.recommended_visible_output, 4096);
        assert!(!m.is_reasoning());
    }

    #[test]
    fn auto_max_tokens_caps_at_ceiling() {
        // Synthetic metadata to confirm the ceiling clamp holds when the
        // reasoning budget exceeds it.
        let m = ModelMetadata {
            id: "test".into(),
            output_token_ceiling: 1024,
            reasoning_token_default: 4096,
            recommended_visible_output: 4096,
            class: ModelClass::Reasoning,
        };
        assert_eq!(m.auto_max_tokens(), 1024);
    }
}
