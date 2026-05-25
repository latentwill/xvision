//! Maps an xvision [`ProviderEntry`] onto the Cline SDK gateway provider
//! identity used in [`crate::protocol::StartRunParams`].
//!
//! Cline (`@cline/llms@0.0.41`) exposes an ai-sdk-style gateway with per-kind
//! provider factories — `createAnthropicProvider`, `createOpenAICompatibleProvider`,
//! `createOpenAIProvider` — selected by a `providerId` string
//! (`ProviderId = BuiltInProviderId | (string & {})`). The `xvision-agentd`
//! sidecar registers those factories on its `createGateway()` instance under the
//! ids defined here, so the Rust side only needs a stable `(provider_id,
//! model_id, base_url)` contract.
//!
//! Mapping:
//! - [`ProviderKind::Anthropic`] → `"anthropic"` (also a Cline built-in id, so it
//!   works through Cline's internal construction even before the gateway lands).
//! - [`ProviderKind::OpenaiCompat`] → `"openai-compat"` with `base_url`
//!   passed through; the sidecar configures `createOpenAICompatibleProvider`
//!   with that base URL. Routing a known OpenAI-compatible service to its
//!   specific Cline built-in id (`openrouter`, `deepseek`, `groq`, `together`,
//!   `fireworks`, `mistral`, `xai`, …) to unlock provider-native features is a
//!   documented refinement (see `docs/superpowers/specs/2026-05-24-cline-provider-matrix.md`);
//!   the generic compat path already covers the whole family via `base_url`.
//! - [`ProviderKind::LocalCandle`] → unsupported; aborts with a typed error
//!   (the in-process mock provider stays on `LlmDispatch`).

use xvision_core::config::{ProviderEntry, ProviderKind};

/// Cline gateway provider id for Anthropic (a Cline built-in id).
pub const CLINE_PROVIDER_ANTHROPIC: &str = "anthropic";
/// Cline gateway provider id under which the sidecar registers
/// `createOpenAICompatibleProvider`. Mirrors Cline's `"openai-compat"`
/// provider *category*; `base_url` is the per-service discriminant.
pub const CLINE_PROVIDER_OPENAI_COMPAT: &str = "openai-compat";

/// A resolved Cline gateway selection for one slot invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClineProvider {
    pub provider_id: String,
    pub model_id: String,
    /// `None` for providers that don't take a base URL (Anthropic); `Some`
    /// for the OpenAI-compatible family. Never `Some("")`.
    pub base_url: Option<String>,
}

/// Why a provider could not be mapped onto the Cline runtime.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderMapError {
    #[error("provider kind {kind:?} has no Cline mapping (provider '{name}'); \
             it stays on the LlmDispatch fallback")]
    Unsupported { name: String, kind: ProviderKind },
}

/// Map an xvision provider + model onto a Cline gateway selection.
///
/// Returns [`ProviderMapError::Unsupported`] (a hard abort, never a silent
/// fallback) for kinds with no Cline mapping — per the Stage 1 provider-matrix
/// contract, an unmapped provider aborts unless the runtime flag explicitly
/// selects `LlmDispatch`.
pub fn map_provider(entry: &ProviderEntry, model_id: &str) -> Result<ClineProvider, ProviderMapError> {
    let (provider_id, base_url) = match entry.kind {
        ProviderKind::Anthropic => (CLINE_PROVIDER_ANTHROPIC.to_string(), None),
        ProviderKind::OpenaiCompat => (
            CLINE_PROVIDER_OPENAI_COMPAT.to_string(),
            Some(entry.base_url.clone()).filter(|s| !s.is_empty()),
        ),
        ProviderKind::LocalCandle => {
            return Err(ProviderMapError::Unsupported {
                name: entry.name.clone(),
                kind: entry.kind,
            })
        }
    };
    Ok(ClineProvider { provider_id, model_id: model_id.to_string(), base_url })
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_core::config::{ProviderEntry, ProviderKind};

    fn entry(kind: ProviderKind, base_url: &str) -> ProviderEntry {
        ProviderEntry {
            name: "p".into(),
            kind,
            base_url: base_url.into(),
            api_key_env: "K".into(),
            enabled_models: vec!["m".into()],
        }
    }

    #[test]
    fn anthropic_maps_to_cline_anthropic() {
        let m = map_provider(&entry(ProviderKind::Anthropic, ""), "claude-opus-4-7").unwrap();
        assert_eq!(m.provider_id, CLINE_PROVIDER_ANTHROPIC);
        assert_eq!(m.model_id, "claude-opus-4-7");
        assert_eq!(m.base_url, None);
    }

    #[test]
    fn openai_compat_passes_base_url_through() {
        let m = map_provider(
            &entry(ProviderKind::OpenaiCompat, "https://openrouter.ai/api/v1"),
            "x",
        )
        .unwrap();
        assert_eq!(m.provider_id, CLINE_PROVIDER_OPENAI_COMPAT);
        assert_eq!(m.base_url.as_deref(), Some("https://openrouter.ai/api/v1"));
    }

    #[test]
    fn openai_compat_empty_base_url_becomes_none() {
        let m = map_provider(&entry(ProviderKind::OpenaiCompat, ""), "x").unwrap();
        assert_eq!(m.base_url, None);
    }

    #[test]
    fn local_candle_is_unmappable_and_aborts() {
        let err = map_provider(&entry(ProviderKind::LocalCandle, ""), "x").unwrap_err();
        assert!(matches!(err, ProviderMapError::Unsupported { .. }));
    }
}
