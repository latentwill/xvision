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

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;

use xvision_engine::api::settings::providers::{self, AddProviderRequest, EffectiveProvider, ProviderRow};
use xvision_engine::api::settings::providers_catalog;
use xvision_engine::api::{Actor, ApiContext};

/// Output format for list subcommands. Used by `provider list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum ListFormat {
    /// Human-readable table columns to stdout (default).
    Table,
    /// Pretty-printed JSON array to stdout.
    Json,
    /// Compact single-line JSON array to stdout. Suitable for piping.
    JsonCompact,
}

#[derive(Args, Debug)]
pub struct ProviderCmd {
    #[command(subcommand)]
    action: ProviderAction,
}

#[derive(Subcommand, Debug)]
enum ProviderAction {
    /// List all registered providers. Default output preserves the legacy
    /// human-readable table; pass `--effective` for the canonical
    /// launchability rollup (provider/has_key/launchable/per-model
    /// enablement) shared with the dashboard and `xvn doctor`.
    #[command(visible_alias = "ls")]
    List {
        /// Emit the canonical `EffectiveProvider` rows backed by the
        /// `providers::effective_providers` helper. Same shape served by
        /// `GET /api/settings/providers` (with rollup) and surfaced in
        /// the `xvn doctor` report.
        #[arg(long, default_value_t = false)]
        effective: bool,
        /// Render JSON (stdout). Implies `--effective`. Alias for
        /// `--format json-compact`. Explicit `--format` wins when both
        /// are passed.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Output format: table (default), json, json-compact.
        /// When set, takes precedence over `--json`. `--format json`
        /// or `--format json-compact` both imply `--effective` behavior.
        #[arg(long, value_enum)]
        format: Option<ListFormat>,
    },
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
        /// `anthropic` | `openai-compat` | `local-candle` | `ollama` | `llama-cpp` | `vllm`.
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
        /// Validate inputs and print what would be added without writing
        /// anything to disk. Exits 0.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Remove a provider by name. Refused if any slot references it.
    Remove {
        #[arg(long)]
        name: String,
        /// Validate that the provider exists and print what would be
        /// removed without writing anything. Exits 0.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Refresh the catalog for one provider (or all if `--name` omitted)
    /// by hitting its `/v1/models` endpoint and writing to disk.
    RefreshModels {
        #[arg(long)]
        name: Option<String>,
        /// Print what would be refreshed without making any network call
        /// or writing to disk. Exits 0.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Show the cached model catalog for a provider.
    ///
    /// By default only models in the provider's `enabled_models` allowlist
    /// are shown. When `enabled_models` is empty (nothing configured yet),
    /// all cached models are shown as a fallback.
    ///
    /// Does NOT hit the network — prints a refresh hint when no cached
    /// catalog exists yet.
    Models {
        #[arg(long)]
        name: String,
        /// Show all cached models regardless of the `enabled_models`
        /// allowlist. Useful for discovering models to enable.
        #[arg(long, default_value_t = false)]
        all: bool,
        /// Add a model id to this provider's `enabled_models` allowlist
        /// and persist it to config/default.toml.
        #[arg(long)]
        enable: Option<String>,
        /// Remove a model id from this provider's `enabled_models`
        /// allowlist and persist the change to config/default.toml.
        #[arg(long)]
        disable: Option<String>,
    },
}

/// Source persisted provider secrets from `$XVN_HOME/secrets/providers.toml`
/// into the process env BEFORE any provider/key lookup (QA U8).
///
/// The dashboard already calls
/// `xvision_engine::api::settings::providers::load_providers_secrets_into_env`
/// at daemon startup (`state.rs`); the CLI did not, so an Ollama (or any)
/// provider whose key lives only in the secrets file — never exported to
/// env — failed with an opaque "no API key" error and forced operators to
/// prefix every command with `XVN_PROVIDER_OLLAMA_LOCAL_KEY=… xvn …`.
///
/// This helper mirrors the dashboard path: it is **idempotent** (the
/// engine helper skips any var already set, so an explicit env export
/// still wins) and **best-effort** (a missing/unreadable secrets file is
/// not fatal — the command proceeds and any genuinely-missing key surfaces
/// later with the now-actionable, env-var-naming error).
///
/// MUST be called once, early, from the CLI bootstrap (`Cli::run`) BEFORE
/// any `ApiContext::open` / env key lookup. Exposed here (rather than
/// inlined into `lib.rs`) so the bootstrap owner wires a single named call.
pub async fn load_secrets_into_env_best_effort(xvn_home: &std::path::Path) {
    // Best-effort: any error (missing/unreadable providers.toml) is non-fatal —
    // a genuinely-missing key still surfaces downstream with an actionable,
    // env-var-naming error. We deliberately swallow the result here.
    let _ = providers::load_providers_secrets_into_env(xvn_home).await;
}

pub async fn run(cmd: ProviderCmd) -> Result<()> {
    let xvn_home = resolve_xvn_home()?;
    let config_path = runtime_config_path(&xvn_home);

    // Read-only `--effective` / `--json` / `--format` paths skip opening
    // the ApiContext so stdout stays JSON-clean. `ApiContext::open`
    // side-effects tracing on stdout (the "V2D: failed to open
    // memory store" WARN) — fixing the tracing-init upstream is the
    // `cli-json-stdout-contract` sibling track's scope.
    if let ProviderAction::List {
        effective,
        json,
        format,
    } = &cmd.action
    {
        // Resolve the effective format: explicit --format wins; then --json
        // (treated as json-compact for back-compat); then table default.
        let resolved = match format {
            Some(f) => *f,
            None if *json => ListFormat::JsonCompact,
            None => ListFormat::Table,
        };
        if *effective || *json || resolved != ListFormat::Table {
            return list_effective(&xvn_home, &config_path, resolved).await;
        }
    }

    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    let ctx = ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("open ApiContext: {e}"))?;

    match cmd.action {
        ProviderAction::List {
            effective: _,
            json: _,
            format: _,
        } => {
            // Legacy human table — falls through here (the `--effective`
            // / `--json` / `--format` branches were handled above without
            // an ApiContext open).
            list_legacy(&ctx, &config_path).await
        }
        ProviderAction::Show { name } => show(&ctx, &config_path, &name).await,
        ProviderAction::Check { name, probe } => check(&ctx, &config_path, &name, probe).await,
        ProviderAction::Add {
            name,
            kind,
            base_url,
            api_key_env,
            api_key,
            dry_run,
        } => {
            add(
                &ctx,
                &config_path,
                name,
                kind,
                base_url,
                api_key_env,
                api_key,
                dry_run,
            )
            .await
        }
        ProviderAction::Remove { name, dry_run } => remove(&ctx, &config_path, &name, dry_run).await,
        ProviderAction::RefreshModels { name, dry_run } => {
            refresh_models(&ctx, &config_path, name.as_deref(), dry_run).await
        }
        ProviderAction::Models {
            name,
            all,
            enable,
            disable,
        } => models(&ctx, &config_path, &name, all, enable, disable).await,
    }
}

fn runtime_config_path(xvn_home: &std::path::Path) -> PathBuf {
    xvision_core::config::runtime_config_path(xvn_home)
}

fn resolve_xvn_home() -> Result<PathBuf> {
    crate::commands::home::resolve_xvn_home_env()
}

/// Canonical "is this provider launchable" view. Path-only — does not
/// open `ApiContext`, so JSON output is uncontaminated by audit-pool
/// migration tracing.
async fn list_effective(
    xvn_home: &std::path::Path,
    config_path: &std::path::Path,
    format: ListFormat,
) -> Result<()> {
    let rows = providers::effective_providers_with_paths(xvn_home, config_path)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    match format {
        ListFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&rows)?);
        }
        ListFormat::JsonCompact => {
            println!("{}", serde_json::to_string(&rows)?);
        }
        ListFormat::Table => {
            print_effective_table(&rows);
        }
    }
    Ok(())
}

async fn list_legacy(ctx: &ApiContext, config_path: &std::path::Path) -> Result<()> {
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

fn print_effective_table(rows: &[EffectiveProvider]) {
    println!(
        "{:<18} {:<14} {:<8} {:<8} {:<7} {:<11} {}",
        "PROVIDER", "KIND", "ENABLED", "KEY", "MODELS", "LAUNCHABLE", "EXPECTED_ENV"
    );
    for r in rows {
        // Name the env var an operator must export to give this provider a
        // key (QA U8). No-auth local kinds report an empty expected env, so
        // render a clear "(none)" rather than a blank gap.
        let expected_env = if r.expected_api_key_env.is_empty() {
            "(none)".to_string()
        } else {
            r.expected_api_key_env.clone()
        };
        println!(
            "{:<18} {:<14} {:<8} {:<8} {:<7} {:<11} {}",
            r.provider,
            r.kind,
            if r.enabled { "yes" } else { "no" },
            if r.has_key { "set" } else { "missing" },
            r.models.len(),
            if r.launchable { "yes" } else { "no" },
            expected_env,
        );
    }
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

async fn check(ctx: &ApiContext, config_path: &std::path::Path, name: &str, probe: bool) -> Result<()> {
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
        let probe_url = provider_catalog_probe_url(&row.kind, &row.base_url);
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

fn provider_catalog_probe_url(kind: &str, base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if kind == "ollama" {
        format!("{trimmed}/api/tags")
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1/models")
    }
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
            p.parse::<u16>().map_err(|e| anyhow::anyhow!("port parse: {e}"))?,
        ),
        None => (
            host_port_path.to_string(),
            if scheme == "https" { 443 } else { 80 },
        ),
    };
    Ok(MinimalUrl { host, port })
}

/// JSON preview shape for `provider add --dry-run`.
#[derive(Debug, Serialize)]
struct DryRunAddPreview<'a> {
    action: &'static str,
    name: &'a str,
    kind: &'a str,
    base_url: &'a str,
    api_key_env: &'a str,
    api_key_provided: bool,
}

async fn add(
    ctx: &ApiContext,
    config_path: &std::path::Path,
    name: String,
    kind: String,
    base_url: String,
    api_key_env: String,
    api_key: Option<String>,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        // Validate + preview only — nothing written.
        let preview = DryRunAddPreview {
            action: "add_provider",
            name: &name,
            kind: &kind,
            base_url: &base_url,
            api_key_env: &api_key_env,
            api_key_provided: api_key.is_some(),
        };
        println!("{}", serde_json::to_string_pretty(&preview)?);
        eprintln!("DRY RUN — would add provider `{name}`");
        return Ok(());
    }
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

async fn remove(ctx: &ApiContext, config_path: &std::path::Path, name: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        // Resolve: verify provider exists (read-only lookup via list).
        let report = providers::list(ctx, config_path)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let found = report.providers.iter().any(|p| p.name == name);
        if !found {
            anyhow::bail!("provider `{name}` not found");
        }
        eprintln!("DRY RUN — would remove provider `{name}`");
        return Ok(());
    }
    providers::remove(ctx, config_path, name)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

async fn refresh_models(
    ctx: &ApiContext,
    config_path: &std::path::Path,
    name: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        match name {
            Some(n) => {
                // Validate provider exists before claiming we'd refresh it.
                let report = providers::list(ctx, config_path)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let found = report.providers.iter().any(|p| p.name == n);
                if !found {
                    anyhow::bail!("provider `{n}` not found");
                }
                eprintln!("DRY RUN — would refresh models for `{n}`");
            }
            None => {
                eprintln!("DRY RUN — would refresh models for all providers");
            }
        }
        return Ok(());
    }
    match name {
        Some(n) => {
            let cat = providers_catalog::refresh(ctx, config_path, n)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!(
                "{:<18} {:>5} models   fetched_at={}   source={}",
                cat.provider,
                cat.models.len(),
                cat.fetched_at.to_rfc3339(),
                cat.source_url
            );
        }
        None => {
            let rows = providers_catalog::refresh_all(ctx, config_path)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!(
                "{:<18} {:<6} {:>6}  {}",
                "PROVIDER", "STATUS", "MODELS", "SOURCE / ERROR"
            );
            for row in rows {
                let status = if row.ok { "ok" } else { "fail" };
                let count = row
                    .model_count
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "—".into());
                let trailing = row.source_url.unwrap_or_else(|| row.error.unwrap_or_default());
                println!("{:<18} {:<6} {:>6}  {}", row.provider, status, count, trailing);
            }
        }
    }
    Ok(())
}

async fn models(
    ctx: &ApiContext,
    config_path: &std::path::Path,
    name: &str,
    show_all: bool,
    enable: Option<String>,
    disable: Option<String>,
) -> Result<()> {
    // --enable and --disable are mutually exclusive write operations.
    if enable.is_some() && disable.is_some() {
        anyhow::bail!("--enable and --disable cannot be used together");
    }

    if let Some(model_id) = enable {
        let row = providers::show(ctx, config_path, name)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut updated = row.enabled_models.clone();
        if updated.iter().any(|m| m == &model_id) {
            eprintln!("`{model_id}` is already enabled for `{name}`");
            return Ok(());
        }
        updated.push(model_id.clone());
        providers::set_enabled_models(ctx, config_path, name, updated)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        eprintln!("enabled `{model_id}` for provider `{name}`");
        return Ok(());
    }

    if let Some(model_id) = disable {
        let row = providers::show(ctx, config_path, name)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let before_len = row.enabled_models.len();
        let updated: Vec<String> = row
            .enabled_models
            .into_iter()
            .filter(|m| m != &model_id)
            .collect();
        if updated.len() == before_len {
            anyhow::bail!("`{model_id}` is not in the enabled_models list for `{name}`");
        }
        providers::set_enabled_models(ctx, config_path, name, updated)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        eprintln!("disabled `{model_id}` for provider `{name}`");
        return Ok(());
    }

    // Read-only display path.
    let cat = providers_catalog::get(ctx, config_path, name)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let cat = match cat {
        Some(c) => c,
        None => {
            eprintln!(
                "no cached catalog for `{name}` — run `xvn provider refresh-models --name {name}` first"
            );
            return Ok(());
        }
    };

    // Load the allowlist from config. Empty = nothing configured yet.
    let row = providers::show(ctx, config_path, name)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let allowlist = &row.enabled_models;
    let filtering = !show_all && !allowlist.is_empty();

    let visible: Vec<_> = cat
        .models
        .iter()
        .filter(|m| should_show_catalog_model(&m.id, allowlist, show_all))
        .collect();

    println!(
        "{} ({} models{})",
        cat.provider,
        visible.len(),
        if filtering {
            format!(", {} in catalog — use --all to see everything", cat.models.len())
        } else {
            format!(", fetched {}", cat.fetched_at.to_rfc3339())
        }
    );
    println!("{:<48} {:>10} {:>10} {:>6}", "ID", "CONTEXT", "MAX_OUT", "REASON");
    for m in &visible {
        let ctx_str = m
            .context_window
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into());
        let out_str = m
            .max_output_tokens
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into());
        let reason = match m.supports_reasoning {
            Some(true) => "yes",
            Some(false) => "no",
            None => "—",
        };
        println!("{:<48} {:>10} {:>10} {:>6}", m.id, ctx_str, out_str, reason);
    }
    Ok(())
}

fn should_show_catalog_model(model_id: &str, allowlist: &[String], show_all: bool) -> bool {
    show_all || allowlist.is_empty() || allowlist.iter().any(|allowed| allowed == model_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_engine::api::Actor;

    async fn test_ctx(dir: &tempfile::TempDir) -> ApiContext {
        ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
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
        list_legacy(&ctx, &config).await.unwrap();
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
            false, // dry_run
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
        remove(&ctx, &config, "ephemeral", false /* dry_run */)
            .await
            .unwrap();
        let cfg = xvision_core::config::load_runtime(&config).unwrap();
        assert!(!cfg.providers.iter().any(|p| p.name == "ephemeral"));
    }

    /// Regression: `xvn provider ls` must resolve to the `list` subcommand
    /// (visible alias). This test FAILS before the alias is added and PASSES
    /// after — if the alias is ever removed the test will catch it.
    #[test]
    fn provider_list_has_ls_visible_alias() {
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        let provider = cmd.find_subcommand("provider").expect("provider subcommand");
        let list = provider.find_subcommand("list").expect("list subcommand");
        let aliases: Vec<&str> = list.get_visible_aliases().collect();
        assert!(
            aliases.contains(&"ls"),
            "expected `ls` visible alias on `xvn provider list`; aliases: {aliases:?}",
        );
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

    #[tokio::test]
    async fn models_enable_adds_to_allowlist() {
        let dir = tempfile::tempdir().unwrap();
        let config = write_min_config(&dir);
        let ctx = test_ctx(&dir).await;
        models(
            &ctx,
            &config,
            "anthropic",
            false,
            Some("claude-test".into()),
            None,
        )
        .await
        .unwrap();
        let cfg = xvision_core::config::load_runtime(&config).unwrap();
        let p = cfg.providers.iter().find(|p| p.name == "anthropic").unwrap();
        assert!(p.enabled_models.contains(&"claude-test".to_string()));
    }

    #[tokio::test]
    async fn models_disable_removes_from_allowlist() {
        let dir = tempfile::tempdir().unwrap();
        let config = write_min_config(&dir);
        let ctx = test_ctx(&dir).await;
        // Enable two models first, then disable one.
        models(
            &ctx,
            &config,
            "anthropic",
            false,
            Some("claude-keep".into()),
            None,
        )
        .await
        .unwrap();
        models(
            &ctx,
            &config,
            "anthropic",
            false,
            Some("claude-remove-me".into()),
            None,
        )
        .await
        .unwrap();
        models(
            &ctx,
            &config,
            "anthropic",
            false,
            None,
            Some("claude-remove-me".into()),
        )
        .await
        .unwrap();
        let cfg = xvision_core::config::load_runtime(&config).unwrap();
        let p = cfg.providers.iter().find(|p| p.name == "anthropic").unwrap();
        assert!(!p.enabled_models.contains(&"claude-remove-me".to_string()));
        assert!(p.enabled_models.contains(&"claude-keep".to_string()));
    }

    #[tokio::test]
    async fn models_disable_unknown_model_errors() {
        let dir = tempfile::tempdir().unwrap();
        let config = write_min_config(&dir);
        let ctx = test_ctx(&dir).await;
        let err = models(&ctx, &config, "anthropic", false, None, Some("ghost".into()))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("not in the enabled_models list"));
    }

    #[tokio::test]
    async fn models_enable_and_disable_flags_are_exclusive() {
        let dir = tempfile::tempdir().unwrap();
        let config = write_min_config(&dir);
        let ctx = test_ctx(&dir).await;
        let err = models(
            &ctx,
            &config,
            "anthropic",
            false,
            Some("a".into()),
            Some("b".into()),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("cannot be used together"));
    }

    #[tokio::test]
    async fn models_without_cached_catalog_returns_soft_hint() {
        let dir = tempfile::tempdir().unwrap();
        let config = write_min_config(&dir);
        let ctx = test_ctx(&dir).await;

        models(&ctx, &config, "anthropic", false, None, None)
            .await
            .expect("missing catalog should print a refresh hint without failing");
    }

    #[test]
    fn model_catalog_filter_honors_nonempty_allowlist() {
        let allowlist = vec!["claude-keep".to_string()];
        assert!(should_show_catalog_model("claude-keep", &allowlist, false));
        assert!(!should_show_catalog_model("claude-hide", &allowlist, false));
    }

    #[test]
    fn model_catalog_filter_show_all_bypasses_allowlist() {
        let allowlist = vec!["claude-keep".to_string()];
        assert!(should_show_catalog_model("claude-hide", &allowlist, true));
    }

    #[test]
    fn model_catalog_filter_empty_allowlist_shows_everything() {
        assert!(should_show_catalog_model("claude-any", &[], false));
    }

    #[test]
    fn provider_catalog_probe_url_preserves_v1_bases() {
        assert_eq!(
            provider_catalog_probe_url("openai-compat", "https://api.openai.com/v1"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            provider_catalog_probe_url("openai-compat", "https://openrouter.ai/api/v1/"),
            "https://openrouter.ai/api/v1/models"
        );
    }

    #[test]
    fn provider_catalog_probe_url_targets_local_provider_shapes() {
        assert_eq!(
            provider_catalog_probe_url("ollama", "http://localhost:11434/"),
            "http://localhost:11434/api/tags"
        );
        assert_eq!(
            provider_catalog_probe_url("llama-cpp", "http://localhost:8080"),
            "http://localhost:8080/v1/models"
        );
        assert_eq!(
            provider_catalog_probe_url("vllm", "http://localhost:8000/v1"),
            "http://localhost:8000/v1/models"
        );
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

[default_llm]
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
