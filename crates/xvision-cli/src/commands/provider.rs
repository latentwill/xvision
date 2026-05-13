//! `xvn provider …` — list / show / check / add / remove registered LLM
//! providers.
//!
//! Business logic lives in `xvision_engine::api::settings::providers::*`
//! (single source of truth, also dispatched by the dashboard's
//! `/api/settings/providers` routes). This module is a thin CLI shim —
//! it parses flags, opens an `ApiContext`, and formats the results for
//! a TTY.
//!
//! `check` is the one exception: it runs a live TCP / HTTP probe with
//! arbitrary network latency, which is intentionally out of scope for
//! the engine API in v1.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};

use xvision_engine::api::settings::providers::{
    self, AddProviderRequest, ProviderRow,
};
use xvision_engine::api::{Actor, ApiContext};

#[derive(Args, Debug)]
pub struct ProviderCmd {
    #[command(subcommand)]
    action: ProviderAction,
}

#[derive(Subcommand, Debug)]
enum ProviderAction {
    /// List all registered providers.
    List,
    /// Show one provider in full.
    Show {
        #[arg(long)]
        name: String,
    },
    /// Probe a provider for reachability.
    Check {
        #[arg(long)]
        name: String,
        /// Send a real /models request (costs nothing on most providers but
        /// burns a request quota slot). Default is a TCP-connect smoke.
        #[arg(long, default_value_t = false)]
        probe: bool,
    },
    /// Register a new provider in config/default.toml.
    Add {
        #[arg(long)]
        name: String,
        /// `anthropic` | `openai-compat` | `local-candle`.
        #[arg(long)]
        kind: String,
        #[arg(long)]
        base_url: String,
        /// Env var holding the API key (empty for no-auth endpoints).
        #[arg(long, default_value = "")]
        api_key_env: String,
        /// Inline API key. When omitted, the engine requires the env var
        /// above to already be exported in the shell.
        #[arg(long)]
        api_key: Option<String>,
    },
    /// Remove a provider by name. Refused if any slot references it.
    Remove {
        #[arg(long)]
        name: String,
    },
}

pub async fn run(cmd: ProviderCmd) -> Result<()> {
    let xvn_home = resolve_xvn_home()?;
    let config_path = runtime_config_path(&xvn_home);
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    let ctx = ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("open ApiContext: {e}"))?;

    match cmd.action {
        ProviderAction::List => list(&ctx, &config_path).await,
        ProviderAction::Show { name } => show(&ctx, &config_path, &name).await,
        ProviderAction::Check { name, probe } => check(&ctx, &config_path, &name, probe).await,
        ProviderAction::Add {
            name,
            kind,
            base_url,
            api_key_env,
            api_key,
        } => add(&ctx, &config_path, name, kind, base_url, api_key_env, api_key).await,
        ProviderAction::Remove { name } => remove(&ctx, &config_path, &name).await,
    }
}

fn runtime_config_path(xvn_home: &std::path::Path) -> PathBuf {
    if let Ok(p) = std::env::var("XVN_CONFIG_PATH") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    xvn_home.join("config").join("default.toml")
}

fn resolve_xvn_home() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("XVN_HOME") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir().context("HOME not set; set XVN_HOME")?;
    Ok(home.join(".xvn"))
}

async fn list(ctx: &ApiContext, config_path: &std::path::Path) -> Result<()> {
    let report = providers::list(ctx, config_path)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!(
        "{:<18} {:<14} {:<42} {:<22} {}",
        "NAME", "KIND", "BASE_URL", "API_KEY_ENV", "KEY"
    );
    for p in &report.providers {
        let key_state = if p.api_key_env.is_empty() {
            "n/a".to_string()
        } else if p.api_key_set {
            "● set".to_string()
        } else {
            "○ missing".to_string()
        };
        let env_display = if p.api_key_env.is_empty() {
            "(none)".to_string()
        } else {
            p.api_key_env.clone()
        };
        let synth_marker = if p.synthetic { "  (synthetic)" } else { "" };
        println!(
            "{:<18} {:<14} {:<42} {:<22} {}{}",
            p.name, p.kind, p.base_url, env_display, key_state, synth_marker
        );
    }
    Ok(())
}

async fn show(ctx: &ApiContext, config_path: &std::path::Path, name: &str) -> Result<()> {
    let row = providers::show(ctx, config_path, name)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("{}", serde_json::to_string_pretty(&row)?);
    if !row.api_key_env.is_empty() {
        let state = if row.api_key_set { "set" } else { "missing" };
        println!("(env {} → {state})", row.api_key_env);
    }
    Ok(())
}

async fn check(
    ctx: &ApiContext,
    config_path: &std::path::Path,
    name: &str,
    probe: bool,
) -> Result<()> {
    // CLI-only — the engine API doesn't run live network probes in v1.
    let row: ProviderRow = providers::show(ctx, config_path, name)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if !row.api_key_env.is_empty() && !row.api_key_set {
        println!("○ env {} not set", row.api_key_env);
    } else if !row.api_key_env.is_empty() {
        println!("● env {} set", row.api_key_env);
    }

    let url = url_parse_minimal(&row.base_url)?;
    let stream = tokio::net::TcpStream::connect((url.host.as_str(), url.port)).await;
    match stream {
        Ok(_) => println!("● tcp {}:{} reachable", url.host, url.port),
        Err(e) => println!("○ tcp {}:{} {e}", url.host, url.port),
    }

    if probe {
        let client = reqwest::Client::new();
        let probe_url = if row.base_url.ends_with('/') {
            format!("{}models", row.base_url)
        } else {
            format!("{}/models", row.base_url)
        };
        let mut req = client.get(&probe_url);
        if !row.api_key_env.is_empty() {
            if let Ok(key) = std::env::var(&row.api_key_env) {
                req = req.header("Authorization", format!("Bearer {key}"));
            }
        }
        match req.send().await {
            Ok(resp) => println!("● GET {probe_url} → {}", resp.status()),
            Err(e) => println!("○ GET {probe_url} → {e}"),
        }
    }
    Ok(())
}

struct MinimalUrl {
    host: String,
    port: u16,
}

/// Tiny URL parser for `https://host[:port]/...` and `http://host[:port]/...`.
/// Avoids pulling in the `url` crate just for `provider check`.
fn url_parse_minimal(s: &str) -> Result<MinimalUrl> {
    let (scheme, rest) = s
        .split_once("://")
        .ok_or_else(|| anyhow::anyhow!("base_url missing scheme: {s}"))?;
    let host_port_path = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match host_port_path.split_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>()
                .map_err(|e| anyhow::anyhow!("port parse: {e}"))?,
        ),
        None => (
            host_port_path.to_string(),
            if scheme == "https" { 443 } else { 80 },
        ),
    };
    Ok(MinimalUrl { host, port })
}

async fn add(
    ctx: &ApiContext,
    config_path: &std::path::Path,
    name: String,
    kind: String,
    base_url: String,
    api_key_env: String,
    api_key: Option<String>,
) -> Result<()> {
    providers::add(
        ctx,
        config_path,
        AddProviderRequest {
            name,
            kind,
            base_url,
            api_key_env,
            api_key,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

async fn remove(ctx: &ApiContext, config_path: &std::path::Path, name: &str) -> Result<()> {
    providers::remove(ctx, config_path, name)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_engine::api::Actor;

    async fn test_ctx(dir: &tempfile::TempDir) -> ApiContext {
        ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "test".into(),
            },
        )
        .await
        .unwrap()
    }

    fn write_min_config(dir: &tempfile::TempDir) -> std::path::PathBuf {
        let p = dir.path().join("config").join("default.toml");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, MIN_CONFIG).unwrap();
        p
    }

    #[tokio::test]
    async fn list_succeeds_against_min_config() {
        let dir = tempfile::tempdir().unwrap();
        let config = write_min_config(&dir);
        let ctx = test_ctx(&dir).await;
        list(&ctx, &config).await.unwrap();
    }

    #[tokio::test]
    async fn show_returns_err_for_unknown_name() {
        let dir = tempfile::tempdir().unwrap();
        let config = write_min_config(&dir);
        let ctx = test_ctx(&dir).await;
        let err = show(&ctx, &config, "nope").await.unwrap_err();
        assert!(format!("{err:#}").contains("not found"));
    }

    #[tokio::test]
    async fn add_appends_provider_row() {
        let dir = tempfile::tempdir().unwrap();
        let config = write_min_config(&dir);
        let ctx = test_ctx(&dir).await;
        add(
            &ctx,
            &config,
            "openai".into(),
            "openai-compat".into(),
            "https://api.openai.com/v1".into(),
            "OPENAI_API_KEY".into(),
            Some("sk-test".into()),
        )
        .await
        .unwrap();
        let cfg = xvision_core::config::load_runtime(&config).unwrap();
        assert!(cfg.providers.iter().any(|p| p.name == "openai"));
    }

    #[tokio::test]
    async fn remove_drops_provider_row() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("default.toml");
        let mut src = MIN_CONFIG.to_string();
        src.push_str(
            r#"
[[providers]]
name = "ephemeral"
kind = "openai-compat"
base_url = "https://x"
api_key_env = "K"
"#,
        );
        std::fs::write(&config, src).unwrap();
        let ctx = test_ctx(&dir).await;
        remove(&ctx, &config, "ephemeral").await.unwrap();
        let cfg = xvision_core::config::load_runtime(&config).unwrap();
        assert!(!cfg.providers.iter().any(|p| p.name == "ephemeral"));
    }

    #[test]
    fn url_parse_handles_https_default_port() {
        let u = url_parse_minimal("https://api.openai.com/v1").unwrap();
        assert_eq!(u.host, "api.openai.com");
        assert_eq!(u.port, 443);
    }

    #[test]
    fn url_parse_handles_explicit_port() {
        let u = url_parse_minimal("http://localhost:11434/v1").unwrap();
        assert_eq!(u.host, "localhost");
        assert_eq!(u.port, 11434);
    }

    #[test]
    fn url_parse_rejects_no_scheme() {
        assert!(url_parse_minimal("api.openai.com/v1").is_err());
    }

    const MIN_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "anthropic"
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"

[intern]
provider = "anthropic"
base_url = "https://api.anthropic.com"
model = "x"
api_key_env = "ANTHROPIC_API_KEY"
temperature = 0.0
max_tokens = 1024

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#;
}
