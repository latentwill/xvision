//! Phase 0 (Cortex deployment) — provider-aware embedder provisioning.
//!
//! These tests exercise the PURE `resolve_embedder_choice` decision
//! function (no network, no env mutation) plus the no-embedder
//! degradation contract on `MemoryRecorder`. The resolution order under
//! test is the one locked in the Phase 0 plan:
//!
//!   1. `XVN_MEMORY_EMBEDDER=local` → `Local`; `=off` → `None`
//!   2. `XVN_MEMORY_EMBEDDER_PROVIDER=<name>`  → that provider's url/key
//!   3. `OPENAI_API_KEY`                        → OpenAI env path
//!   4. config_embedder (memory.toml): off/local/<provider>
//!   5. auto / default: a real `api.openai.com` provider with a key, ELSE
//!      the offline `Local` embedder (amended 2026-06-05 — memory works out
//!      of the box; only explicit `off` yields `None`).

use std::collections::HashMap;

use xvision_engine::agent::embedder_choice::{resolve_embedder_choice, EmbedderChoice, EmbedderEnv};
use xvision_engine::api::settings::providers::EffectiveProvider;

use xvision_memory::store::MemoryStore;
use xvision_engine::agent::memory_recorder::{MemoryRecorder, RecallResult};
use xvision_memory::types::MemoryMode;

use std::sync::Arc;

/// Build an `EffectiveProvider` fixture. `key` resolves into the
/// `EmbedderEnv::resolved_provider_keys` map below — it is NOT read from
/// process env, so these tests never touch real credentials.
fn provider(name: &str, base_url: &str, has_key: bool) -> EffectiveProvider {
    EffectiveProvider {
        provider: name.to_string(),
        kind: "openai-compat".to_string(),
        base_url: base_url.to_string(),
        api_key_env: format!("{}_API_KEY", name.to_uppercase()),
        enabled: true,
        has_key,
        models: Vec::new(),
        launchable: has_key,
    }
}

fn env_with_keys(keys: &[(&str, &str)]) -> EmbedderEnv {
    let mut resolved = HashMap::new();
    for (name, key) in keys {
        resolved.insert(name.to_string(), key.to_string());
    }
    EmbedderEnv {
        memory_embedder: None,
        memory_embedder_provider: None,
        memory_embedder_model: None,
        openai_api_key: None,
        openai_base_url: None,
        config_embedder: None,
        resolved_provider_keys: resolved,
    }
}

#[test]
fn auto_detects_real_openai_provider_with_key() {
    // No explicit overrides; a configured openai-compat provider pointed
    // at the real api.openai.com with a resolvable key should be picked.
    let env = env_with_keys(&[("openai", "sk-live")]);
    let providers = vec![provider("openai", "https://api.openai.com/v1", true)];

    let choice = resolve_embedder_choice(&env, &providers);
    match choice {
        EmbedderChoice::OpenAiCompat {
            base_url,
            api_key,
            model,
        } => {
            assert_eq!(base_url, "https://api.openai.com/v1");
            assert_eq!(api_key, "sk-live");
            assert_eq!(model, "text-embedding-3-small");
        }
        other => panic!("expected OpenAiCompat, got {other:?}"),
    }
}

#[test]
fn does_not_auto_detect_non_openai_provider() {
    // A deepseek provider (NOT api.openai.com) must NOT be auto-picked —
    // it may lack an /embeddings endpoint. Without an explicit
    // XVN_MEMORY_EMBEDDER_PROVIDER opt-in the auto path skips it and falls
    // back to the offline Local embedder (NOT the deepseek provider, and
    // NOT None — memory still works out of the box).
    let env = env_with_keys(&[("deepseek", "sk-deepseek")]);
    let providers = vec![provider("deepseek", "https://api.deepseek.com/v1", true)];

    let choice = resolve_embedder_choice(&env, &providers);
    assert!(
        matches!(choice, EmbedderChoice::Local),
        "non-openai provider must not be auto-selected; auto falls back to Local, got {choice:?}"
    );
}

#[test]
fn explicit_provider_opt_in_uses_that_provider_even_if_not_openai() {
    // With XVN_MEMORY_EMBEDDER_PROVIDER=myproxy, the named provider's
    // base_url/key wins even though it is not api.openai.com.
    let mut env = env_with_keys(&[("myproxy", "sk-proxy")]);
    env.memory_embedder_provider = Some("myproxy".to_string());
    let providers = vec![provider("myproxy", "https://proxy.internal/v1", true)];

    let choice = resolve_embedder_choice(&env, &providers);
    match choice {
        EmbedderChoice::OpenAiCompat {
            base_url,
            api_key,
            model,
        } => {
            assert_eq!(base_url, "https://proxy.internal/v1");
            assert_eq!(api_key, "sk-proxy");
            assert_eq!(model, "text-embedding-3-small");
        }
        other => panic!("expected OpenAiCompat for explicit opt-in, got {other:?}"),
    }
}

#[test]
fn explicit_provider_model_override_is_threaded() {
    let mut env = env_with_keys(&[("myproxy", "sk-proxy")]);
    env.memory_embedder_provider = Some("myproxy".to_string());
    env.memory_embedder_model = Some("text-embedding-3-large".to_string());
    let providers = vec![provider("myproxy", "https://proxy.internal/v1", true)];

    let choice = resolve_embedder_choice(&env, &providers);
    match choice {
        EmbedderChoice::OpenAiCompat { model, .. } => {
            assert_eq!(model, "text-embedding-3-large");
        }
        other => panic!("expected OpenAiCompat, got {other:?}"),
    }
}

#[test]
fn openai_api_key_env_path() {
    // No providers, but OPENAI_API_KEY is set → OpenAI env path.
    let mut env = env_with_keys(&[]);
    env.openai_api_key = Some("sk-env".to_string());

    let choice = resolve_embedder_choice(&env, &[]);
    match choice {
        EmbedderChoice::OpenAiCompat {
            base_url,
            api_key,
            ..
        } => {
            assert_eq!(base_url, "https://api.openai.com/v1");
            assert_eq!(api_key, "sk-env");
        }
        other => panic!("expected OpenAiCompat from env, got {other:?}"),
    }
}

#[test]
fn no_providers_no_key_defaults_to_local() {
    // Amended (Cortex deployment): with nothing configured the default
    // `auto` falls back to the offline Local embedder, NOT None, so memory
    // works out of the box. Explicit `off` is the only path to None (see
    // `config_off_yields_none` / `env_off_yields_none`).
    let env = env_with_keys(&[]);
    let choice = resolve_embedder_choice(&env, &[]);
    assert!(
        matches!(choice, EmbedderChoice::Local),
        "expected Local fallback, got {choice:?}"
    );
}

#[test]
fn config_off_yields_none() {
    let mut env = env_with_keys(&[]);
    env.config_embedder = Some("off".to_string());
    let choice = resolve_embedder_choice(&env, &[]);
    assert!(
        matches!(choice, EmbedderChoice::None),
        "config off must disable the embedder, got {choice:?}"
    );
}

#[test]
fn env_off_yields_none() {
    let mut env = env_with_keys(&[]);
    env.memory_embedder = Some("off".to_string());
    let choice = resolve_embedder_choice(&env, &[]);
    assert!(
        matches!(choice, EmbedderChoice::None),
        "env XVN_MEMORY_EMBEDDER=off must disable the embedder, got {choice:?}"
    );
}

#[test]
fn local_flag_selects_local() {
    let mut env = env_with_keys(&[]);
    env.memory_embedder = Some("local".to_string());
    // Even with a real openai provider present, the explicit local flag
    // takes precedence (it's resolution step 1).
    let providers = vec![provider("openai", "https://api.openai.com/v1", true)];

    let choice = resolve_embedder_choice(&env, &providers);
    assert!(
        matches!(choice, EmbedderChoice::Local),
        "expected Local, got {choice:?}"
    );
}

#[test]
fn provider_without_resolvable_key_is_skipped() {
    // has_key=false (or key not in the resolved map) means the provider
    // can't be used — the auto path skips it and falls back to the offline
    // Local embedder (NOT None).
    let env = env_with_keys(&[]);
    let providers = vec![provider("openai", "https://api.openai.com/v1", false)];

    let choice = resolve_embedder_choice(&env, &providers);
    assert!(
        matches!(choice, EmbedderChoice::Local),
        "provider with no key must not be selected; auto falls back to Local, got {choice:?}"
    );
}

#[tokio::test]
async fn recorder_without_embedder_returns_no_embedder() {
    // Proves the no-embedder degradation contract: a recorder built via
    // `new` (no embedder) returns `NoEmbedder` for a non-Off recall
    // rather than crashing or silently succeeding.
    let store = Arc::new(MemoryStore::open_in_memory().await.expect("open store"));
    let recorder = MemoryRecorder::new(store);

    let result = recorder
        .recall(MemoryMode::Global, "agent-a", "query text", 4, None, 0)
        .await
        .expect("recall must not error");

    match result {
        RecallResult::NoEmbedder { namespace } => {
            assert_eq!(namespace, "global");
        }
        other => panic!("expected NoEmbedder, got {other:?}"),
    }
}
