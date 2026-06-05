//! Pure embedder-source resolution for the Cortex memory layer.
//!
//! Provisioning an embedder must NOT hard-depend on OpenAI. This module
//! holds the side-effect-free decision function the engine startup wiring
//! calls (`crate::api::build_default_embedder`) after it has gathered the
//! relevant env vars + the operator's configured providers + their
//! resolved API keys.
//!
//! Resolution order (locked, Phase 0 Cortex deployment plan):
//!
//!   1. `XVN_MEMORY_EMBEDDER=local` → [`EmbedderChoice::Local`]
//!      (deterministic, offline, low-quality dev/offline fallback).
//!   2. `XVN_MEMORY_EMBEDDER_PROVIDER=<name>` → resolve that provider's
//!      `base_url` + key and build an OpenAI-compatible `/embeddings`
//!      embedder, even when the provider is NOT api.openai.com (explicit
//!      operator opt-in).
//!   3. `OPENAI_API_KEY` set → the historical OpenAI env path, base URL
//!      from `OPENAI_BASE_URL` (default `https://api.openai.com/v1`).
//!   4. Auto-detect: scan the providers for an enabled provider with a
//!      resolvable key whose `base_url` points at the REAL api.openai.com
//!      (guaranteed to serve `/embeddings`). Be conservative — never
//!      auto-pick deepseek/other providers that may lack an embeddings
//!      endpoint; those require the explicit opt-in (step 2).
//!   5. Otherwise [`EmbedderChoice::None`] — recall/record no-op. Never
//!      crashes startup.
//!
//! In every `OpenAiCompat` branch the model is `XVN_MEMORY_EMBEDDER_MODEL`
//! when set, otherwise [`DEFAULT_EMBEDDER_MODEL`].
//!
//! The function is pure so tests inject env vars + resolved keys directly
//! (see `tests/memory_embedder_provisioning.rs`) without touching process
//! env or the network. The async I/O (reading provider secrets, real env)
//! lives in the caller.

use std::collections::HashMap;

use crate::api::settings::providers::EffectiveProvider;

/// Default embedding model when the operator doesn't override via
/// `XVN_MEMORY_EMBEDDER_MODEL`. Matches `OpenAiEmbedder`'s default and
/// the 1536-dim memory store schema.
pub const DEFAULT_EMBEDDER_MODEL: &str = "text-embedding-3-small";

/// Captured, pre-resolved inputs to [`resolve_embedder_choice`]. Keeping
/// every relevant env var + the resolved provider keys on this struct
/// makes the decision testable in isolation: tests construct it directly,
/// production fills it from `std::env::var` + `resolve_provider_key_value`.
#[derive(Debug, Clone, Default)]
pub struct EmbedderEnv {
    /// `XVN_MEMORY_EMBEDDER` — `Some("local")` forces the offline
    /// deterministic embedder. Any other value is ignored (the
    /// provider/openai/auto path still applies).
    pub memory_embedder: Option<String>,
    /// `XVN_MEMORY_EMBEDDER_PROVIDER` — explicit provider name to use as
    /// the embeddings backend (OpenAI-compatible).
    pub memory_embedder_provider: Option<String>,
    /// `XVN_MEMORY_EMBEDDER_MODEL` — overrides the embedding model id.
    pub memory_embedder_model: Option<String>,
    /// `OPENAI_API_KEY`.
    pub openai_api_key: Option<String>,
    /// `OPENAI_BASE_URL`.
    pub openai_base_url: Option<String>,
    /// Provider name → resolved API key. Populated by the caller from
    /// `resolve_provider_key_value` (env var first, then stored secret)
    /// so this pure function never performs I/O. A provider missing from
    /// this map (or mapped to an empty string) is treated as having no
    /// usable key.
    pub resolved_provider_keys: HashMap<String, String>,
}

/// The resolved embedder source. The caller turns this into a concrete
/// `Arc<dyn Embedder>` (or `None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedderChoice {
    /// Offline deterministic embedder (`LocalEmbedder`).
    Local,
    /// OpenAI-compatible `/embeddings` endpoint.
    OpenAiCompat {
        base_url: String,
        api_key: String,
        model: String,
    },
    /// No embedder available — recall/record degrade to a no-op.
    None,
}

fn non_empty(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

/// Look up the usable (non-empty) resolved key for a provider name.
fn resolved_key<'a>(env: &'a EmbedderEnv, name: &str) -> Option<&'a str> {
    env.resolved_provider_keys
        .get(name)
        .map(String::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

/// True iff `base_url` points at the genuine OpenAI API host (which is
/// guaranteed to serve `/embeddings`). Deliberately narrow — any other
/// host requires the explicit `XVN_MEMORY_EMBEDDER_PROVIDER` opt-in.
fn is_real_openai(base_url: &str) -> bool {
    base_url.contains("api.openai.com")
}

/// Resolve which embedder the engine should provision. Pure — see the
/// module docs for the locked resolution order.
pub fn resolve_embedder_choice(env: &EmbedderEnv, providers: &[EffectiveProvider]) -> EmbedderChoice {
    let model = non_empty(&env.memory_embedder_model)
        .unwrap_or(DEFAULT_EMBEDDER_MODEL)
        .to_string();

    // 1. Explicit local override.
    if let Some(v) = non_empty(&env.memory_embedder) {
        if v.eq_ignore_ascii_case("local") {
            return EmbedderChoice::Local;
        }
    }

    // 2. Explicit provider opt-in. The named provider wins even when it is
    //    not api.openai.com — the operator asserts it serves /embeddings.
    if let Some(name) = non_empty(&env.memory_embedder_provider) {
        if let Some(p) = providers.iter().find(|p| p.provider == name) {
            if let Some(key) = resolved_key(env, name) {
                return EmbedderChoice::OpenAiCompat {
                    base_url: p.base_url.clone(),
                    api_key: key.to_string(),
                    model,
                };
            }
        }
        // Named-but-unresolvable provider: fall through. The caller logs
        // that the requested embedder provider had no usable key, then we
        // try the remaining steps so memory still works if OPENAI_API_KEY
        // or an auto-detectable provider is present.
    }

    // 3. OPENAI_API_KEY env path (historical behavior).
    if let Some(key) = non_empty(&env.openai_api_key) {
        let base_url = non_empty(&env.openai_base_url)
            .unwrap_or("https://api.openai.com/v1")
            .to_string();
        return EmbedderChoice::OpenAiCompat {
            base_url,
            api_key: key.to_string(),
            model,
        };
    }

    // 4. Conservative auto-detect: only a real api.openai.com provider
    //    with a resolvable key.
    for p in providers {
        if p.enabled && is_real_openai(&p.base_url) {
            if let Some(key) = resolved_key(env, &p.provider) {
                return EmbedderChoice::OpenAiCompat {
                    base_url: p.base_url.clone(),
                    api_key: key.to_string(),
                    model,
                };
            }
        }
    }

    // 5. Nothing configured — recall/record no-op.
    EmbedderChoice::None
}

impl EmbedderChoice {
    /// `embedder_id` the resolved choice will report, mirroring each
    /// concrete embedder's `Embedder::id()`. `None` for
    /// [`EmbedderChoice::None`]. Used by `xvn memory status` / `xvn doctor`
    /// to surface the embedder without instantiating it (the
    /// `OpenAiCompat` id is fixed to the default-model form today, matching
    /// `OpenAiEmbedder::id()`).
    pub fn embedder_id(&self) -> Option<&'static str> {
        match self {
            EmbedderChoice::Local => Some("local:hash-v1"),
            EmbedderChoice::OpenAiCompat { .. } => Some("openai:text-embedding-3-small"),
            EmbedderChoice::None => None,
        }
    }

    /// Operator-readable source label for the resolved choice — which
    /// resolution branch won. `None` when no embedder is configured.
    pub fn source_label(&self) -> Option<&'static str> {
        match self {
            EmbedderChoice::Local => Some("local"),
            EmbedderChoice::OpenAiCompat { .. } => Some("openai-compat"),
            EmbedderChoice::None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(name: &str, base_url: &str, has_key: bool) -> EffectiveProvider {
        EffectiveProvider {
            provider: name.to_string(),
            kind: "openai-compat".to_string(),
            base_url: base_url.to_string(),
            api_key_env: String::new(),
            enabled: true,
            has_key,
            models: Vec::new(),
            launchable: has_key,
        }
    }

    #[test]
    fn local_precedence_over_everything() {
        let mut env = EmbedderEnv {
            memory_embedder: Some("local".into()),
            openai_api_key: Some("sk".into()),
            ..Default::default()
        };
        env.resolved_provider_keys.insert("openai".into(), "sk".into());
        let providers = vec![ep("openai", "https://api.openai.com/v1", true)];
        assert_eq!(resolve_embedder_choice(&env, &providers), EmbedderChoice::Local);
    }

    #[test]
    fn empty_resolved_key_is_not_usable() {
        let mut env = EmbedderEnv::default();
        env.resolved_provider_keys.insert("openai".into(), "  ".into());
        let providers = vec![ep("openai", "https://api.openai.com/v1", true)];
        assert_eq!(resolve_embedder_choice(&env, &providers), EmbedderChoice::None);
    }
}
