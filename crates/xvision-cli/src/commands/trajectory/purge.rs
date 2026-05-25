//! `xvn trajectory purge`
//!
//! Delete recordings (and their frames + blobs) by TTL:
//!   --before <rfc3339>  purge recordings that expire before this timestamp
//!   --expired           purge all recordings whose expires_at <= now

use clap::Args;
use std::path::PathBuf;

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct PurgeArgs {
    /// Purge recordings that expired before this RFC 3339 timestamp.
    #[arg(long, conflicts_with = "expired")]
    pub before: Option<String>,

    /// Purge all recordings whose `expires_at` is in the past (≤ now).
    #[arg(long)]
    pub expired: bool,

    /// Path to the SQLite database (default: the migrated `$XVN_HOME/xvn.db`).
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Blob store root directory (default: `$XVN_HOME/agent_runs/blobs`).
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

    if args.dry_run {
        eprintln!("(dry-run: no changes made)");
        return Ok(());
    }

    let store = super::open_store(args.db, args.blob_root).await?;

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
