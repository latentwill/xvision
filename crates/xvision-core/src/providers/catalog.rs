//! Catalog data types. A `Catalog` is the parsed result of hitting a
//! provider's `/v1/models` (or equivalent) endpoint, normalized into a
//! shape the engine and dashboard can reason about.
//!
//! Pure data — no HTTP, no disk, no async. The fetching, caching, and
//! lookup-with-fallback logic all live in `xvision-engine::providers`,
//! which can take the runtime dependencies (`reqwest`, `tokio::fs`)
//! without dragging them into the workspace's data-only crate.
//!
//! Why a new shape instead of reusing `ModelMetadata`:
//! - `ModelMetadata` is xvision's *internal* per-model budget hints
//!   (reasoning class, recommended_visible_output) — those are
//!   xvision-side editorial decisions, not facts the provider exposes.
//! - `ModelEntry` carries the *provider-supplied* facts (context_window,
//!   max_output_tokens, pricing, raw json). Editorial overlays compose
//!   on top of catalog entries at lookup time.
//!
//! Provider API → `ModelEntry` mapping (PR #1 covers the first three):
//!
//! | Provider                  | Endpoint                          | Notes                                    |
//! |---------------------------|-----------------------------------|------------------------------------------|
//! | Anthropic                 | `GET /v1/models`                  | `id`, `display_name`, `created_at`       |
//! | OpenRouter                | `GET /api/v1/models`              | Rich: `context_length`, `top_provider.max_completion_tokens`, pricing |
//! | OpenAI-compat (DeepSeek,  | `GET /v1/models`                  | Usually just `id`+`object`; no metadata. |
//! |   Groq, Together, etc.)   |                                   | Catalog entries carry only the id list.  |
//! | OpenAI proper             | `GET /v1/models`                  | Same shape as OpenAI-compat — no limits. |

use serde::{Deserialize, Serialize};

/// One entry in a provider's model catalog. Optional fields capture the
/// reality that not every provider exposes every fact — OpenAI's
/// `/v1/models` returns just `id` and `object`, so for those rows
/// `context_window` and `max_output_tokens` will be `None` and callers
/// must fall back to operator-supplied values or xvision's editorial
/// `ModelMetadata` overlay.
///
/// `raw` keeps the provider's original entry so future fields can be
/// surfaced without re-fetching.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelEntry {
    /// Provider-side model id as returned by the API. Used verbatim in
    /// agent slot config and as the wire-level identifier when
    /// dispatching. No normalization — preserving the provider's casing
    /// and exact form is load-bearing for OpenRouter-style
    /// `vendor/model` slugs.
    pub id: String,

    /// Human-readable name when the provider supplies one. Anthropic and
    /// OpenRouter set this; bare OpenAI-compat providers usually don't.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub display_name: Option<String>,

    /// Total context window (input + output tokens). When `None`, the UI
    /// should display "—" rather than fabricate a number.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub context_window: Option<u32>,

    /// Hard cap on output tokens for this model. When `Some`, the agent
    /// slot's `max_tokens` resolver uses this as the ceiling instead of
    /// the static `ModelMetadata::unknown_default` fallback.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_output_tokens: Option<u32>,

    /// Reasoning/thinking class. Some providers (OpenRouter, Anthropic)
    /// surface this explicitly; for others it's inferred from the model
    /// id by the fetcher. `None` means "unknown — assume standard".
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub supports_reasoning: Option<bool>,

    /// Tool-use / function-calling support when the provider exposes it.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub supports_tools: Option<bool>,

    /// Price per 1M input tokens in USD. Only OpenRouter exposes this
    /// today (Anthropic publishes pricing out-of-band).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pricing_per_million_input_usd: Option<f64>,

    /// Price per 1M output tokens in USD.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pricing_per_million_output_usd: Option<f64>,

    /// Provider's raw row, preserved verbatim. Lets future fields ride
    /// out without a re-fetch and gives the dashboard a debug surface.
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
    pub raw: serde_json::Value,
}

impl ModelEntry {
    /// Minimal entry — used by fetchers that have nothing beyond an id
    /// (most OpenAI-compat providers). Callers can layer more fields
    /// after construction.
    pub fn minimal(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: None,
            context_window: None,
            max_output_tokens: None,
            supports_reasoning: None,
            supports_tools: None,
            pricing_per_million_input_usd: None,
            pricing_per_million_output_usd: None,
            raw: serde_json::Value::Null,
        }
    }
}

/// One provider's worth of catalog data, plus provenance.
///
/// `fetched_at` is the source-of-truth for staleness checks. The cache
/// layer uses this to decide when to refresh; consumers can also display
/// "fetched N minutes ago" in the UI.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    /// Matches `ProviderEntry.name`. The provider this catalog belongs
    /// to — not the model vendor (a single OpenRouter provider entry
    /// can list models from many vendors).
    pub provider: String,

    /// Wall-clock time of the most recent successful fetch, in UTC.
    /// Cache freshness is computed against this.
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub fetched_at: chrono::DateTime<chrono::Utc>,

    /// The HTTP URL the fetcher hit. Stored for transparency and to
    /// help diagnose unexpected catalog contents.
    pub source_url: String,

    /// All models the provider returned, in the order the API gave them.
    /// We don't sort or dedupe — preserving provider-side order avoids
    /// surprising the UI when the upstream introduces new fields.
    pub models: Vec<ModelEntry>,
}

impl Catalog {
    /// Build a catalog with a `now` stamp.
    pub fn new(
        provider: impl Into<String>,
        source_url: impl Into<String>,
        models: Vec<ModelEntry>,
    ) -> Self {
        Self {
            provider: provider.into(),
            fetched_at: chrono::Utc::now(),
            source_url: source_url.into(),
            models,
        }
    }

    /// Look up a model by exact id match. Catalog entries preserve the
    /// provider's original id form, so the caller must pass the same id
    /// they got from the catalog (or from `enabled_models`).
    pub fn find(&self, id: &str) -> Option<&ModelEntry> {
        self.models.iter().find(|m| m.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_entry_has_none_for_all_optional_fields() {
        let m = ModelEntry::minimal("gpt-anything");
        assert_eq!(m.id, "gpt-anything");
        assert_eq!(m.context_window, None);
        assert_eq!(m.max_output_tokens, None);
        assert_eq!(m.supports_reasoning, None);
    }

    #[test]
    fn catalog_find_uses_exact_id_match() {
        let cat = Catalog::new(
            "openrouter",
            "https://openrouter.ai/api/v1/models",
            vec![
                ModelEntry::minimal("anthropic/claude-opus-4-7"),
                ModelEntry::minimal("deepseek/deepseek-v4-pro"),
            ],
        );
        assert!(cat.find("anthropic/claude-opus-4-7").is_some());
        assert!(cat.find("Anthropic/Claude-Opus-4-7").is_none()); // case-sensitive
        assert!(cat.find("deepseek-v4-pro").is_none()); // no prefix matching
    }

    #[test]
    fn serde_roundtrip_omits_none_fields() {
        let m = ModelEntry {
            id: "x".into(),
            display_name: Some("X".into()),
            context_window: Some(200_000),
            max_output_tokens: None,
            supports_reasoning: Some(true),
            supports_tools: None,
            pricing_per_million_input_usd: None,
            pricing_per_million_output_usd: None,
            raw: serde_json::json!({"id": "x"}),
        };
        let json = serde_json::to_string(&m).unwrap();
        // Optional `None` fields must not appear in serialized output —
        // this matters for the on-disk cache, where extra null fields
        // would balloon the file and obscure diffs.
        assert!(!json.contains("max_output_tokens"));
        assert!(!json.contains("supports_tools"));
        assert!(json.contains("\"context_window\":200000"));

        let parsed: ModelEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, m);
    }
}
