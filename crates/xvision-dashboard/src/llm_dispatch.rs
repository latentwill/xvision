//! Provider-aware LLM dispatch resolution for the chat-rail / wizard
//! SSE routes. Loads `RuntimeConfig`, finds the requested provider (or
//! falls back to the `[intern]` default), reads its API key from env,
//! and hands back a boxed `LlmDispatch` of the right wire kind.
//!
//! Failure surfaces as a typed `DashboardError` so the HTTP handlers
//! can render a meaningful 4xx/5xx body instead of bubbling up a raw
//! anyhow chain.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use xvision_core::config::{ProviderEntry, ProviderKind, RuntimeConfig};
use xvision_engine::agent::llm::{AnthropicDispatch, LlmDispatch, OpenaiCompatDispatch};

use crate::error::DashboardError;

/// Resolution of the model + provider that a chat request should use.
pub struct ResolvedDispatch {
    pub dispatch: Arc<dyn LlmDispatch>,
    pub model: String,
    pub provider_name: String,
}

/// Resolve a `(provider, model)` selection from a chat request body.
///
/// `provider`: explicit provider name from the request, or `None` to use
///             the `[intern]`-referenced default.
/// `model`:    explicit model from the request, or `None` to fall back
///             to the model declared in `[intern]` for the default
///             provider (otherwise the caller's `default_model`).
pub async fn resolve(
    provider: Option<&str>,
    model: Option<&str>,
    default_model: &str,
) -> Result<ResolvedDispatch, DashboardError> {
    let path = config_path();
    let cfg = load_cfg(&path).await?;

    let intern_provider_name = intern_default_name(&cfg);

    let provider_name = provider
        .map(str::to_string)
        .unwrap_or_else(|| intern_provider_name.clone());

    let entry = cfg
        .providers
        .iter()
        .find(|p| p.name == provider_name)
        .ok_or_else(|| {
            DashboardError::NotFound(format!(
                "provider `{provider_name}` is not configured — add it in Settings → Providers"
            ))
        })?;

    let api_key = if entry.api_key_env.is_empty() {
        String::new()
    } else {
        std::env::var(&entry.api_key_env).map_err(|_| DashboardError::Validation {
            field: "provider".into(),
            msg: format!(
                "no API key for provider `{}` (env var {} is unset). \
                 Paste a key in Settings → Providers or export {} in your shell.",
                entry.name, entry.api_key_env, entry.api_key_env
            ),
        })?
    };

    if api_key.is_empty() && entry.kind != ProviderKind::LocalCandle {
        return Err(DashboardError::Validation {
            field: "provider".into(),
            msg: format!(
                "provider `{}` has no API key set. Paste one in Settings → Providers.",
                entry.name
            ),
        });
    }

    let model = model
        .map(str::to_string)
        .or_else(|| {
            if provider_name == intern_provider_name {
                Some(cfg.intern.model.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| default_model.to_string());

    let dispatch: Arc<dyn LlmDispatch> = match entry.kind {
        ProviderKind::Anthropic => Arc::new(AnthropicDispatch::new(api_key)),
        ProviderKind::OpenaiCompat => {
            Arc::new(OpenaiCompatDispatch::new(entry.base_url.clone(), api_key))
        }
        ProviderKind::LocalCandle => {
            return Err(DashboardError::Validation {
                field: "provider".into(),
                msg:
                    "local-candle providers are not yet wired into the chat surface — use anthropic or openai-compat"
                        .into(),
            });
        }
    };

    tracing::info!(
        target: "xvision::dashboard::chat",
        provider = %entry.name,
        kind = ?entry.kind,
        base_url = %entry.base_url,
        model = %model,
        "resolved chat dispatch"
    );

    Ok(ResolvedDispatch {
        dispatch,
        model,
        provider_name: entry.name.clone(),
    })
}

/// Reuse the same `XVN_CONFIG_PATH` / `<cwd>/config/default.toml`
/// resolution that the providers CRUD route uses.
fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("XVN_CONFIG_PATH") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config/default.toml")
}

async fn load_cfg(path: &Path) -> Result<RuntimeConfig, DashboardError> {
    let p = path.to_path_buf();
    tokio::task::spawn_blocking(move || xvision_core::config::load_runtime(&p))
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("spawn_blocking: {e}")))?
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("load config: {e}")))
}

/// Name of the provider currently referenced by `[intern]`. If no row
/// declares the intern's triple, the auto-derived `_default_intern`
/// synthetic name is returned (which the resolver then looks up just
/// like any other entry).
fn intern_default_name(cfg: &RuntimeConfig) -> String {
    let kind: ProviderKind = cfg.intern.provider.into();
    cfg.providers
        .iter()
        .find(|p: &&ProviderEntry| {
            p.matches_triple(kind, &cfg.intern.base_url, &cfg.intern.api_key_env)
        })
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "_default_intern".to_string())
}
