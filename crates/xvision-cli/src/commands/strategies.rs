//! `xvn strategies ...` — operator surface for the per-user strategies
//! folder (`$XVN_HOME/strategies/`).
//!
//! v1 surface (V2F wave-2):
//! - `xvn strategies init [--force]` — materialize the five
//!   allowlisted subfolders and copy the curated
//!   `docs/strategies/templates/**` library into `library/` along
//!   with a provenance manifest at `library/.from-docs.json`.
//!   Idempotent re-runs preserve user-edited copies unless `--force`.
//! - `xvn strategies import <path> [--to <subfolder>] [--no-clobber]`
//!   — copy a file into the strategies folder. Default subfolder is
//!   chosen by extension (.md/.txt → notes/, .pdf/.csv → docs/,
//!   .json → strategy-files/). `--to` overrides with the allowlist
//!   (`notes` / `docs` / `strategy-files` / `evals` / `library`).
//!   `--no-clobber` skips when the target already exists. `.pdf`
//!   triggers a `pdftotext` summary sidecar when the binary is on
//!   PATH; `.csv` gets a markdown-table sidecar.
//!
//! See contracts:
//! - `team/contracts/strategies-folder-surface.md` (read surface)
//! - `team/contracts/strategies-folder-prepopulation.md` (init verb)
//! - `team/contracts/strategies-folder-import.md` (import verb)

use std::path::PathBuf;

use clap::{Args, Subcommand};

use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::strategies_folder::{
    self, prepop, ImportOptions, ImportOutcome, SUBFOLDER_ALLOWLIST,
};
use xvision_engine::strategies_folder::prepop::InitOptions;

use crate::exit::{CliError, CliResult, XvnExit};

/// `xvn strategies` parent verb. Sub-verbs live as variants of
/// [`StrategiesOp`] so additional operators (future `compact`, etc.)
/// can be added without restructuring.
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
    /// Copy a file into `$XVN_HOME/strategies/`.
    ///
    /// Default destination is chosen by extension:
    ///
    ///   .md / .txt  → notes/
    ///   .pdf / .csv → docs/
    ///   .json       → strategy-files/
    ///
    /// `.pdf` files trigger a `pdftotext`-based summary sidecar
    /// (`<name>.summary.md`) when the binary is on PATH. `.csv`
    /// files get a markdown-table sidecar with the header + first
    /// 50 rows. Other types are imported verbatim.
    ///
    /// Re-importing the same file overwrites by default. Pass
    /// `--no-clobber` to skip when the target already exists.
    Import {
        /// Path to the source file on the host filesystem.
        path: PathBuf,
        /// Destination subfolder. Must be one of:
        /// `notes`, `docs`, `strategy-files`, `evals`, `library`.
        /// When omitted, picks a default based on extension (see above).
        #[arg(long = "to")]
        to: Option<String>,
        /// Refuse to overwrite an existing file in the destination.
        /// Default behavior overwrites (most-recent-edit wins).
        #[arg(long = "no-clobber", default_value_t = false)]
        no_clobber: bool,
        /// Emit the resulting `ImportOutcome` as JSON instead of a
        /// human-readable summary.
        #[arg(long)]
        json: bool,
    },
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
        StrategiesOp::Import {
            path,
            to,
            no_clobber,
            json,
        } => run_import(path, to, no_clobber, json).await,
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

fn home() -> PathBuf {
    crate::commands::home::resolve_xvn_home_env().expect("resolve XVN_HOME")
}

async fn open_ctx() -> CliResult<ApiContext> {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&home(), Actor::Cli { user })
        .await
        .map_err(|e| CliError::upstream(anyhow::anyhow!("open ApiContext: {e}")))
}

fn api_to_cli(prefix: &str, e: ApiError) -> CliError {
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

async fn run_import(
    path: PathBuf,
    to: Option<String>,
    no_clobber: bool,
    json: bool,
) -> CliResult<()> {
    if let Some(name) = to.as_deref() {
        if !SUBFOLDER_ALLOWLIST.contains(&name) {
            return Err(CliError::usage(anyhow::anyhow!(
                "--to '{name}' is not in the allowlist ({})",
                SUBFOLDER_ALLOWLIST.join(", ")
            )));
        }
    }

    let ctx = open_ctx().await?;
    let outcome = strategies_folder::import_from_path(
        &ctx,
        &path,
        ImportOptions {
            subfolder: to,
            clobber: !no_clobber,
        },
    )
    .await
    .map_err(|e| api_to_cli("strategies import", e))?;

    emit_outcome(&outcome, json)
}

fn emit_outcome(outcome: &ImportOutcome, json: bool) -> CliResult<()> {
    if json {
        let body = serde_json::to_string_pretty(outcome).map_err(|e| {
            CliError::upstream(anyhow::anyhow!("serialize ImportOutcome: {e}"))
        })?;
        println!("{body}");
        return Ok(());
    }
    println!("imported: {}", outcome.entry.rel_path);
    if let Some(summary) = &outcome.summary {
        println!("summary:  {}", summary.rel_path);
    }
    for finding in &outcome.findings {
        eprintln!("finding[{}]: {}", finding.code, finding.detail);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn strategies_subcommands_registered() {
        let cmd = crate::Cli::command();
        let strategies = cmd
            .find_subcommand("strategies")
            .expect("xvn strategies subcommand should be registered");
        strategies
            .find_subcommand("init")
            .expect("xvn strategies init sub-verb should be registered");
        strategies
            .find_subcommand("import")
            .expect("xvn strategies import sub-verb should be registered");
    }

    #[test]
    fn strategies_import_accepts_to_and_no_clobber() {
        let result = crate::Cli::command().try_get_matches_from([
            "xvn",
            "strategies",
            "import",
            "/tmp/foo.md",
            "--to",
            "notes",
            "--no-clobber",
        ]);
        assert!(result.is_ok(), "expected ok, got {result:?}");
    }
}
