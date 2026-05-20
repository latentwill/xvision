//! `xvn strategies` — operations on the per-user strategies folder
//! at `$XVN_HOME/strategies/`. The v1 surface is the `init` sub-verb,
//! which materializes the five allowlisted subfolders and copies the
//! curated `docs/strategies/templates/**` library into
//! `library/` along with a provenance manifest. Wave-2 sibling track
//! adds the `import` sub-verb.
//!
//! See contracts at:
//! - `team/contracts/strategies-folder-surface.md` (read surface)
//! - `team/contracts/strategies-folder-prepopulation.md` (this track)
//! - `team/contracts/strategies-folder-import.md` (sibling track)

use std::path::PathBuf;

use clap::{Args, Subcommand};

use xvision_engine::strategies_folder::prepop::{self, InitOptions};

use crate::exit::{CliError, CliResult, XvnExit};

/// `xvn strategies` parent verb. Sub-verbs live as variants of
/// [`StrategiesOp`] so additional operators (`import`, future
/// `compact`, etc.) can be added without restructuring.
#[derive(Args, Debug)]
pub struct StrategiesCmd {
    #[command(subcommand)]
    pub op: StrategiesOp,
}

#[derive(Subcommand, Debug)]
pub enum StrategiesOp {
    /// Initialize `$XVN_HOME/strategies/` — creates the five
    /// subfolders (`notes`, `docs`, `strategy-files`, `evals`,
    /// `library`) if missing, then copies the curated
    /// `docs/strategies/` template library into `library/` along
    /// with a provenance manifest at `library/.from-docs.json`.
    ///
    /// Idempotent: re-running without `--force` preserves
    /// user-edited copies and surfaces a drift finding to stderr.
    /// Use `--force` to overwrite user edits.
    Init(InitArgs),
}

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Overwrite user-edited library files (skip drift detection).
    /// Without this flag, any file diverging from the manifest's
    /// recorded sha256 is preserved and a finding is emitted to
    /// stderr.
    #[arg(long)]
    pub force: bool,
    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

pub async fn run(cmd: StrategiesCmd) -> CliResult<()> {
    match cmd.op {
        StrategiesOp::Init(args) => run_init(args).await,
    }
}

async fn run_init(args: InitArgs) -> CliResult<()> {
    let xvn_home = crate::commands::home::resolve_xvn_home(args.xvn_home.clone())
        .map_err(|e| CliError {
            exit: XvnExit::Usage,
            source: e,
        })?;

    let report = prepop::init(&xvn_home, InitOptions { force: args.force })
        .await
        .map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("strategies init: {e}"),
        })?;

    // Summary line on stdout — the at-a-glance success signal.
    let root = xvision_engine::strategies_folder::folder_root(&xvn_home);
    println!(
        "strategies folder initialized at {} ({} new, {} refreshed, {} preserved, {} stale)",
        root.display(),
        report.new_files.len(),
        report.refreshed_files.len(),
        report.drift.len(),
        report.stale_source.len()
    );

    if !report.created_subfolders.is_empty() {
        println!(
            "  created subfolders: {}",
            report.created_subfolders.join(", ")
        );
    }

    // Findings on stderr in the standard `KIND: message` format the
    // CLI uses elsewhere. Operators running `xvn strategies init`
    // manually see them; downstream scripts can grep for the prefix.
    for rel in &report.drift {
        eprintln!(
            "strategies_library_drift: {} diverges from the manifest's recorded sha256 — copy preserved (rerun with --force to overwrite)",
            rel
        );
    }
    for rel in &report.stale_source {
        eprintln!(
            "strategies_library_stale_source: manifest entry {} points at a docs/strategies/ source that no longer exists — copy preserved",
            rel
        );
    }

    Ok(())
}
