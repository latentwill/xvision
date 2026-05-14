//! `xvn migrate` — apply pending migrations + seed (or report what would
//! happen with `--dry-run`).
//!
//! Migrations and the canonical seed are also applied on every
//! `ApiContext::open`, so calling `xvn migrate` after any other `xvn`
//! command is effectively a no-op. This subcommand exists for operators
//! who want an explicit, scriptable "make sure xvn_home is initialized"
//! step plus a `--dry-run` inspection mode.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use sqlx::SqlitePool;
use xvision_engine::api::{Actor, ApiContext};

use crate::exit::{CliResult, ResultExt, XvnExit};

/// `xvn migrate` — apply migrations + seed, or report pending state in
/// `--dry-run` mode.
#[derive(Args, Debug)]
pub struct MigrateCmd {
    /// Report pending migrations and seed deltas without mutating.
    #[arg(long)]
    pub dry_run: bool,

    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
}

pub async fn run(cmd: MigrateCmd) -> CliResult<()> {
    let xvn_home =
        crate::commands::home::resolve_xvn_home(cmd.xvn_home).exit_with(XvnExit::Usage)?;

    if cmd.dry_run {
        run_dry(xvn_home).await.exit_with(XvnExit::Upstream)
    } else {
        run_apply(xvn_home).await.exit_with(XvnExit::Upstream)
    }
}

/// Dry-run: inspect state without mutating anything.
async fn run_dry(xvn_home: PathBuf) -> Result<()> {
    let db_path = xvn_home.join("xvn.db");

    if !db_path.exists() {
        // No DB yet — everything is pending.
        println!("pending: all 7 migrations will be applied on first open");
        println!();
        println!("seed plan (would run on first open):");
        println!("  + seed canonical scenarios (0/4 present)");
        println!();
        println!("would NOT mutate — pass without --dry-run to apply");
        return Ok(());
    }

    // DB exists — open read-only to inspect state.
    let url = format!("sqlite://{}?mode=ro", db_path.display());
    let pool = SqlitePool::connect(&url)
        .await
        .context("open xvn.db read-only")?;

    // Check which expected tables exist.
    let table_rows: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='table' AND name IN \
         ('api_audit','eval_runs','chat_sessions','search_index',\
          'bars_cache','scenarios')",
    )
    .fetch_all(&pool)
    .await
    .context("query sqlite_master")?;

    let present_tables: Vec<String> = table_rows.into_iter().map(|(n,)| n).collect();
    let expected_unique: &[&str] = &[
        "api_audit",
        "eval_runs",
        "chat_sessions",
        "search_index",
        "bars_cache",
        "scenarios",
    ];
    let missing_tables: Vec<&&str> = expected_unique
        .iter()
        .filter(|t| !present_tables.iter().any(|p| p == **t))
        .collect();

    if missing_tables.is_empty() {
        println!("migrations: all applied (idempotent on open)");
    } else {
        println!(
            "migrations: {} of {} core tables present; missing: {}",
            present_tables.len(),
            expected_unique.len(),
            missing_tables
                .iter()
                .map(|t| **t)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Check canonical scenario seed.
    let scenario_tables_present = present_tables.iter().any(|p| p == "scenarios");
    if scenario_tables_present {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM scenarios WHERE source = 'canonical'")
                .fetch_one(&pool)
                .await
                .context("count canonical scenarios")?;
        let n = count.0;
        if n < 4 {
            println!("seed: + seed canonical scenarios (currently {n}/4)");
        } else {
            println!("seed: canonical scenarios present ({n}/4)");
        }
    } else {
        println!("seed: + seed canonical scenarios (0/4 — table missing)");
    }

    let legacy_default = xvn_home
        .join("strategies")
        .join(["bun", "dle", "-canonical", "-defaults", ".json"].concat());
    if legacy_default.exists() {
        println!("cleanup: remove legacy default strategy file");
    }

    println!();
    println!("would NOT mutate — pass without --dry-run to apply");

    pool.close().await;
    Ok(())
}

/// Apply mode: open ApiContext (runs all migrations + seed) then report done.
async fn run_apply(xvn_home: PathBuf) -> Result<()> {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());

    ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .context("open ApiContext")?;

    println!("migrations applied.");
    Ok(())
}
