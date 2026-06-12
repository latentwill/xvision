//! `xvn run inspect <id>` — build the canonical export deliverables
//! for a finished agent run.
//!
//! Writes `xvn_run.json` (schema `xvn.agent_run.v2`) + `xvn_report.md`
//! into `--out <dir>` (default: cwd). `--out -` writes JSON to stdout,
//! which is the form the autooptimizer ingests. `--format` controls
//! which deliverables get materialized; both are emitted by default
//! because the two are paired everywhere downstream.
//!
//! Idempotent on finished runs — repeated invocations produce
//! identical bytes (see `tests/run_inspect.rs`).

use std::path::PathBuf;

use clap::{Args, ValueEnum};
use sqlx::SqlitePool;

use xvision_observability::{build_export, build_report};

use crate::exit::{CliError, CliResult, XvnExit};

#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Agent run id (e.g. `run_01H…`). Required.
    pub id: String,

    /// Override the xvn home directory (default: `$XVN_HOME` or `~/.xvn`).
    /// Used to locate `xvn.db` if `--db` is not set.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,

    /// Path to the sqlite database that holds the agent_runs tables.
    /// Defaults to `<xvn_home>/xvn.db` to match the engine's
    /// default store location.
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Output directory. Files written: `xvn_run.json`,
    /// `xvn_report.md`. Pass `-` to write JSON to stdout instead.
    #[arg(long)]
    pub out: Option<String>,

    /// Which deliverables to write. `both` is the default because the
    /// two are paired everywhere downstream.
    #[arg(long, value_enum, default_value_t = OutputFormat::Both)]
    pub format: OutputFormat,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "kebab-case")]
pub enum OutputFormat {
    Json,
    Md,
    Both,
}

pub async fn run(args: InspectArgs) -> CliResult<()> {
    let pool = open_pool(args.xvn_home.as_deref(), args.db.as_deref()).await?;

    // Stage 1 (Cline runtime unification, operational-visibility contract
    // item 3): surface the run's trajectory mode in the CLI. Read directly
    // from `agent_runs` (a cheap scalar) and print to stderr so it never
    // contaminates the `--out -` JSON stdout deliverable. Best-effort: a
    // pool that predates migration 039 (no `trajectory_mode` column) or a
    // missing row simply skips the line rather than failing the inspect.
    if let Ok(Some(mode)) =
        sqlx::query_scalar::<_, Option<String>>("SELECT trajectory_mode FROM agent_runs WHERE id = ?")
            .bind(&args.id)
            .fetch_optional(&pool)
            .await
    {
        if let Some(mode) = mode {
            eprintln!("xvn run inspect → trajectory mode: {mode}");
        }
    }

    // Stdout sink for JSON. Markdown is meaningless on stdout (mixed
    // with caller output), so we restrict `-` to JSON.
    if let Some("-") = args.out.as_deref() {
        if !matches!(args.format, OutputFormat::Json) {
            return Err(CliError {
                exit: XvnExit::Usage,
                source: anyhow::anyhow!(
                    "`--out -` requires `--format json` (markdown to stdout would mix with caller output)"
                ),
            });
        }
        let export = build_export(&pool, &args.id).await.map_err(map_export_err)?;
        let bytes = serde_json::to_vec_pretty(&export).map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("serialize xvn_run.json: {e}"),
        })?;
        use std::io::Write;
        std::io::stdout().write_all(&bytes).map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("write stdout: {e}"),
        })?;
        println!();
        return Ok(());
    }

    let out_dir = match args.out.as_deref() {
        Some(path) => PathBuf::from(path),
        None => std::env::current_dir().map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("current_dir: {e}"),
        })?,
    };
    std::fs::create_dir_all(&out_dir).map_err(|e| CliError {
        exit: XvnExit::Upstream,
        source: anyhow::anyhow!("create_dir_all({}): {e}", out_dir.display()),
    })?;

    if matches!(args.format, OutputFormat::Json | OutputFormat::Both) {
        let export = build_export(&pool, &args.id).await.map_err(map_export_err)?;
        let bytes = serde_json::to_vec_pretty(&export).map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("serialize xvn_run.json: {e}"),
        })?;
        let path = out_dir.join("xvn_run.json");
        // Trailing newline keeps the file POSIX-friendly and matches
        // what `xvn eval export` emits.
        let mut buf = bytes;
        buf.push(b'\n');
        std::fs::write(&path, &buf).map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("write {}: {e}", path.display()),
        })?;
        eprintln!("xvn run inspect → {} ({} bytes)", path.display(), buf.len());
    }

    if matches!(args.format, OutputFormat::Md | OutputFormat::Both) {
        let report = build_report(&pool, &args.id).await.map_err(map_export_err)?;
        let path = out_dir.join("xvn_report.md");
        std::fs::write(&path, report.markdown.as_bytes()).map_err(|e| CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!("write {}: {e}", path.display()),
        })?;
        eprintln!(
            "xvn run inspect → {} ({} bytes)",
            path.display(),
            report.markdown.len()
        );
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

async fn open_pool(
    xvn_home: Option<&std::path::Path>,
    db: Option<&std::path::Path>,
) -> CliResult<SqlitePool> {
    let path = match db {
        Some(p) => p.to_path_buf(),
        None => {
            let home = match xvn_home {
                Some(h) => h.to_path_buf(),
                None => default_xvn_home(),
            };
            home.join("xvn.db")
        }
    };

    // Use `mode=ro` so the inspect verb cannot drift the canonical
    // ledger even if a regression elsewhere tries to write through
    // this handle. Falls back to `rwc` if the read-only handle fails
    // to open (the recorder may have created the file with different
    // perms on a previous run).
    let url = format!("sqlite://{}?mode=ro", path.display());
    match SqlitePool::connect(&url).await {
        Ok(pool) => Ok(pool),
        Err(e) => {
            // The file may not exist (run id was wrong, or the
            // operator pointed at a fresh home). Surface a typed
            // not-found rather than the raw sqlx error.
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
