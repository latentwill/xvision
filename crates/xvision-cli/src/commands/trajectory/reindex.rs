//! `xvn trajectory reindex`
//!
//! Recompute `key_fingerprint` over all recordings.  Run this after a
//! schema-compatible change to `TrajectoryKey` fields (e.g. adding a new
//! optional field with an empty default).
//!
//! This must NOT be called after a breaking schema version bump without also
//! bumping `TRAJECTORY_SCHEMA_VERSION` on the affected recordings.

use clap::Args;
use std::path::PathBuf;

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct ReindexArgs {
    /// Path to the SQLite database (default: the migrated `$XVN_HOME/xvn.db`).
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Blob store root directory (default: `$XVN_HOME/agent_runs/blobs`).
    #[arg(long)]
    pub blob_root: Option<PathBuf>,
}

pub async fn run(args: ReindexArgs) -> CliResult<()> {
    let store = super::open_store(args.db, args.blob_root).await?;

    let updated = store.reindex().await.map_err(|e| CliError {
        exit: XvnExit::Upstream,
        source: anyhow::anyhow!("reindex: {e}"),
    })?;

    println!("reindexed {updated} recording(s)");
    Ok(())
}
