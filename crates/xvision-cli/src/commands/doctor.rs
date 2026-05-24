use std::path::PathBuf;

use clap::Args;
use serde::Serialize;
use xvision_engine::api::settings::providers::{self, EffectiveProvider};

#[derive(Args, Debug)]
pub struct DoctorCmd {
    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Emit a machine-readable report.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    xvn_home: String,
    db_path: String,
    config_path: String,
    provider_secrets_path: String,
    broker_secrets_path: String,
    strategies_dir: String,
    /// Post-2026-05-21 the strategy `template_registry` was removed.
    /// The doctor report's `templates` field is preserved on the wire
    /// (always an empty array) so external consumers keep parsing the
    /// JSON shape. Operator-readable strategy starters now live under
    /// `$XVN_HOME/strategies/library/` (initialized via
    /// `xvn strategies init`).
    templates: Vec<String>,
    config_exists: bool,
    provider_secrets_exists: bool,
    broker_secrets_exists: bool,
    remote_target: String,
    /// Canonical provider rollup — same shape as
    /// `xvn provider list --effective --json`. Sourced from
    /// `xvision_engine::api::settings::providers::effective_providers`
    /// so the doctor report can't drift from the CLI / dashboard verdict.
    /// Empty when the config file is missing (`config_exists == false`).
    #[serde(default)]
    providers: Vec<EffectiveProvider>,
}

pub async fn run(cmd: DoctorCmd) -> anyhow::Result<()> {
    let xvn_home = crate::commands::home::resolve_xvn_home(cmd.xvn_home)?;
    let config_path = runtime_config_path(&xvn_home);
    let provider_secrets_path = xvn_home.join("secrets").join("providers.toml");
    let broker_secrets_path = xvn_home.join("secrets").join("brokers.toml");
    let config_exists = config_path.exists();
    // Doctor is a diagnostic verb — a missing config or audit migration is
    // *what doctor exists to surface*, so any failure here degrades to an
    // empty `providers` block rather than aborting the report.
    let providers: Vec<EffectiveProvider> = if config_exists {
        match load_effective_providers(&xvn_home, &config_path).await {
            Ok(rows) => rows,
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let report = DoctorReport {
        xvn_home: xvn_home.display().to_string(),
        db_path: xvn_home.join("xvn.db").display().to_string(),
        config_path: config_path.display().to_string(),
        provider_secrets_path: provider_secrets_path.display().to_string(),
        broker_secrets_path: broker_secrets_path.display().to_string(),
        strategies_dir: xvn_home.join("strategies").display().to_string(),
        templates: Vec::new(),
        config_exists,
        provider_secrets_exists: provider_secrets_path.exists(),
        broker_secrets_exists: broker_secrets_path.exists(),
        remote_target: std::env::var("XVN_REMOTE_URL").unwrap_or_else(|_| "local".to_string()),
        providers,
    };

    if cmd.json {
        crate::io::print_json(&report).map_err(|e| anyhow::anyhow!("emit doctor json: {}", e.source))?;
    } else {
        println!("xvn_home              {}", report.xvn_home);
        println!("db_path               {}", report.db_path);
        println!("config_path           {}", report.config_path);
        println!("provider_secrets      {}", report.provider_secrets_path);
        println!("broker_secrets        {}", report.broker_secrets_path);
        println!("strategies_dir        {}", report.strategies_dir);
        println!("remote_target         {}", report.remote_target);
        println!("config_exists         {}", report.config_exists);
        println!("provider_secrets      {}", report.provider_secrets_exists);
        println!("broker_secrets        {}", report.broker_secrets_exists);
        println!("templates             (registry removed; see $XVN_HOME/strategies/library)");
        if report.providers.is_empty() {
            println!("providers             (none configured)");
        } else {
            println!("providers");
            for p in &report.providers {
                println!(
                    "  {:<16} enabled={}, key={}, {} models, launchable={}",
                    p.provider,
                    if p.enabled { "true" } else { "false" },
                    if p.has_key { "present" } else { "missing" },
                    p.models.len(),
                    if p.launchable { "true" } else { "false" },
                );
            }
        }
    }

    Ok(())
}

async fn load_effective_providers(
    xvn_home: &std::path::Path,
    config_path: &std::path::Path,
) -> anyhow::Result<Vec<EffectiveProvider>> {
    // Avoid `ApiContext::open` — opening the audit pool side-effects
    // tracing on stdout (the "memory: migrate" WARN) which would corrupt
    // `xvn doctor --json` output. The path-only variant returns the same
    // rollup.
    providers::effective_providers_with_paths(xvn_home, config_path)
        .await
        .map_err(|e| anyhow::anyhow!("effective_providers: {e}"))
}

fn runtime_config_path(xvn_home: &std::path::Path) -> PathBuf {
    if let Ok(path) = std::env::var("XVN_CONFIG_PATH") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    xvn_home.join("config").join("default.toml")
}
