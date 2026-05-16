//! `xvn obs …` — observability operator surface (retention + janitor).
//!
//! Wired from the top-level `xvn` enum in `crates/xvision-cli/src/lib.rs`.
//! Subcommands live in this module so the observability work doesn't
//! sprawl into `commands/mod.rs`.

pub mod janitor;
pub mod retention;

use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct ObsCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Inspect / edit the agent-run retention policy.
    Retention(retention::RetentionCmd),
    /// Run the retention janitor (TTL + max-bytes pass).
    Janitor(janitor::JanitorCmd),
}

pub async fn run(cmd: ObsCmd) -> anyhow::Result<()> {
    match cmd.op {
        Op::Retention(c) => retention::run(c).await,
        Op::Janitor(c) => janitor::run(c).await,
    }
}
