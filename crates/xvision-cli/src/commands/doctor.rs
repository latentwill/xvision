use std::collections::HashSet;
use std::path::PathBuf;

use clap::Args;
use serde::Serialize;
use xvision_engine::api::memory::{self as memory_api, MemoryStatus};
use xvision_engine::api::search as api_search;
use xvision_engine::api::settings::providers::{self, EffectiveProvider};
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};

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
    strategies_on_disk: usize,
    /// Count of on-disk strategy bundles that are also present in search_index
    /// (i.e. on-disk ∩ indexed). Does NOT count indexed rows with no on-disk file.
    strategies_on_disk_and_indexed: usize,
    strategies_orphaned: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    docker_home_warning: Option<String>,
    /// Cortex memory health: store path + writability, embedder
    /// presence/source, grace window, per-namespace live-observation
    /// counts. Sourced from `api::memory::status` so the doctor report
    /// can't drift from `xvn memory status`. Defaults (all-empty) when the
    /// store can't be opened (e.g. read-only home), so the report still
    /// renders.
    #[serde(default)]
    memory: MemoryStatus,
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
    let (strategies_on_disk, strategies_on_disk_and_indexed, strategies_orphaned) =
        collect_strategy_counts(&xvn_home).await;
    let docker_home_warning = check_docker_home(&xvn_home.display().to_string());

    // Memory health is best-effort: a read-only home or absent store must
    // not abort the report (doctor exists to surface such conditions). On
    // any failure we fall back to the default (all-empty) MemoryStatus.
    let memory: MemoryStatus = match memory_api::open_default_store().await {
        Ok(store) => memory_api::status(&store, &xvn_home).await.unwrap_or_default(),
        Err(_) => MemoryStatus::default(),
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
        strategies_on_disk,
        strategies_on_disk_and_indexed,
        strategies_orphaned,
        docker_home_warning,
        memory,
    };

    if cmd.json {
        crate::io::print_json(&report).map_err(|e| anyhow::anyhow!("emit doctor json: {}", e.source))?;
    } else {
        print_report(&report);
    }
    Ok(())
}

fn print_report(report: &DoctorReport) {
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
    println!("strategies_on_disk    {}", report.strategies_on_disk);
    println!(
        "strategies_on_disk_and_indexed    {}",
        report.strategies_on_disk_and_indexed
    );
    println!("strategies_orphaned   {}", report.strategies_orphaned);
    println!("templates             (registry removed; see $XVN_HOME/strategies/library)");
    if report.strategies_orphaned > 0 {
        println!(
            "hint: run xvn strategy reindex to backfill {} orphaned strategies",
            report.strategies_orphaned
        );
    }
    if let Some(warn) = &report.docker_home_warning {
        println!("warning: {warn}");
    }
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
    println!("memory");
    println!("  store_path          {}", report.memory.store_path);
    println!("  writable            {}", report.memory.writable);
    println!("  embedder_present    {}", report.memory.embedder_present);
    println!(
        "  embedder_id         {}",
        report.memory.embedder_id.as_deref().unwrap_or("-")
    );
    println!(
        "  embedder_source     {}",
        report.memory.embedder_source.as_deref().unwrap_or("-")
    );
    println!("  grace_days          {}", report.memory.grace_days);
    println!("  namespaces          {}", report.memory.namespaces.len());
}

async fn collect_strategy_counts(xvn_home: &std::path::Path) -> (usize, usize, usize) {
    let store = FilesystemStore::new(strategy_store_dir(xvn_home));
    let on_disk = store.list().await.unwrap_or_default();
    let db_path = xvn_home.join("xvn.db");
    let indexed_ids = api_search::indexed_strategy_ids_raw(&db_path).await;
    let indexed_set: HashSet<&str> = indexed_ids.iter().map(|s| s.as_str()).collect();
    let indexed_count = on_disk
        .iter()
        .filter(|id| indexed_set.contains(id.as_str()))
        .count();
    let orphaned = on_disk.len().saturating_sub(indexed_count);
    (on_disk.len(), indexed_count, orphaned)
}

fn check_docker_home(cli_home: &str) -> Option<String> {
    let out = std::process::Command::new("docker")
        .args([
            "inspect",
            "--format={{range .Config.Env}}{{println .}}{{end}}",
            "xvn-app",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        if let Some(val) = line.strip_prefix("XVN_HOME=") {
            if val != cli_home {
                return Some(format!(
                    "docker xvn-app XVN_HOME={val} differs from CLI XVN_HOME={cli_home}"
                ));
            }
            return None;
        }
    }
    None
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
    xvision_core::config::runtime_config_path(xvn_home)
}
