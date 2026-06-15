//! `xvn run …` — agent-run observability operator surface.
//!
//! - `inspect <id>` writes the paired `xvn_run.json` + `xvn_report.md`
//!   deliverables into a directory.
//! - `export-run <id>` writes ONE full-fidelity, self-contained document
//!   ("the flywheel document") — every action in order, with blob-backed
//!   payloads inlined — in a single format, to a file or stdout.
//!
//! The verb is shaped as a subcommand so future operations on agent runs
//! (replay, tee-to-stream, …) have a stable home without breaking the
//! surface.

pub mod export_run;
pub mod inspect;

use clap::{Args, Subcommand};

use crate::exit::CliResult;

#[derive(Args, Debug)]
pub struct RunCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Materialize `xvn_run.json` + `xvn_report.md` for a finished
    /// agent run by reading the canonical SQLite ledger.
    Inspect(inspect::InspectArgs),
    /// Export ONE full-fidelity, self-contained document of a run (every
    /// action in order, payloads inlined) — the flywheel document.
    ExportRun(export_run::ExportRunArgs),
}

pub async fn run(cmd: RunCmd) -> CliResult<()> {
    match cmd.op {
        Op::Inspect(args) => inspect::run(args).await,
        Op::ExportRun(args) => export_run::run(args).await,
    }
}
