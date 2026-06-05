//! `/api/settings/memory` — Cortex memory enablement surface.
//!
//! A minimal file-backed picker (mirroring `settings::observability`) that
//! persists which embedder source the memory layer should prefer and
//! whether Chat / Optimizer surfaces record+recall by default.
//!
//! Operator decisions (already approved):
//!   1. Memory works WITHOUT an external provider — the embedder resolver's
//!      final fallback is the offline `Local` embedder (lexical quality), so
//!      a real provider is PREFERRED but not required.
//!   2. Memory defaults ON for Chat + Optimizer (NOT strategy-agent eval,
//!      which stays per-slot opt-in to keep backtests reproducible).
//!   3. This config is the backing store the React settings card talks to.
//!
//! Persistence lives at `$XVN_HOME/config/memory.toml`. A missing/empty
//! file → all defaults (`Auto` embedder + chat/optimizer enabled). Parse
//! errors degrade to defaults with a warn — never panic.
//!
//! Env overrides still win over this config for all three knobs (see the
//! resolver in `crate::agent::embedder_choice` and the dashboard
//! `AppState` accessors) so operators can force behavior.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::api::{
    audit::{self, Outcome},
    ApiContext, ApiResult,
};

/// Which embedder source the memory layer should prefer. Serialized as a
/// flat string in both TOML and JSON so it round-trips cleanly and the
/// React card can send/receive a single string field:
///
///   - `"off"`     → no embedder; recall/record no-op (only way to fully
///                   disable, env `XVN_MEMORY_EMBEDDER=off` does the same).
///   - `"local"`   → force the offline deterministic `LocalEmbedder`.
///   - `"auto"`    → (DEFAULT) prefer a real OpenAI provider/key when
///                   available, else fall back to `Local`.
///   - `"custom"`  → a no-auth OpenAI-compatible `/v1` endpoint typed
///                   directly in the card (base URL in `embedder_base_url`),
///                   for local servers (Ollama, llama.cpp, LM Studio, vLLM).
///   - `<name>`    → a named provider to use as the embeddings backend.
///
/// `off` / `local` / `auto` / `custom` are reserved words; anything else is
/// a provider name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryEmbedderSource {
    Off,
    Local,
    Auto,
    /// No-auth custom OpenAI-compatible endpoint (base URL in
    /// `MemoryConfig.embedder_base_url`).
    Custom,
    Provider(String),
}

impl Default for MemoryEmbedderSource {
    fn default() -> Self {
        MemoryEmbedderSource::Auto
    }
}

impl MemoryEmbedderSource {
    /// The flat config string. This is exactly the value handed to
    /// `EmbedderEnv::config_embedder` so the resolver and the persisted
    /// config agree on the vocabulary.
    pub fn as_config_string(&self) -> String {
        match self {
            MemoryEmbedderSource::Off => "off".to_string(),
            MemoryEmbedderSource::Local => "local".to_string(),
            MemoryEmbedderSource::Auto => "auto".to_string(),
            MemoryEmbedderSource::Custom => "custom".to_string(),
            MemoryEmbedderSource::Provider(name) => name.clone(),
        }
    }

    /// Parse a flat string. Reserved words map to their variant; anything
    /// else (non-empty, trimmed) is a provider name. Empty/whitespace →
    /// `Auto` (the default).
    pub fn parse(s: &str) -> Self {
        let t = s.trim();
        if t.is_empty() {
            return MemoryEmbedderSource::Auto;
        }
        if t.eq_ignore_ascii_case("off") {
            MemoryEmbedderSource::Off
        } else if t.eq_ignore_ascii_case("local") {
            MemoryEmbedderSource::Local
        } else if t.eq_ignore_ascii_case("auto") {
            MemoryEmbedderSource::Auto
        } else if t.eq_ignore_ascii_case("custom") {
            MemoryEmbedderSource::Custom
        } else {
            MemoryEmbedderSource::Provider(t.to_string())
        }
    }
}

impl Serialize for MemoryEmbedderSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.as_config_string())
    }
}

impl<'de> Deserialize<'de> for MemoryEmbedderSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(MemoryEmbedderSource::parse(&s))
    }
}

fn default_true() -> bool {
    true
}

/// Persisted memory enablement config. Missing fields default such that an
/// empty/missing file yields `Auto` + both surfaces enabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Preferred embedder source. `#[serde(default)]` → `Auto`.
    #[serde(default)]
    pub embedder: MemoryEmbedderSource,
    /// Whether the chat rail records+recalls by default. Default ON.
    #[serde(default = "default_true")]
    pub chat_enabled: bool,
    /// Whether the optimizer records+recalls by default. Default ON.
    #[serde(default = "default_true")]
    pub optimizer_enabled: bool,
    /// Embedding model id to request from the embeddings provider (e.g.
    /// `nomic-embed-text`, `qwen3-embedding`, `text-embedding-3-small`).
    /// `None` ≡ "use the resolver default" ([`crate::agent::embedder_choice::DEFAULT_EMBEDDER_MODEL`]).
    /// Ignored by the offline `Local` / `Off` sources (they have no model
    /// concept). The env override `XVN_MEMORY_EMBEDDER_MODEL` still wins.
    #[serde(default)]
    pub embedder_model: Option<String>,
    /// Base URL for the `"custom"` embedder source — a no-auth
    /// OpenAI-compatible `/v1` endpoint typed directly in the card (e.g.
    /// `http://localhost:11434/v1` for Ollama). `None` when the source isn't
    /// `custom`. NO API key is stored here: `memory.toml` is not a 0600
    /// secrets file, so the custom path is no-auth only. Authenticated
    /// endpoints use a registered provider (Providers tab) instead.
    #[serde(default)]
    pub embedder_base_url: Option<String>,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        MemoryConfig {
            embedder: MemoryEmbedderSource::Auto,
            chat_enabled: true,
            optimizer_enabled: true,
            embedder_model: None,
            embedder_base_url: None,
        }
    }
}

impl MemoryConfig {
    /// Load from disk. Missing / empty file → defaults. Parse errors →
    /// defaults + warn (best-effort; never panics or aborts startup).
    pub fn load_from_file(path: &Path) -> MemoryConfig {
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return MemoryConfig::default(),
        };
        if raw.trim().is_empty() {
            return MemoryConfig::default();
        }
        match toml::from_str::<MemoryConfig>(&raw) {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %path.display(),
                    "memory.toml parse error; using defaults"
                );
                MemoryConfig::default()
            }
        }
    }

    /// Write to disk, creating the parent dir. Mirrors observability's
    /// `write_config`.
    pub fn write_to_file(path: &Path, cfg: &MemoryConfig) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = toml::to_string_pretty(cfg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, body)
    }
}

/// Free function form so non-ApiContext callers (CLI, dashboard AppState
/// bootstrap) can load the config without constructing a context.
pub fn load_from_file(path: &Path) -> MemoryConfig {
    MemoryConfig::load_from_file(path)
}

/// Report DTO returned by both `get` and `set`. `embedder` is the flat
/// config string (`off`/`local`/`auto`/<provider>).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryReport {
    pub embedder: String,
    pub chat_enabled: bool,
    pub optimizer_enabled: bool,
    /// The persisted embedding model id, or `null` when none is set (the
    /// resolver default applies). The card renders this as the embedding-
    /// model picker value; empty/clear sends `""` which stores `None`.
    pub embedder_model: Option<String>,
    /// Base URL for the `"custom"` embedder source (no-auth OpenAI-compatible
    /// `/v1` endpoint), or `null` when none is set. The card renders this in
    /// the custom base-URL input, shown only when the source is `custom`.
    pub embedder_base_url: Option<String>,
    /// True when the persisted config file exists. False → defaults are in
    /// force; the UI renders "Default" vs "Custom".
    pub persisted: bool,
}

/// Partial-update request. Only provided fields are changed; omitted fields
/// keep their persisted (or default) value.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateMemoryRequest {
    /// New embedder source as a flat string (`off`/`local`/`auto`/<provider>).
    #[serde(default)]
    pub embedder: Option<String>,
    #[serde(default)]
    pub chat_enabled: Option<bool>,
    #[serde(default)]
    pub optimizer_enabled: Option<bool>,
    /// New embedding model id. An empty/whitespace string clears it back to
    /// the resolver default (stored as `None`); `None` here leaves the
    /// persisted value untouched (partial update).
    #[serde(default)]
    pub embedder_model: Option<String>,
    /// New base URL for the `"custom"` embedder source. An empty/whitespace
    /// string clears it (stored as `None`); `None` here leaves the persisted
    /// value untouched (partial update). Used verbatim (trimmed) — never
    /// rewritten, so the operator-typed `/v1` survives.
    #[serde(default)]
    pub embedder_base_url: Option<String>,
}

fn config_path(ctx: &ApiContext) -> PathBuf {
    ctx.xvn_home.join("config").join("memory.toml")
}

fn report_from_cfg(cfg: &MemoryConfig, persisted: bool) -> MemoryReport {
    MemoryReport {
        embedder: cfg.embedder.as_config_string(),
        chat_enabled: cfg.chat_enabled,
        optimizer_enabled: cfg.optimizer_enabled,
        embedder_model: cfg.embedder_model.clone(),
        embedder_base_url: cfg.embedder_base_url.clone(),
        persisted,
    }
}

/// Read the effective memory config. Missing file → defaults (`Auto` +
/// chat/optimizer enabled).
pub async fn get(ctx: &ApiContext) -> ApiResult<MemoryReport> {
    let started = Instant::now();
    let path = config_path(ctx);
    let persisted = path.exists();
    let cfg = MemoryConfig::load_from_file(&path);
    let report = report_from_cfg(&cfg, persisted);

    let _ = audit::record(
        ctx,
        "settings",
        "memory.get",
        None,
        None,
        Outcome::Ok,
        started.elapsed().as_millis() as i64,
    )
    .await;

    Ok(report)
}

/// Partial update — only the provided fields are changed. Seeds from disk
/// (or defaults) so omitted fields are preserved.
pub async fn set(ctx: &ApiContext, req: UpdateMemoryRequest) -> ApiResult<MemoryReport> {
    let started = Instant::now();
    let path = config_path(ctx);

    let mut cfg = MemoryConfig::load_from_file(&path);
    if let Some(e) = req.embedder.as_deref() {
        cfg.embedder = MemoryEmbedderSource::parse(e);
    }
    if let Some(c) = req.chat_enabled {
        cfg.chat_enabled = c;
    }
    if let Some(o) = req.optimizer_enabled {
        cfg.optimizer_enabled = o;
    }
    if let Some(m) = req.embedder_model.as_deref() {
        // Trim; empty ≡ "clear back to the resolver default" → None.
        let t = m.trim();
        cfg.embedder_model = if t.is_empty() { None } else { Some(t.to_string()) };
    }
    if let Some(b) = req.embedder_base_url.as_deref() {
        // Trim; empty ≡ "clear" → None. Used verbatim otherwise — we do NOT
        // rewrite the operator-typed URL (their `/v1` survives).
        let t = b.trim();
        cfg.embedder_base_url = if t.is_empty() { None } else { Some(t.to_string()) };
    }

    MemoryConfig::write_to_file(&path, &cfg)
        .map_err(|e| crate::api::ApiError::Internal(format!("write memory.toml: {e}")))?;

    let report = report_from_cfg(&cfg, true);

    let _ = audit::record(
        ctx,
        "settings",
        "memory.set",
        Some(&cfg.embedder.as_config_string()),
        None,
        Outcome::Ok,
        started.elapsed().as_millis() as i64,
    )
    .await;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Actor;
    use sqlx::SqlitePool;
    use tempfile::TempDir;

    async fn test_ctx() -> (ApiContext, TempDir) {
        let tmp = TempDir::new().unwrap();
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let ctx = ApiContext::new(pool, Actor::Cli { user: "test".into() }, tmp.path().to_path_buf());
        (ctx, tmp)
    }

    #[test]
    fn embedder_source_round_trips_strings() {
        assert_eq!(MemoryEmbedderSource::parse("off"), MemoryEmbedderSource::Off);
        assert_eq!(MemoryEmbedderSource::parse("local"), MemoryEmbedderSource::Local);
        assert_eq!(MemoryEmbedderSource::parse("auto"), MemoryEmbedderSource::Auto);
        assert_eq!(MemoryEmbedderSource::parse("  "), MemoryEmbedderSource::Auto);
        assert_eq!(
            MemoryEmbedderSource::parse("myproxy"),
            MemoryEmbedderSource::Provider("myproxy".into())
        );
        assert_eq!(MemoryEmbedderSource::Off.as_config_string(), "off");
        assert_eq!(MemoryEmbedderSource::Auto.as_config_string(), "auto");
        assert_eq!(
            MemoryEmbedderSource::Provider("myproxy".into()).as_config_string(),
            "myproxy"
        );
    }

    #[test]
    fn custom_is_a_reserved_keyword_not_a_provider() {
        // `custom` is the custom-endpoint keyword — it must parse to its own
        // variant, NOT be mistaken for a provider name.
        assert_eq!(MemoryEmbedderSource::parse("custom"), MemoryEmbedderSource::Custom);
        assert_eq!(MemoryEmbedderSource::parse("CUSTOM"), MemoryEmbedderSource::Custom);
        assert_eq!(MemoryEmbedderSource::Custom.as_config_string(), "custom");
    }

    #[test]
    fn defaults_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config").join("memory.toml");
        let cfg = MemoryConfig::load_from_file(&path);
        assert_eq!(cfg.embedder, MemoryEmbedderSource::Auto);
        assert!(cfg.chat_enabled);
        assert!(cfg.optimizer_enabled);
        assert_eq!(cfg.embedder_model, None);
    }

    #[test]
    fn parse_error_yields_defaults() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("memory.toml");
        std::fs::write(&path, "this is not valid toml = = =").unwrap();
        let cfg = MemoryConfig::load_from_file(&path);
        assert_eq!(cfg.embedder, MemoryEmbedderSource::Auto);
        assert!(cfg.chat_enabled);
        assert!(cfg.optimizer_enabled);
    }

    #[test]
    fn partial_toml_keeps_other_defaults() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("memory.toml");
        // Only chat_enabled present → embedder=Auto, optimizer_enabled=true.
        std::fs::write(&path, "chat_enabled = false\n").unwrap();
        let cfg = MemoryConfig::load_from_file(&path);
        assert_eq!(cfg.embedder, MemoryEmbedderSource::Auto);
        assert!(!cfg.chat_enabled);
        assert!(cfg.optimizer_enabled);
    }

    #[tokio::test]
    async fn get_returns_defaults_when_no_file() {
        let (ctx, _tmp) = test_ctx().await;
        let report = get(&ctx).await.unwrap();
        assert_eq!(report.embedder, "auto");
        assert!(report.chat_enabled);
        assert!(report.optimizer_enabled);
        assert!(!report.persisted);
    }

    #[tokio::test]
    async fn set_then_get_round_trips() {
        let (ctx, _tmp) = test_ctx().await;
        let report = set(
            &ctx,
            UpdateMemoryRequest {
                embedder: Some("local".into()),
                chat_enabled: Some(false),
                optimizer_enabled: Some(true),
                embedder_model: None,
                embedder_base_url: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(report.embedder, "local");
        assert!(!report.chat_enabled);
        assert!(report.optimizer_enabled);
        assert!(report.persisted);

        let after = get(&ctx).await.unwrap();
        assert_eq!(after.embedder, "local");
        assert!(!after.chat_enabled);
        assert!(after.optimizer_enabled);
        assert!(after.persisted);
    }

    #[tokio::test]
    async fn partial_update_only_changes_provided_fields() {
        let (ctx, _tmp) = test_ctx().await;
        // Seed: embedder=local, chat off, optimizer on.
        set(
            &ctx,
            UpdateMemoryRequest {
                embedder: Some("local".into()),
                chat_enabled: Some(false),
                optimizer_enabled: Some(true),
                embedder_model: None,
                embedder_base_url: None,
            },
        )
        .await
        .unwrap();

        // Update only optimizer_enabled → embedder + chat unchanged.
        let report = set(
            &ctx,
            UpdateMemoryRequest {
                embedder: None,
                chat_enabled: None,
                optimizer_enabled: Some(false),
                embedder_model: None,
                embedder_base_url: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(report.embedder, "local");
        assert!(!report.chat_enabled);
        assert!(!report.optimizer_enabled);
    }

    #[tokio::test]
    async fn embedder_model_round_trips() {
        let (ctx, _tmp) = test_ctx().await;
        // Default: no model set.
        let report = get(&ctx).await.unwrap();
        assert_eq!(report.embedder_model, None);

        // Set a model.
        let report = set(
            &ctx,
            UpdateMemoryRequest {
                embedder: Some("ollama".into()),
                embedder_model: Some("nomic-embed-text".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(report.embedder_model.as_deref(), Some("nomic-embed-text"));

        let after = get(&ctx).await.unwrap();
        assert_eq!(after.embedder_model.as_deref(), Some("nomic-embed-text"));
        assert_eq!(after.embedder, "ollama");
    }

    #[tokio::test]
    async fn empty_embedder_model_clears_to_none() {
        let (ctx, _tmp) = test_ctx().await;
        // Seed a model.
        set(
            &ctx,
            UpdateMemoryRequest {
                embedder_model: Some("qwen3-embedding".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        // Clear via empty string.
        let report = set(
            &ctx,
            UpdateMemoryRequest {
                embedder_model: Some("   ".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(report.embedder_model, None);

        // A None on a later update leaves it untouched (still None here).
        let report = set(
            &ctx,
            UpdateMemoryRequest {
                chat_enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(report.embedder_model, None);
    }

    #[tokio::test]
    async fn embedder_base_url_round_trips() {
        let (ctx, _tmp) = test_ctx().await;
        // Default: no base url set.
        let report = get(&ctx).await.unwrap();
        assert_eq!(report.embedder_base_url, None);

        // Set a custom endpoint.
        let report = set(
            &ctx,
            UpdateMemoryRequest {
                embedder: Some("custom".into()),
                embedder_base_url: Some("http://localhost:11434/v1".into()),
                embedder_model: Some("nomic-embed-text".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(report.embedder, "custom");
        assert_eq!(
            report.embedder_base_url.as_deref(),
            Some("http://localhost:11434/v1")
        );

        let after = get(&ctx).await.unwrap();
        assert_eq!(after.embedder, "custom");
        assert_eq!(
            after.embedder_base_url.as_deref(),
            Some("http://localhost:11434/v1")
        );
        assert_eq!(after.embedder_model.as_deref(), Some("nomic-embed-text"));
    }

    #[tokio::test]
    async fn empty_embedder_base_url_clears_to_none() {
        let (ctx, _tmp) = test_ctx().await;
        set(
            &ctx,
            UpdateMemoryRequest {
                embedder_base_url: Some("http://localhost:11434/v1".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        // Clear via empty/whitespace.
        let report = set(
            &ctx,
            UpdateMemoryRequest {
                embedder_base_url: Some("   ".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(report.embedder_base_url, None);
    }

    #[tokio::test]
    async fn embedder_base_url_partial_update_preserved() {
        let (ctx, _tmp) = test_ctx().await;
        set(
            &ctx,
            UpdateMemoryRequest {
                embedder_base_url: Some("http://localhost:11434/v1".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        // Update an unrelated field; base_url must survive.
        let report = set(
            &ctx,
            UpdateMemoryRequest {
                chat_enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            report.embedder_base_url.as_deref(),
            Some("http://localhost:11434/v1")
        );
    }

    #[tokio::test]
    async fn embedder_model_partial_update_preserved() {
        let (ctx, _tmp) = test_ctx().await;
        set(
            &ctx,
            UpdateMemoryRequest {
                embedder_model: Some("bge-m3".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        // Update an unrelated field; model must survive.
        let report = set(
            &ctx,
            UpdateMemoryRequest {
                optimizer_enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(report.embedder_model.as_deref(), Some("bge-m3"));
    }
}
