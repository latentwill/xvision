//! `xvn run export-run <id> --format md|json [--out <path>]` — the
//! full-fidelity export ("the flywheel document").
//!
//! Produces ONE complete, self-contained, high-fidelity document of a
//! run that the operator pastes to a coding agent: every action in order,
//! with full payloads. Unlike `xvn run inspect` (which writes the paired
//! `xvn_run.json` + `xvn_report.md` deliverables), `export-run` emits a
//! single document in the requested format with blob-backed model / tool
//! payloads INLINED from the content-addressed blob store, so the reader
//! never needs a follow-up `/blobs/:ref` fetch.
//!
//! `--out <path>` writes to a file; omit it (or pass `-`) to write to
//! stdout. Default format is markdown (the agent-readable form).
//!
//! Fidelity follows the run's retention: a `hash_only` run cannot inline
//! payloads, and the document's header says so explicitly.

use std::path::PathBuf;

use clap::{Args, ValueEnum};
use sqlx::SqlitePool;

use xvision_observability::{build_export_with_blobs, render_report, BlobStore};

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct ExportRunArgs {
    /// Agent run id (e.g. `run_01H…`). Required.
    pub id: String,

    /// Override the xvn home directory (default: `$XVN_HOME` or `~/.xvn`).
    /// Used to locate `xvn.db` and the blob store if not set explicitly.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,

    /// Path to the sqlite database holding the agent_runs tables.
    /// Defaults to `<xvn_home>/xvn.db`.
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Root of the content-addressed blob store used to inline payloads.
    /// Defaults to `<xvn_home>/agent_runs/blobs`.
    #[arg(long)]
    pub blob_root: Option<PathBuf>,

    /// Output file path. Omit (or pass `-`) to write to stdout.
    #[arg(long)]
    pub out: Option<String>,

    /// Document format. `md` is the agent-readable default.
    #[arg(long, value_enum, default_value_t = Format::Md)]
    pub format: Format,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "kebab-case")]
pub enum Format {
    Json,
    Md,
}

pub async fn run(args: ExportRunArgs) -> CliResult<()> {
    let home = args.xvn_home.clone().unwrap_or_else(default_xvn_home);
    let db_path = args.db.clone().unwrap_or_else(|| home.join("xvn.db"));
    let blob_root = args
        .blob_root
        .clone()
        .unwrap_or_else(|| home.join("agent_runs").join("blobs"));

    let pool = open_pool(&db_path).await?;
    let store = BlobStore::new(blob_root);

    let export = build_export_with_blobs(&pool, &args.id, Some(&store))
        .await
        .map_err(map_export_err)?;

    // Surface fidelity clearly when full payloads were never stored, so
    // the operator knows the document is structural-only, not a full
    // transcript.
    if export.retention_mode == "hash_only" {
        eprintln!(
            "xvn run export-run → note: run `{}` retention is `hash_only`; \
             prompts/responses/tool I/O were not retained (hashes only).",
            args.id
        );
    }

    let document = match args.format {
        Format::Json => {
            let mut bytes = serde_json::to_vec_pretty(&export).map_err(|e| CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("serialize export json: {e}"),
            })?;
            bytes.push(b'\n');
            bytes
        }
        Format::Md => {
            let mut md = render_report(&export).markdown.into_bytes();
            if !md.ends_with(b"\n") {
                md.push(b'\n');
            }
            md
        }
    };

    match args.out.as_deref() {
        None | Some("-") => {
            use std::io::Write;
            std::io::stdout().write_all(&document).map_err(|e| CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("write stdout: {e}"),
            })?;
        }
        Some(path) => {
            let path = PathBuf::from(path);
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| CliError {
                        exit: XvnExit::Upstream,
                        source: anyhow::anyhow!("create_dir_all({}): {e}", parent.display()),
                    })?;
                }
            }
            std::fs::write(&path, &document).map_err(|e| CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("write {}: {e}", path.display()),
            })?;
            eprintln!(
                "xvn run export-run → {} ({} bytes)",
                path.display(),
                document.len()
            );
        }
    }

    Ok(())
}

fn map_export_err(e: xvision_observability::ExportError) -> CliError {
    use xvision_observability::ExportError;
    match e {
        ExportError::NotFound(_) => CliError {
            exit: XvnExit::NotFound,
            source: anyhow::anyhow!(e),
        },
        ExportError::Sqlite(_) | ExportError::InvalidTimestamp { .. } | ExportError::InvalidJson { .. } => {
            CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!(e),
            }
        }
    }
}

async fn open_pool(path: &std::path::Path) -> CliResult<SqlitePool> {
    // Read-only so the export verb cannot drift the canonical ledger.
    let url = format!("sqlite://{}?mode=ro", path.display());
    match SqlitePool::connect(&url).await {
        Ok(pool) => Ok(pool),
        Err(e) => {
            if !path.exists() {
                return Err(CliError {
                    exit: XvnExit::NotFound,
                    source: anyhow::anyhow!(
                        "sqlite database not found at {} (set --db or XVN_HOME)",
                        path.display()
                    ),
                });
            }
            Err(CliError {
                exit: XvnExit::Upstream,
                source: anyhow::anyhow!("open sqlite at {}: {e}", path.display()),
            })
        }
    }
}

fn default_xvn_home() -> PathBuf {
    std::env::var("XVN_HOME").map(PathBuf::from).unwrap_or_else(|_| {
        dirs::home_dir()
            .map(|h| h.join(".xvn"))
            .unwrap_or_else(|| PathBuf::from(".xvn"))
    })
}
