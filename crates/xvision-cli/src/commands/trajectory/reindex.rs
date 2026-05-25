//! `xvn trajectory reindex`
//!
//! Recompute `key_fingerprint` over all recordings.  Run this after a
//! schema-compatible change to `TrajectoryKey` fields (e.g. adding a new
//! optional field with an empty default).
//!
//! This must NOT be called after a breaking schema version bump without also
//! bumping `TRAJECTORY_SCHEMA_VERSION` on the affected recordings.

use clap::Args;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use xvision_observability::BlobStore;
use xvision_observability::config::RetentionMode;
use xvision_observability::trajectory::store::TrajectoryStore;

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct ReindexArgs {
    /// Path to the SQLite database.
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Blob store root directory.
    #[arg(long)]
    pub blob_root: Option<PathBuf>,
}

pub async fn run(args: ReindexArgs) -> CliResult<()> {
    let home = default_xvn_home();
    let db_path = args.db.as_deref().map(|p| p.to_path_buf())
        .unwrap_or_else(|| home.join("data").join("store.db"));
    let blob_root = args.blob_root.as_deref().map(|p| p.to_path_buf())
        .unwrap_or_else(|| home.join("data").join("blobs"));

    let url = format!("sqlite://{}", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .map_err(|e| CliError {
            exit: XvnExit::NotFound,
            source: anyhow::anyhow!("open database: {e}"),
        })?;

    let store = TrajectoryStore::new(pool, BlobStore::new(blob_root), RetentionMode::FullDebug);

    let updated = store.reindex().await.map_err(|e| CliError {
        exit: XvnExit::Upstream,
        source: anyhow::anyhow!("reindex: {e}"),
    })?;

    println!("reindexed {updated} recording(s)");
    Ok(())
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
