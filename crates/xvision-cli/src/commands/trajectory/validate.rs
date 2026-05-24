//! `xvn trajectory validate <recording_id>`
//!
//! Exits 0 when the recording is intact (status=complete, no frame gaps).
//! Exits non-zero when the recording is corrupt, incomplete, or has frame gaps.

use clap::Args;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use xvision_observability::BlobStore;
use xvision_observability::config::RetentionMode;
use xvision_observability::trajectory::store::TrajectoryStore;

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Recording id to validate.
    pub recording_id: String,

    /// Path to the SQLite database.
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Blob store root directory.
    #[arg(long)]
    pub blob_root: Option<PathBuf>,

    /// Suppress output (exit code only).
    #[arg(long, short)]
    pub quiet: bool,
}

pub async fn run(args: ValidateArgs) -> CliResult<()> {
    let home = default_xvn_home();
    let db_path = args.db.as_deref().map(|p| p.to_path_buf())
        .unwrap_or_else(|| home.join("data").join("store.db"));
    let blob_root = args.blob_root.as_deref().map(|p| p.to_path_buf())
        .unwrap_or_else(|| home.join("data").join("blobs"));

    let url = format!("sqlite://{}?mode=ro", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .map_err(|e| CliError {
            exit: XvnExit::NotFound,
            source: anyhow::anyhow!("open database at {}: {e}", db_path.display()),
        })?;

    let store = TrajectoryStore::new(pool, BlobStore::new(blob_root), RetentionMode::FullDebug);

    match store.validate(&args.recording_id).await {
        Ok(()) => {
            if !args.quiet {
                println!("OK: recording {} is complete and intact", args.recording_id);
            }
            Ok(())
        }
        Err(msg) => {
            if !args.quiet {
                eprintln!("INVALID: {msg}");
            }
            Err(CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("{msg}"),
            })
        }
    }
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
