//! `xvn trajectory inspect <recording_id>`
//!
//! Prints: schema version, status, key fingerprint, and per-slot/step frame
//! counts for a trajectory recording.

use clap::Args;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use xvision_observability::BlobStore;
use xvision_observability::config::RetentionMode;
use xvision_observability::trajectory::store::TrajectoryStore;

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Recording id (e.g. `rec_<ulid>`).
    pub recording_id: String,

    /// Path to the SQLite database (default: `$XVN_HOME/data/store.db`).
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Blob store root directory (default: `$XVN_HOME/data/blobs`).
    #[arg(long)]
    pub blob_root: Option<PathBuf>,
}

pub async fn run(args: InspectArgs) -> CliResult<()> {
    let (pool, blob_root) = open_pool_and_blob(args.db.as_deref(), args.blob_root.as_deref()).await?;
    let store = TrajectoryStore::new(pool, BlobStore::new(blob_root), RetentionMode::FullDebug);

    let info = store
        .get_recording(&args.recording_id)
        .await
        .map_err(|e| CliError {
            exit: XvnExit::NotFound,
            source: anyhow::anyhow!("recording not found: {e}"),
        })?;

    println!("recording_id:    {}", info.recording_id);
    println!("schema_version:  {}", info.schema_version);
    println!("status:          {}", info.status);
    println!("key_fingerprint: {}", info.key_fingerprint);
    println!("cycle_id:        {}", info.cycle_id);
    println!("slot_role:       {}", info.slot_role);
    println!(
        "arm_scope:       {}",
        info.arm_scope.as_deref().unwrap_or("(none)")
    );
    println!(
        "simulation_id:   {}",
        info.simulation_id.as_deref().unwrap_or("(none)")
    );
    println!("provider:        {}", info.provider);
    println!("model:           {}", info.model);
    println!(
        "model_version:   {}",
        info.model_version.as_deref().unwrap_or("(none)")
    );
    println!("system_prompt_hash: {}", info.system_prompt_hash);
    if let Some(reason) = &info.recovery_reason {
        println!("recovery_reason: {reason}");
    }
    println!("created_at:      {} ms", info.created_at);
    if let Some(ts) = info.completed_at {
        println!("completed_at:    {} ms", ts);
    }
    if let Some(ts) = info.expires_at {
        println!("expires_at:      {} ms", ts);
    }

    let counts = store
        .frame_counts(&args.recording_id)
        .await
        .map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("frame_counts: {e}"),
        })?;

    if counts.is_empty() {
        println!("\n(no frames recorded)");
    } else {
        println!("\nframe counts by (slot_role, step_index):");
        for c in &counts {
            println!("  {:<20} step {:>3}  {:>6} frames", c.slot_role, c.step_index, c.count);
        }
    }

    Ok(())
}

async fn open_pool_and_blob(
    db: Option<&std::path::Path>,
    blob_root: Option<&std::path::Path>,
) -> CliResult<(sqlx::SqlitePool, PathBuf)> {
    let home = default_xvn_home();
    let db_path = db.map(|p| p.to_path_buf()).unwrap_or_else(|| home.join("data").join("store.db"));
    let blob = blob_root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| home.join("data").join("blobs"));

    let url = format!("sqlite://{}?mode=ro", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .map_err(|e| {
            if !db_path.exists() {
                CliError {
                    exit: XvnExit::NotFound,
                    source: anyhow::anyhow!("database not found at {}: {e}", db_path.display()),
                }
            } else {
                CliError {
                    exit: XvnExit::Upstream,
                    source: anyhow::anyhow!("open database: {e}"),
                }
            }
        })?;

    Ok((pool, blob))
}

fn default_xvn_home() -> PathBuf {
    std::env::var("XVN_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|h| h.join(".xvn"))
                .unwrap_or_else(|| PathBuf::from(".xvn"))
        })
}
