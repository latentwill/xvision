//! `xvn run …` — agent-run observability operator surface.
//!
//! Today the only subcommand is `inspect <id>` which writes the
//! `xvn_run.json` + `xvn_report.md` deliverables. The verb is shaped
//! as a subcommand so future operations on agent runs (replay,
//! tee-to-stream, …) have a stable home without breaking the surface.

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
}

pub async fn run(cmd: RunCmd) -> CliResult<()> {
    match cmd.op {
        Op::Inspect(args) => inspect::run(args).await,
    }
}
