//! Pure embedder-source resolution for the Cortex memory layer.
//!
//! Provisioning an embedder must NOT hard-depend on OpenAI. This module
//! holds the side-effect-free decision function the engine startup wiring
//! calls (`crate::api::build_default_embedder`) after it has gathered the
//! relevant env vars + the operator's configured providers + their
//! resolved API keys.
//!
//! Resolution order (Cortex deployment; amended 2026-06-05 so memory works
//! out of the box without an external provider — the final fallback is now
//! the offline `Local` embedder, NOT `None`. A real provider is still
//! PREFERRED automatically: semantic > lexical):
//!
//! Env overrides win, then the persisted config, then `auto` → Local.
//!
//!   1. `XVN_MEMORY_EMBEDDER=local` → [`EmbedderChoice::Local`];
//!      `XVN_MEMORY_EMBEDDER=off` → [`EmbedderChoice::None`] (ops force-off).
//!   2. `XVN_MEMORY_EMBEDDER_PROVIDER=<name>` → resolve that provider's
//!      `base_url` + key and build an OpenAI-compatible `/embeddings`
//!      embedder, even when the provider is NOT api.openai.com (explicit
//!      operator opt-in). Unresolvable → fall through to the auto path
//!      (NOT `None`).
//!   3. `OPENAI_API_KEY` set → the historical OpenAI env path, base URL
//!      from `OPENAI_BASE_URL` (default `https://api.openai.com/v1`).
//!   4. `config_embedder` (from `$XVN_HOME/config/memory.toml` via the
//!      settings card): `"off"` → `None`; `"local"` → `Local`; a provider
//!      name (not `"auto"`) → that provider (resolve; if unresolvable →
//!      `Local` with a warn, NOT `None`).
//!   5. `"auto"` / unset (DEFAULT): auto-detect an enabled provider with a
//!      resolvable key whose `base_url` points at the REAL api.openai.com
//!      (guaranteed to serve `/embeddings`; conservative — never auto-pick
//!      deepseek/other providers that may lack an embeddings endpoint).
//!      ELSE → [`EmbedderChoice::Local`] (the new offline final fallback).
//!
//! Net effect: with nothing configured, default = `Auto` = `Local`, so
//! memory works offline; a real OpenAI key/provider is preferred
//! automatically; explicit `off` (env or config) is the only way to get
//! `None`.
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
    /// Wins over `config_embedder_model`.
    pub memory_embedder_model: Option<String>,
    /// The persisted embedding model id from `$XVN_HOME/config/memory.toml`
    /// (`MemoryConfig.embedder_model`). Used when the env override above is
    /// unset; `None` → [`DEFAULT_EMBEDDER_MODEL`].
    pub config_embedder_model: Option<String>,
    /// `OPENAI_API_KEY`.
    pub openai_api_key: Option<String>,
    /// `OPENAI_BASE_URL`.
    pub openai_base_url: Option<String>,
    /// The persisted memory-settings embedder string from
    /// `$XVN_HOME/config/memory.toml` — one of `"off" | "local" | "auto" |
    /// <provider-name>`. `None` is treated as the default `"auto"`. Env
    /// overrides above (steps 1–3) win over this; see the module docs.
    pub config_embedder: Option<String>,
    /// The persisted custom-endpoint base URL from
    /// `$XVN_HOME/config/memory.toml` (`MemoryConfig.embedder_base_url`).
    /// Only consulted when `config_embedder == "custom"`: a non-empty value
    /// builds a no-auth `OpenAiCompat` against this base URL; empty/`None`
    /// degrades to `Local` with a warn.
    pub config_embedder_base_url: Option<String>,
    /// `XVN_MEMORY_EMBEDDER_BASE_URL` — optional env override that forces a
    /// custom no-auth (or `OPENAI_API_KEY`-keyed) OpenAI-compatible endpoint,
    /// high in the precedence. Empty/unset → ignored.
    pub memory_embedder_base_url: Option<String>,
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

/// Kinds (the `EffectiveProvider::kind` string) that treat an empty API key
/// as no-auth — a local embeddings server that needs no credential. Mirrors
/// the `optional_auth` / `no_auth_kind` notion in
/// `api::settings::providers` (Ollama / llama.cpp / vLLM / local-candle). A
/// selected provider of one of these kinds resolves to `OpenAiCompat` with
/// an empty `api_key` rather than falling through to `Local`.
fn is_no_auth_kind(kind: &str) -> bool {
    matches!(kind, "ollama" | "llama-cpp" | "vllm" | "local-candle")
}

/// Try to build an `OpenAiCompat` choice from a named provider. Returns
/// `None` when the provider isn't found, or when it genuinely needs a key
/// (auth-bearing kind) and none is resolvable — so the caller can decide
/// the fallback. A no-auth kind (Ollama / llama.cpp / vLLM / local-candle)
/// resolves even with no key, using whatever key is present (possibly
/// empty).
fn provider_choice(
    env: &EmbedderEnv,
    providers: &[EffectiveProvider],
    name: &str,
    model: &str,
) -> Option<EmbedderChoice> {
    let p = providers.iter().find(|p| p.provider == name)?;
    let key = match resolved_key(env, name) {
        Some(k) => k.to_string(),
        // No usable key: only proceed for no-auth kinds (empty key is fine);
        // an auth-bearing provider with no key falls through (returns None).
        None if is_no_auth_kind(&p.kind) => String::new(),
        None => return None,
    };
    Some(EmbedderChoice::OpenAiCompat {
        base_url: p.base_url.clone(),
        api_key: key,
        model: model.to_string(),
    })
}

/// Conservative auto-detect: only a real api.openai.com provider with a
/// resolvable key. `None` when none match.
fn auto_openai_choice(
    env: &EmbedderEnv,
    providers: &[EffectiveProvider],
    model: &str,
) -> Option<EmbedderChoice> {
    for p in providers {
        if p.enabled && is_real_openai(&p.base_url) {
            if let Some(key) = resolved_key(env, &p.provider) {
                return Some(EmbedderChoice::OpenAiCompat {
                    base_url: p.base_url.clone(),
                    api_key: key.to_string(),
                    model: model.to_string(),
                });
            }
        }
    }
    None
}

/// Resolve which embedder the engine should provision. Pure — see the
/// module docs for the resolution order. The `auto`/default tail now falls
/// back to [`EmbedderChoice::Local`] (offline, lexical) rather than `None`
/// so memory works out of the box; only an explicit `off` (env or config)
/// yields `None`.
pub fn resolve_embedder_choice(env: &EmbedderEnv, providers: &[EffectiveProvider]) -> EmbedderChoice {
    // Model precedence: env override → persisted config model → default.
    let model = non_empty(&env.memory_embedder_model)
        .or_else(|| non_empty(&env.config_embedder_model))
        .unwrap_or(DEFAULT_EMBEDDER_MODEL)
        .to_string();

    // 1. Explicit env override: local forces offline; off forces no-op.
    if let Some(v) = non_empty(&env.memory_embedder) {
        if v.eq_ignore_ascii_case("local") {
            return EmbedderChoice::Local;
        }
        if v.eq_ignore_ascii_case("off") {
            return EmbedderChoice::None;
        }
    }

    // 1b. Explicit env custom endpoint override: force a no-auth (or
    //     OPENAI_API_KEY-keyed) OpenAI-compatible endpoint at this base URL.
    //     Used verbatim (trimmed); ops can force a custom embeddings host
    //     without touching providers or the settings card.
    if let Some(base_url) = non_empty(&env.memory_embedder_base_url) {
        return EmbedderChoice::OpenAiCompat {
            base_url: base_url.to_string(),
            api_key: non_empty(&env.openai_api_key).unwrap_or("").to_string(),
            model,
        };
    }

    // 2. Explicit env provider opt-in. The named provider wins even when it
    //    is not api.openai.com — the operator asserts it serves /embeddings.
    //    Unresolvable → fall through to the auto path (NOT None), so memory
    //    still works via OPENAI_API_KEY / auto-detect / Local fallback.
    if let Some(name) = non_empty(&env.memory_embedder_provider) {
        if let Some(choice) = provider_choice(env, providers, name, &model) {
            return choice;
        }
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

    // 4. Config-backed embedder (from memory.toml). `None`/unset ≡ "auto".
    //    `off` → None; `local` → Local; a provider name (not "auto") →
    //    that provider; unresolvable provider name → Local (with a warn),
    //    NOT None — memory still works offline.
    let config = non_empty(&env.config_embedder).unwrap_or("auto");
    if config.eq_ignore_ascii_case("off") {
        return EmbedderChoice::None;
    }
    if config.eq_ignore_ascii_case("local") {
        return EmbedderChoice::Local;
    }
    if config.eq_ignore_ascii_case("custom") {
        // No-auth custom OpenAI-compatible endpoint typed in the card. Use the
        // base URL verbatim (trimmed), with an empty api_key. A missing/blank
        // base URL degrades to Local (best-effort; never crash).
        if let Some(base_url) = non_empty(&env.config_embedder_base_url) {
            return EmbedderChoice::OpenAiCompat {
                base_url: base_url.to_string(),
                api_key: String::new(),
                model,
            };
        }
        tracing::warn!(
            "memory: custom embedder selected but no base_url set; \
             falling back to the offline LocalEmbedder"
        );
        return EmbedderChoice::Local;
    }
    if !config.eq_ignore_ascii_case("auto") {
        // Named provider from the settings card.
        if let Some(choice) = provider_choice(env, providers, config, &model) {
            return choice;
        }
        tracing::warn!(
            provider = %config,
            "memory: configured embedder provider has no usable key; \
             falling back to the offline LocalEmbedder (lexical quality)"
        );
        return EmbedderChoice::Local;
    }

    // 5. Auto (the default): prefer a real api.openai.com provider with a
    //    key; ELSE fall back to the offline Local embedder so memory works
    //    out of the box.
    if let Some(choice) = auto_openai_choice(env, providers, &model) {
        return choice;
    }
    EmbedderChoice::Local
}

impl EmbedderChoice {
    /// `embedder_id` the resolved choice will report, mirroring each
    /// concrete embedder's `Embedder::id()`. `None` for
    /// [`EmbedderChoice::None`]. Used by `xvn memory status` / `xvn doctor`
    /// to surface the embedder without instantiating it. The `OpenAiCompat`
    /// id is model-aware (`openaicompat:<model>`), matching
    /// `OpenAiEmbedder::id()`, so the store keeps nomic / qwen / openai
    /// embeddings in separate vector spaces.
    pub fn embedder_id(&self) -> Option<String> {
        match self {
            EmbedderChoice::Local => Some("local:hash-v1".to_string()),
            EmbedderChoice::OpenAiCompat { model, .. } => Some(format!("openaicompat:{model}")),
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
        ep_kind(name, base_url, has_key, "openai-compat")
    }

    fn ep_kind(name: &str, base_url: &str, has_key: bool, kind: &str) -> EffectiveProvider {
        EffectiveProvider {
            provider: name.to_string(),
            kind: kind.to_string(),
            base_url: base_url.to_string(),
            api_key_env: String::new(),
            expected_api_key_env: String::new(),
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
    fn empty_resolved_key_falls_back_to_local() {
        // Under the default `auto`, an openai provider with no usable key
        // no longer yields None — it falls back to the offline Local
        // embedder so memory still works.
        let mut env = EmbedderEnv::default();
        env.resolved_provider_keys.insert("openai".into(), "  ".into());
        let providers = vec![ep("openai", "https://api.openai.com/v1", true)];
        assert_eq!(resolve_embedder_choice(&env, &providers), EmbedderChoice::Local);
    }

    #[test]
    fn config_auto_no_providers_falls_back_to_local() {
        // The headline change: nothing configured (default = auto) and no
        // providers/keys → Local, not None. Memory works out of the box.
        let env = EmbedderEnv::default();
        assert_eq!(resolve_embedder_choice(&env, &[]), EmbedderChoice::Local);
    }

    #[test]
    fn config_off_yields_none() {
        let env = EmbedderEnv {
            config_embedder: Some("off".into()),
            ..Default::default()
        };
        assert_eq!(resolve_embedder_choice(&env, &[]), EmbedderChoice::None);
    }

    #[test]
    fn env_off_overrides_config_auto() {
        // Ops force-off via env wins over config auto.
        let env = EmbedderEnv {
            memory_embedder: Some("off".into()),
            config_embedder: Some("auto".into()),
            ..Default::default()
        };
        assert_eq!(resolve_embedder_choice(&env, &[]), EmbedderChoice::None);
    }

    #[test]
    fn config_local_yields_local() {
        let env = EmbedderEnv {
            config_embedder: Some("local".into()),
            ..Default::default()
        };
        assert_eq!(resolve_embedder_choice(&env, &[]), EmbedderChoice::Local);
    }

    #[test]
    fn resolvable_openai_provider_beats_local_under_auto() {
        // A real openai provider with a key is PREFERRED over the Local
        // fallback when the config is auto (semantic > lexical).
        let mut env = EmbedderEnv::default();
        env.resolved_provider_keys
            .insert("openai".into(), "sk-live".into());
        let providers = vec![ep("openai", "https://api.openai.com/v1", true)];
        match resolve_embedder_choice(&env, &providers) {
            EmbedderChoice::OpenAiCompat { api_key, .. } => assert_eq!(api_key, "sk-live"),
            other => panic!("expected OpenAiCompat, got {other:?}"),
        }
    }

    #[test]
    fn config_naming_unresolvable_provider_falls_back_to_local() {
        // Config names a provider that has no usable key → Local, NOT None.
        let env = EmbedderEnv {
            config_embedder: Some("myproxy".into()),
            ..Default::default()
        };
        let providers = vec![ep("myproxy", "https://proxy.internal/v1", false)];
        assert_eq!(resolve_embedder_choice(&env, &providers), EmbedderChoice::Local);
    }

    #[test]
    fn config_ollama_provider_no_key_resolves_with_empty_key_and_model() {
        // A selected Ollama-kind provider with NO key must resolve to
        // OpenAiCompat with an empty api_key (no-auth) + the config model —
        // it must NOT fall through to Local.
        let env = EmbedderEnv {
            config_embedder: Some("ollama".into()),
            config_embedder_model: Some("nomic-embed-text".into()),
            ..Default::default()
        };
        let providers = vec![ep_kind("ollama", "http://localhost:11434/v1", false, "ollama")];
        match resolve_embedder_choice(&env, &providers) {
            EmbedderChoice::OpenAiCompat {
                base_url,
                api_key,
                model,
            } => {
                assert_eq!(base_url, "http://localhost:11434/v1");
                assert_eq!(api_key, "");
                assert_eq!(model, "nomic-embed-text");
            }
            other => panic!("expected OpenAiCompat for no-auth Ollama, got {other:?}"),
        }
    }

    #[test]
    fn env_model_overrides_config_model() {
        // XVN_MEMORY_EMBEDDER_MODEL beats the persisted config model.
        let env = EmbedderEnv {
            config_embedder: Some("ollama".into()),
            config_embedder_model: Some("nomic-embed-text".into()),
            memory_embedder_model: Some("qwen3-embedding".into()),
            ..Default::default()
        };
        let providers = vec![ep_kind("ollama", "http://localhost:11434/v1", false, "ollama")];
        match resolve_embedder_choice(&env, &providers) {
            EmbedderChoice::OpenAiCompat { model, .. } => {
                assert_eq!(model, "qwen3-embedding");
            }
            other => panic!("expected OpenAiCompat, got {other:?}"),
        }
    }

    #[test]
    fn config_model_alone_threads_into_auto_openai() {
        // config_embedder_model with the default auto path (real openai
        // provider + key) sets the model on the resolved choice.
        let mut env = EmbedderEnv {
            config_embedder_model: Some("text-embedding-3-large".into()),
            ..Default::default()
        };
        env.resolved_provider_keys
            .insert("openai".into(), "sk-live".into());
        let providers = vec![ep("openai", "https://api.openai.com/v1", true)];
        match resolve_embedder_choice(&env, &providers) {
            EmbedderChoice::OpenAiCompat { model, .. } => {
                assert_eq!(model, "text-embedding-3-large");
            }
            other => panic!("expected OpenAiCompat, got {other:?}"),
        }
    }

    #[test]
    fn config_naming_auth_provider_without_key_still_falls_back_to_local() {
        // An auth-bearing (openai-compat) provider with no key still falls
        // through to Local — the no-auth relaxation is kind-gated.
        let env = EmbedderEnv {
            config_embedder: Some("myproxy".into()),
            ..Default::default()
        };
        let providers = vec![ep("myproxy", "https://proxy.internal/v1", false)];
        assert_eq!(resolve_embedder_choice(&env, &providers), EmbedderChoice::Local);
    }

    #[test]
    fn config_custom_with_base_url_yields_openai_compat_no_auth() {
        // The custom-endpoint path: config == "custom" + a typed base_url →
        // OpenAiCompat with the base_url verbatim, an EMPTY api_key (no-auth),
        // and the config model. No provider registration involved.
        let env = EmbedderEnv {
            config_embedder: Some("custom".into()),
            config_embedder_base_url: Some("http://localhost:11434/v1".into()),
            config_embedder_model: Some("nomic-embed-text".into()),
            ..Default::default()
        };
        match resolve_embedder_choice(&env, &[]) {
            EmbedderChoice::OpenAiCompat {
                base_url,
                api_key,
                model,
            } => {
                assert_eq!(base_url, "http://localhost:11434/v1");
                assert_eq!(api_key, "");
                assert_eq!(model, "nomic-embed-text");
            }
            other => panic!("expected OpenAiCompat for custom endpoint, got {other:?}"),
        }
    }

    #[test]
    fn config_custom_without_base_url_falls_back_to_local() {
        // custom selected but no base_url → Local (best-effort; never crash).
        let env = EmbedderEnv {
            config_embedder: Some("custom".into()),
            config_embedder_base_url: None,
            ..Default::default()
        };
        assert_eq!(resolve_embedder_choice(&env, &[]), EmbedderChoice::Local);
    }

    #[test]
    fn config_custom_blank_base_url_falls_back_to_local() {
        // Whitespace-only base_url is treated as unset → Local.
        let env = EmbedderEnv {
            config_embedder: Some("custom".into()),
            config_embedder_base_url: Some("   ".into()),
            ..Default::default()
        };
        assert_eq!(resolve_embedder_choice(&env, &[]), EmbedderChoice::Local);
    }

    #[test]
    fn env_model_overrides_config_model_on_custom_path() {
        // XVN_MEMORY_EMBEDDER_MODEL still wins for the custom endpoint.
        let env = EmbedderEnv {
            config_embedder: Some("custom".into()),
            config_embedder_base_url: Some("http://localhost:11434/v1".into()),
            config_embedder_model: Some("nomic-embed-text".into()),
            memory_embedder_model: Some("qwen3-embedding".into()),
            ..Default::default()
        };
        match resolve_embedder_choice(&env, &[]) {
            EmbedderChoice::OpenAiCompat { model, .. } => {
                assert_eq!(model, "qwen3-embedding");
            }
            other => panic!("expected OpenAiCompat, got {other:?}"),
        }
    }

    #[test]
    fn config_naming_resolvable_provider_uses_it() {
        let mut env = EmbedderEnv {
            config_embedder: Some("myproxy".into()),
            ..Default::default()
        };
        env.resolved_provider_keys
            .insert("myproxy".into(), "sk-proxy".into());
        let providers = vec![ep("myproxy", "https://proxy.internal/v1", true)];
        match resolve_embedder_choice(&env, &providers) {
            EmbedderChoice::OpenAiCompat {
                base_url, api_key, ..
            } => {
                assert_eq!(base_url, "https://proxy.internal/v1");
                assert_eq!(api_key, "sk-proxy");
            }
            other => panic!("expected OpenAiCompat, got {other:?}"),
        }
    }
}
