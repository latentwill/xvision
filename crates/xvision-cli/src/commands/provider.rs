//! `xvn provider …` — list / show / check / add / remove registered LLM
//! providers. Reads from / writes to `config/default.toml`.
//!
//! `add` and `remove` mutate the file in place via `toml_edit` to preserve
//! comments and formatting (Plan #7 Phase 4 Task 14, deferred). `list` and
//! `show` are read-only.

use clap::{Args, Subcommand};

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
    },
    /// Remove a provider by name. Refused if any slot references it.
    Remove {
        #[arg(long)]
        name: String,
    },
}

pub async fn run(cmd: ProviderCmd) -> anyhow::Result<()> {
    let config_path = std::env::current_dir()?.join("config/default.toml");
    match cmd.action {
        ProviderAction::List => list(&config_path),
        ProviderAction::Show { name } => show(&config_path, &name),
        ProviderAction::Check { name, probe } => check(&config_path, &name, probe).await,
        ProviderAction::Add {
            name,
            kind,
            base_url,
            api_key_env,
        } => add(&config_path, &name, &kind, &base_url, &api_key_env),
        ProviderAction::Remove { name } => remove(&config_path, &name),
    }
}

fn list(config_path: &std::path::Path) -> anyhow::Result<()> {
    let cfg = xvision_core::config::load_runtime(config_path)?;
    println!(
        "{:<18} {:<14} {:<42} {:<22} {}",
        "NAME", "KIND", "BASE_URL", "API_KEY_ENV", "KEY"
    );
    for p in &cfg.providers {
        let key_state = if p.api_key_env.is_empty() {
            "n/a".to_string()
        } else if std::env::var(&p.api_key_env).is_ok() {
            "● set".to_string()
        } else {
            "○ missing".to_string()
        };
        let kind = match p.kind {
            xvision_core::config::ProviderKind::Anthropic => "anthropic",
            xvision_core::config::ProviderKind::OpenaiCompat => "openai-compat",
            xvision_core::config::ProviderKind::LocalCandle => "local-candle",
        };
        let env_display = if p.api_key_env.is_empty() {
            "(none)".to_string()
        } else {
            p.api_key_env.clone()
        };
        let synth_marker = if p.name.starts_with('_') {
            "  (synthetic)"
        } else {
            ""
        };
        println!(
            "{:<18} {:<14} {:<42} {:<22} {}{}",
            p.name, kind, p.base_url, env_display, key_state, synth_marker
        );
    }
    Ok(())
}

fn show(config_path: &std::path::Path, name: &str) -> anyhow::Result<()> {
    let cfg = xvision_core::config::load_runtime(config_path)?;
    let p = cfg
        .providers
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| anyhow::anyhow!("provider `{name}` not found"))?;
    println!("{}", serde_json::to_string_pretty(p)?);
    if !p.api_key_env.is_empty() {
        let state = if std::env::var(&p.api_key_env).is_ok() {
            "set"
        } else {
            "missing"
        };
        println!("(env {} → {state})", p.api_key_env);
    }
    Ok(())
}

async fn check(
    _config_path: &std::path::Path,
    _name: &str,
    _probe: bool,
) -> anyhow::Result<()> {
    anyhow::bail!("`xvn provider check` lands in Plan #7 Phase 4 Task 15")
}

fn add(
    _config_path: &std::path::Path,
    _name: &str,
    _kind: &str,
    _base_url: &str,
    _api_key_env: &str,
) -> anyhow::Result<()> {
    anyhow::bail!("`xvn provider add` lands in Plan #7 Phase 4 Task 14")
}

fn remove(_config_path: &std::path::Path, _name: &str) -> anyhow::Result<()> {
    anyhow::bail!("`xvn provider remove` lands in Plan #7 Phase 4 Task 14")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_returns_err_for_unknown_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.toml");
        std::fs::write(&path, MIN_CONFIG).unwrap();
        let err = show(&path, "nope").unwrap_err();
        assert!(format!("{err:#}").contains("not found"));
    }

    #[test]
    fn show_prints_known_provider() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.toml");
        std::fs::write(&path, MIN_CONFIG).unwrap();
        // We just want to assert no error; stdout capture isn't easy in a unit
        // test without extra plumbing.
        show(&path, "anthropic").unwrap();
    }

    #[test]
    fn list_succeeds_against_min_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.toml");
        std::fs::write(&path, MIN_CONFIG).unwrap();
        list(&path).unwrap();
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
