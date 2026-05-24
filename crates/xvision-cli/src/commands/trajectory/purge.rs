//! `xvn trajectory purge`
//!
//! Delete recordings (and their frames + blobs) by TTL:
//!   --before <rfc3339>  purge recordings that expire before this timestamp
//!   --expired           purge all recordings whose expires_at <= now

use clap::Args;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use xvision_observability::BlobStore;
use xvision_observability::config::RetentionMode;
use xvision_observability::trajectory::store::TrajectoryStore;

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct PurgeArgs {
    /// Purge recordings that expired before this RFC 3339 timestamp.
    #[arg(long, conflicts_with = "expired")]
    pub before: Option<String>,

    /// Purge all recordings whose `expires_at` is in the past (≤ now).
    #[arg(long)]
    pub expired: bool,

    /// Path to the SQLite database.
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Blob store root directory.
    #[arg(long)]
    pub blob_root: Option<PathBuf>,

    /// Print what would be deleted without deleting.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(args: PurgeArgs) -> CliResult<()> {
    if !args.expired && args.before.is_none() {
        return Err(CliError::usage(anyhow::anyhow!(
            "one of --before <rfc3339> or --expired is required"
        )));
    }

    let home = default_xvn_home();
    let db_path = args.db.as_deref().map(|p| p.to_path_buf())
        .unwrap_or_else(|| home.join("data").join("store.db"));
    let blob_root = args.blob_root.as_deref().map(|p| p.to_path_buf())
        .unwrap_or_else(|| home.join("data").join("blobs"));

    if args.dry_run {
        eprintln!("(dry-run: no changes made)");
        return Ok(());
    }

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

    let deleted = if let Some(before) = &args.before {
        store.purge_before(before).await.map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("purge_before: {e}"),
        })?
    } else {
        // --expired: use now
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        store.purge_expired(now_ms).await.map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("purge_expired: {e}"),
        })?
    };

    println!("purged {deleted} recording(s)");
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
