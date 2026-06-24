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
//! - [`ProviderKind::OpenaiCompat`] → a concrete Cline OpenAI-compatible
//!   provider id inferred from `base_url` (`openrouter`, `deepseek`, `groq`,
//!   etc.), falling back to Cline's generic `litellm` carrier for arbitrary
//!   OpenAI-compatible endpoints. `@cline/llms@0.0.41` does not register
//!   `"openai-compatible"` as a gateway provider id; it is an internal provider
//!   factory family.
//! - [`ProviderKind::LocalCandle`] → unsupported; aborts with a typed error
//!   (the in-process mock provider stays on `LlmDispatch`).

use xvision_core::config::{ProviderEntry, ProviderKind};

/// Cline gateway provider id for Anthropic (a Cline built-in id).
pub const CLINE_PROVIDER_ANTHROPIC: &str = "anthropic";
/// Generic Cline OpenAI-compatible provider used when `base_url` does not
/// match a more specific built-in provider.
pub const CLINE_PROVIDER_LITELLM: &str = "litellm";

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
    #[error(
        "provider kind {kind:?} has no Cline mapping (provider '{name}'); \
             it stays on the LlmDispatch fallback"
    )]
    Unsupported { name: String, kind: ProviderKind },
}

fn cline_openai_compat_provider_id(base_url: &str) -> &'static str {
    let lower = base_url.trim_end_matches('/').to_ascii_lowercase();
    if lower.contains("openrouter.ai") {
        "openrouter"
    } else if lower.contains("api.openai.com") {
        "openai-native"
    } else if lower.contains("api.deepseek.com") {
        "deepseek"
    } else if lower.contains("api.groq.com") {
        "groq"
    } else if lower.contains("api.x.ai") {
        "xai"
    } else if lower.contains("api.together.xyz") {
        "together"
    } else if lower.contains("api.fireworks.ai") {
        "fireworks"
    } else if lower.contains("api.mistral.ai") {
        "mistral"
    } else if lower.contains("localhost:11434")
        || lower.contains("127.0.0.1:11434")
        || lower.contains(":11434")
    {
        "ollama"
    } else if lower.contains("localhost:1234")
        || lower.contains("127.0.0.1:1234")
        || lower.contains(":1234")
    {
        "lmstudio"
    } else {
        CLINE_PROVIDER_LITELLM
    }
}

fn local_openai_base_url(base_url: &str) -> Option<String> {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.ends_with("/v1") {
        Some(trimmed.to_string())
    } else {
        Some(format!("{trimmed}/v1"))
    }
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
        ProviderKind::OpenaiCompat => {
            let provider_id = cline_openai_compat_provider_id(&entry.base_url);
            let base_url = if matches!(provider_id, "ollama" | "lmstudio") {
                local_openai_base_url(&entry.base_url)
            } else {
                Some(entry.base_url.clone()).filter(|s| !s.is_empty())
            };
            (provider_id.to_string(), base_url)
        }
        ProviderKind::Ollama => ("ollama".to_string(), local_openai_base_url(&entry.base_url)),
        ProviderKind::LlamaCpp => (
            CLINE_PROVIDER_LITELLM.to_string(),
            local_openai_base_url(&entry.base_url),
        ),
        ProviderKind::Vllm => (
            CLINE_PROVIDER_LITELLM.to_string(),
            Some(entry.base_url.clone()).filter(|s| !s.is_empty()),
        ),
        ProviderKind::LocalCandle => {
            return Err(ProviderMapError::Unsupported {
                name: entry.name.clone(),
                kind: entry.kind,
            })
        }
    };
    Ok(ClineProvider {
        provider_id,
        model_id: model_id.to_string(),
        base_url,
    })
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
    fn openai_compat_openrouter_maps_to_cline_openrouter() {
        let m = map_provider(
            &entry(ProviderKind::OpenaiCompat, "https://openrouter.ai/api/v1"),
            "x",
        )
        .unwrap();
        assert_eq!(m.provider_id, "openrouter");
        assert_eq!(m.base_url.as_deref(), Some("https://openrouter.ai/api/v1"));
    }

    #[test]
    fn openai_compat_unknown_base_url_uses_generic_litellm_carrier() {
        let m = map_provider(
            &entry(ProviderKind::OpenaiCompat, "https://proxy.example/v1"),
            "x",
        )
        .unwrap();
        assert_eq!(m.provider_id, CLINE_PROVIDER_LITELLM);
        assert_eq!(m.base_url.as_deref(), Some("https://proxy.example/v1"));
    }

    #[test]
    fn openai_compat_empty_base_url_uses_generic_litellm_without_base_override() {
        let m = map_provider(&entry(ProviderKind::OpenaiCompat, ""), "x").unwrap();
        assert_eq!(m.provider_id, CLINE_PROVIDER_LITELLM);
        assert_eq!(m.base_url, None);
    }

    #[test]
    fn openai_compat_ollama_native_root_maps_to_cline_openai_root() {
        let m = map_provider(
            &entry(ProviderKind::OpenaiCompat, "http://localhost:11434"),
            "lfm2.5:8b",
        )
        .unwrap();
        assert_eq!(m.provider_id, "ollama");
        assert_eq!(m.base_url.as_deref(), Some("http://localhost:11434/v1"));
    }

    #[test]
    fn openai_compat_remote_ollama_port_maps_to_cline_ollama() {
        let m = map_provider(
            &entry(ProviderKind::OpenaiCompat, "http://100.90.135.112:11434/"),
            "hf.co/unsloth/Qwen3-4B-Instruct-2507-GGUF:UD-Q4_K_XL",
        )
        .unwrap();
        assert_eq!(m.provider_id, "ollama");
        assert_eq!(m.base_url.as_deref(), Some("http://100.90.135.112:11434/v1"));
    }

    #[test]
    fn ollama_native_root_maps_to_cline_openai_root() {
        let m = map_provider(
            &entry(ProviderKind::Ollama, "http://localhost:11434"),
            "lfm2.5:8b",
        )
        .unwrap();
        assert_eq!(m.provider_id, "ollama");
        assert_eq!(m.base_url.as_deref(), Some("http://localhost:11434/v1"));
    }

    #[test]
    fn ollama_openai_root_is_not_double_v1_appended() {
        let m = map_provider(
            &entry(ProviderKind::Ollama, "http://localhost:11434/v1"),
            "lfm2.5:8b",
        )
        .unwrap();
        assert_eq!(m.provider_id, "ollama");
        assert_eq!(m.base_url.as_deref(), Some("http://localhost:11434/v1"));
    }

    #[test]
    fn llama_cpp_native_root_maps_to_cline_openai_root() {
        let m = map_provider(
            &entry(ProviderKind::LlamaCpp, "http://localhost:8080"),
            "qwen2.5-coder",
        )
        .unwrap();
        assert_eq!(m.provider_id, CLINE_PROVIDER_LITELLM);
        assert_eq!(m.base_url.as_deref(), Some("http://localhost:8080/v1"));
    }

    #[test]
    fn llama_cpp_openai_root_is_not_double_v1_appended() {
        let m = map_provider(
            &entry(ProviderKind::LlamaCpp, "http://localhost:8080/v1"),
            "qwen2.5-coder",
        )
        .unwrap();
        assert_eq!(m.provider_id, CLINE_PROVIDER_LITELLM);
        assert_eq!(m.base_url.as_deref(), Some("http://localhost:8080/v1"));
    }

    #[test]
    fn lmstudio_native_root_maps_to_cline_openai_root() {
        let m = map_provider(
            &entry(ProviderKind::OpenaiCompat, "http://localhost:1234"),
            "qwen2.5-coder",
        )
        .unwrap();
        assert_eq!(m.provider_id, "lmstudio");
        assert_eq!(m.base_url.as_deref(), Some("http://localhost:1234/v1"));
    }

    #[test]
    fn lmstudio_openai_root_is_not_double_v1_appended() {
        let m = map_provider(
            &entry(ProviderKind::OpenaiCompat, "http://localhost:1234/v1"),
            "qwen2.5-coder",
        )
        .unwrap();
        assert_eq!(m.provider_id, "lmstudio");
        assert_eq!(m.base_url.as_deref(), Some("http://localhost:1234/v1"));
    }

    #[test]
    fn vllm_keeps_openai_root_on_litellm_carrier() {
        let m = map_provider(
            &entry(ProviderKind::Vllm, "http://localhost:8000/v1"),
            "Qwen/Qwen3-8B",
        )
        .unwrap();
        assert_eq!(m.provider_id, CLINE_PROVIDER_LITELLM);
        assert_eq!(m.base_url.as_deref(), Some("http://localhost:8000/v1"));
    }

    #[test]
    fn vllm_bare_base_url_is_left_as_configured() {
        let m = map_provider(
            &entry(ProviderKind::Vllm, "http://localhost:8000"),
            "Qwen/Qwen3-8B",
        )
        .unwrap();
        assert_eq!(m.provider_id, CLINE_PROVIDER_LITELLM);
        assert_eq!(m.base_url.as_deref(), Some("http://localhost:8000"));
    }

    #[test]
    fn local_candle_is_unmappable_and_aborts() {
        let err = map_provider(&entry(ProviderKind::LocalCandle, ""), "x").unwrap_err();
        assert!(matches!(err, ProviderMapError::Unsupported { .. }));
    }
}
