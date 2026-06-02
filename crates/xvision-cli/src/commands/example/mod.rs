//! `xvn example` — seed curated example scenarios and tutorial artifacts
//! into the active `XVN_HOME` so first-run users have concrete market
//! windows to point at.
//!
//! Identification of seed-owned rows is delegated to
//! `xvision_engine::strategies::templates::{is_example_strategy,
//! is_example_scenario}` — the CLI never reads or replaces operator data.

use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::exit::{CliError, CliResult, XvnExit};

mod seed;

#[derive(Args, Debug)]
pub struct ExampleCmd {
    #[command(subcommand)]
    pub op: ExampleOp,
    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum ExampleOp {
    /// Populate (or refresh) the example scenarios and tutorial artifacts
    /// in the active XVN home.
    Seed(SeedArgs),
}

#[derive(Args, Debug)]
pub struct SeedArgs {
    /// Delete seed-owned legacy strategies, refresh seed-owned scenarios,
    /// then rewrite the tutorial. Without `--reset` the seed is
    /// idempotent: existing scenarios are skipped, never overwritten.
    #[arg(long, default_value_t = false)]
    pub reset: bool,
    /// Emit a structured JSON summary of what changed.
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

pub async fn run(cmd: ExampleCmd) -> CliResult<()> {
    match cmd.op {
        ExampleOp::Seed(args) => seed::run(cmd.xvn_home, args).await,
    }
}

/// Map an engine `ApiError` onto a typed `CliError`. Kept module-local
/// so it stays in sync with how strategy/scenario commands classify the
/// same shapes.
pub(crate) fn api_to_cli(prefix: &str, e: xvision_engine::api::ApiError) -> CliError {
    use xvision_engine::api::ApiError;
    let exit = match &e {
        ApiError::NotFound(_) => XvnExit::NotFound,
        ApiError::Validation(_) => XvnExit::Usage,
        ApiError::Conflict(_) => XvnExit::Conflict,
        ApiError::Internal(_) | ApiError::Db(_) | ApiError::Other(_) => XvnExit::Upstream,
    };
    CliError {
        exit,
        source: anyhow::anyhow!("{prefix}: {e}"),
    }
}

pub(crate) type CliResultUnit = CliResult<()>;
