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

use clap::{Args, Subcommand};

use crate::exit::CliResult;

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
