//! `xvn obs janitor run --once` — fire the retention janitor on demand.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use sqlx::SqlitePool;
use xvision_observability::{
    default_config_path, resolve_retention, run_janitor_once, BlobStore, CliOverrides,
    JanitorConfig,
};

#[derive(Args, Debug)]
pub struct JanitorCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Run a single retention pass and exit.
    Run(RunArgs),
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Path to observability.toml (resolves TTL + max-bytes from the
    /// stored policy). Defaults to `$XVN_HOME/config/observability.toml`.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Path to the sqlite database that holds the agent_runs tables.
    /// Defaults to the engine's store at `data/store.db`.
    #[arg(long, default_value = "data/store.db")]
    pub db: PathBuf,
    /// Root of the content-addressed blob store.
    /// Defaults to `$XVN_HOME/agent_runs/blobs/`.
    #[arg(long)]
    pub blob_root: Option<PathBuf>,
    /// Run exactly one pass and exit. Reserved for future
    /// `--watch`/`--interval` flags; today the only mode is `--once`.
    #[arg(long, default_value_t = true)]
    pub once: bool,
}

pub async fn run(cmd: JanitorCmd) -> anyhow::Result<()> {
    match cmd.op {
        Op::Run(args) => run_once_cmd(args).await,
    }
}

async fn run_once_cmd(args: RunArgs) -> anyhow::Result<()> {
    // Resolve retention to get TTL + max-bytes; CLI overrides empty
    // because janitor takes its inputs from the persisted policy.
    let cfg_path = args.config.unwrap_or_else(default_config_path);
    let view = resolve_retention(&cfg_path, &CliOverrides::default())?;
    let janitor_cfg = JanitorConfig {
        payload_ttl_days: view.payload_ttl_days.value,
        max_payload_bytes: view.max_payload_bytes.value,
    };

    let blob_root = args.blob_root.unwrap_or_else(default_blob_root);
    let blob_store = BlobStore::new(blob_root);

    let url = format!("sqlite://{}?mode=rwc", args.db.display());
    let pool = SqlitePool::connect(&url).await?;

    let stats = run_janitor_once(&pool, &blob_store, &janitor_cfg).await?;
    println!(
        "janitor: row_refs_nulled={} blob_files_deleted={} bytes_freed={}",
        stats.row_refs_nulled, stats.blob_files_deleted, stats.bytes_freed
    );
    if !args.once {
        eprintln!("note: only --once is supported today; exiting after a single pass");
    }
    Ok(())
}

fn default_blob_root() -> PathBuf {
    let base = std::env::var("XVN_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|h| h.join(".xvn"))
                .unwrap_or_else(|| PathBuf::from("."))
        });
    base.join("agent_runs").join("blobs")
}
