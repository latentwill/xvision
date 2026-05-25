//! `xvn trajectory validate <recording_id>`
//!
//! Exits 0 when the recording is intact (status=complete, no frame gaps).
//! Exits non-zero when the recording is corrupt, incomplete, or has frame gaps.

use clap::Args;
use std::path::PathBuf;

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Recording id to validate.
    pub recording_id: String,

    /// Path to the SQLite database (default: the migrated `$XVN_HOME/xvn.db`).
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Blob store root directory (default: `$XVN_HOME/agent_runs/blobs`).
    #[arg(long)]
    pub blob_root: Option<PathBuf>,

    /// Suppress output (exit code only).
    #[arg(long, short)]
    pub quiet: bool,
}

pub async fn run(args: ValidateArgs) -> CliResult<()> {
    let store = super::open_store(args.db, args.blob_root).await?;

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
