//! `xvn provider …` — list / show / check / add / remove registered LLM
//! providers. Reads from / writes to `config/default.toml`.
//!
//! `add` and `remove` mutate the file in place via `toml_edit` to preserve
//! comments and formatting. `list` and `show` are read-only.

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

async fn check(config_path: &std::path::Path, name: &str, probe: bool) -> anyhow::Result<()> {
    let cfg = xvision_core::config::load_runtime(config_path)?;
    let p = cfg
        .providers
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| anyhow::anyhow!("provider `{name}` not found"))?;

    if !p.api_key_env.is_empty() && std::env::var(&p.api_key_env).is_err() {
        println!("○ env {} not set", p.api_key_env);
    } else if !p.api_key_env.is_empty() {
        println!("● env {} set", p.api_key_env);
    }

    let url = url_parse_minimal(&p.base_url)?;
    let stream = tokio::net::TcpStream::connect((url.host.as_str(), url.port)).await;
    match stream {
        Ok(_) => println!("● tcp {}:{} reachable", url.host, url.port),
        Err(e) => println!("○ tcp {}:{} {e}", url.host, url.port),
    }

    if probe {
        let client = reqwest::Client::new();
        let probe_url = if p.base_url.ends_with('/') {
            format!("{}models", p.base_url)
        } else {
            format!("{}/models", p.base_url)
        };
        let mut req = client.get(&probe_url);
        if !p.api_key_env.is_empty() {
            if let Ok(key) = std::env::var(&p.api_key_env) {
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
fn url_parse_minimal(s: &str) -> anyhow::Result<MinimalUrl> {
    let (scheme, rest) = s
        .split_once("://")
        .ok_or_else(|| anyhow::anyhow!("base_url missing scheme: {s}"))?;
    let host_port_path = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match host_port_path.split_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>().map_err(|e| anyhow::anyhow!("port parse: {e}"))?,
        ),
        None => (
            host_port_path.to_string(),
            if scheme == "https" { 443 } else { 80 },
        ),
    };
    Ok(MinimalUrl { host, port })
}

fn add(
    config_path: &std::path::Path,
    name: &str,
    kind: &str,
    base_url: &str,
    api_key_env: &str,
) -> anyhow::Result<()> {
    use toml_edit::{value, ArrayOfTables, DocumentMut, Table};

    match kind {
        "anthropic" | "openai-compat" | "local-candle" => {}
        other => {
            anyhow::bail!("invalid kind `{other}`; must be one of: anthropic | openai-compat | local-candle")
        }
    }
    if name.starts_with('_') {
        anyhow::bail!("provider names starting with '_' are reserved");
    }

    let raw = std::fs::read_to_string(config_path)?;
    let mut doc: DocumentMut = raw.parse()?;
    let providers = match doc
        .entry("providers")
        .or_insert_with(|| toml_edit::Item::ArrayOfTables(ArrayOfTables::new()))
    {
        toml_edit::Item::ArrayOfTables(arr) => arr,
        _ => anyhow::bail!("[[providers]] is not an array of tables"),
    };
    if providers
        .iter()
        .any(|t| t.get("name").and_then(|v| v.as_str()) == Some(name))
    {
        anyhow::bail!("provider `{name}` already exists");
    }
    let mut row = Table::new();
    row.insert("name", value(name));
    row.insert("kind", value(kind));
    row.insert("base_url", value(base_url));
    row.insert("api_key_env", value(api_key_env));
    providers.push(row);

    std::fs::write(config_path, doc.to_string())?;
    xvision_core::config::load_runtime(config_path)?;
    Ok(())
}

fn remove(config_path: &std::path::Path, name: &str) -> anyhow::Result<()> {
    use toml_edit::DocumentMut;

    let cfg = xvision_core::config::load_runtime(config_path)?;
    let intern_kind: xvision_core::config::ProviderKind = cfg.intern.provider.into();
    if let Some(p) = cfg.providers.iter().find(|p| p.name == name) {
        if p.matches_triple(intern_kind, &cfg.intern.base_url, &cfg.intern.api_key_env) {
            anyhow::bail!(
                "cannot remove provider `{name}`: referenced by [intern] (workspace default Intern slot). \
                 Edit [intern] to point at a different provider first."
            );
        }
    } else {
        anyhow::bail!("provider `{name}` not found");
    }

    let raw = std::fs::read_to_string(config_path)?;
    let mut doc: DocumentMut = raw.parse()?;
    if let Some(toml_edit::Item::ArrayOfTables(arr)) = doc.get_mut("providers") {
        let before = arr.len();
        arr.retain(|t| t.get("name").and_then(|v| v.as_str()) != Some(name));
        if arr.len() == before {
            anyhow::bail!("provider `{name}` not found in TOML (race / synthetic row)");
        }
    } else {
        anyhow::bail!("no [[providers]] block in {}", config_path.display());
    }
    std::fs::write(config_path, doc.to_string())?;
    xvision_core::config::load_runtime(config_path)?;
    Ok(())
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

    #[test]
    fn add_appends_provider_row() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.toml");
        std::fs::write(&path, MIN_CONFIG).unwrap();
        add(
            &path,
            "openai",
            "openai-compat",
            "https://api.openai.com/v1",
            "OPENAI_API_KEY",
        )
        .unwrap();
        let cfg = xvision_core::config::load_runtime(&path).unwrap();
        assert!(cfg.providers.iter().any(|p| p.name == "openai"));
    }

    #[test]
    fn add_rejects_duplicate_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.toml");
        std::fs::write(&path, MIN_CONFIG).unwrap();
        let err = add(&path, "anthropic", "anthropic", "https://x", "K").unwrap_err();
        assert!(format!("{err:#}").contains("already exists"));
    }

    #[test]
    fn add_rejects_invalid_kind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.toml");
        std::fs::write(&path, MIN_CONFIG).unwrap();
        let err = add(&path, "x", "BOGUS", "https://x", "K").unwrap_err();
        assert!(format!("{err:#}").contains("kind"));
    }

    #[test]
    fn remove_drops_provider_row() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.toml");
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
        std::fs::write(&path, src).unwrap();
        remove(&path, "ephemeral").unwrap();
        let cfg = xvision_core::config::load_runtime(&path).unwrap();
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

    #[test]
    fn remove_refuses_when_intern_block_references_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.toml");
        std::fs::write(&path, MIN_CONFIG).unwrap();
        let err = remove(&path, "anthropic").unwrap_err();
        assert!(format!("{err:#}").contains("referenced by [intern]"));
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
