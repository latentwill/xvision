//! `xvn trajectory …` — trajectory store operator surface (item 10, Stage 2).
//!
//! Subcommands:
//!   inspect  <recording_id>     — print schema version, status, fingerprint, frame counts
//!   validate <recording_id>     — exit 0 if intact, non-zero if corrupt/incomplete/gapped
//!   purge    --before|--expired — delete recordings past TTL, GC orphaned blobs
//!   reindex                     — recompute key_fingerprint over all recordings

pub mod inspect;
pub mod purge;
pub mod reindex;
pub mod validate;

use std::path::PathBuf;

use clap::{Args, Subcommand};
use xvision_engine::api::{Actor, ApiContext};
use xvision_observability::config::RetentionMode;
use xvision_observability::trajectory::store::TrajectoryStore;
use xvision_observability::BlobStore;

use crate::exit::{CliError, CliResult, XvnExit};

/// Open the trajectory store against the SAME migrated DB + blob tree the
/// record path writes to.
///
/// §2-D review nit #1: recordings are persisted to the `ApiContext` DB
/// (`$XVN_HOME/xvn.db`, which owns migration 040) and blobs land under
/// `$XVN_HOME/agent_runs/blobs` (see `api::eval` recording wiring +
/// `cline_recording::open_store`). The earlier CLI default pointed at a raw,
/// un-migrated `$XVN_HOME/data/store.db` + `$XVN_HOME/data/blobs`, so it could
/// never read a real recording. We now route through `ApiContext::open`, which
/// applies every embedded migration (including 040, `migrate_trajectory_frames`)
/// before the store opens over the already-migrated pool.
///
/// `--db` / `--blob-root` overrides are honoured for ad-hoc inspection of a
/// store at a non-default location; when `db_override` is set we open it
/// directly (and still apply migration 040 so a hand-placed file is readable),
/// otherwise we go through the canonical `ApiContext` path.
pub async fn open_store(
    db_override: Option<PathBuf>,
    blob_override: Option<PathBuf>,
) -> CliResult<TrajectoryStore> {
    let xvn_home = crate::commands::home::resolve_xvn_home_env().map_err(|e| CliError {
        exit: XvnExit::Usage,
        source: e,
    })?;

    let (pool, blob_root) = if let Some(db_path) = db_override {
        // Explicit DB override: open it directly and apply migration 040 so a
        // hand-placed / relocated file carries the trajectory schema.
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
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
                        source: anyhow::anyhow!("open database at {}: {e}", db_path.display()),
                    }
                }
            })?;
        // Ensure the trajectory tables exist on the overridden DB. The main
        // migrator gates this on the recordings table's existence and the
        // statements are idempotent, so this is a no-op on an already-migrated
        // file.
        ApiContext::ensure_trajectory_schema(&pool)
            .await
            .map_err(|e| CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("apply trajectory migration: {e}"),
            })?;
        let blob_root = blob_override
            .unwrap_or_else(|| xvn_home.join("agent_runs").join("blobs"));
        (pool, blob_root)
    } else {
        // Canonical path: ApiContext::open migrates `$XVN_HOME/xvn.db` (incl.
        // migration 040) and hands us the exact pool the record path writes to.
        let ctx = ApiContext::open(&xvn_home, Actor::Cli { user: whoami() })
            .await
            .map_err(|e| CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("open xvn.db ({}): {e}", xvn_home.display()),
            })?;
        let blob_root = blob_override
            .unwrap_or_else(|| ctx.xvn_home.join("agent_runs").join("blobs"));
        (ctx.db.clone(), blob_root)
    };

    Ok(TrajectoryStore::new(
        pool,
        BlobStore::new(blob_root),
        RetentionMode::FullDebug,
    ))
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "cli".to_string())
}

#[derive(Args, Debug)]
pub struct TrajectoryCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Print schema version, status, key fingerprint, and per-slot/step
    /// frame counts for a trajectory recording.
    Inspect(inspect::InspectArgs),
    /// Validate a recording (exit non-zero on missing/out-of-order frames
    /// or non-complete status).
    Validate(validate::ValidateArgs),
    /// Purge recordings past their TTL and GC orphaned blobs.
    Purge(purge::PurgeArgs),
    /// Recompute `key_fingerprint` over all recordings after a
    /// schema-compatible change.
    Reindex(reindex::ReindexArgs),
}

pub async fn run(cmd: TrajectoryCmd) -> CliResult<()> {
    match cmd.op {
        Op::Inspect(a) => inspect::run(a).await,
        Op::Validate(a) => validate::run(a).await,
        Op::Purge(a) => purge::run(a).await,
        Op::Reindex(a) => reindex::run(a).await,
    }
}
