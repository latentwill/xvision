//! `xvn store …` — explicit operations on the SQLite flight recorder.
//!
//! - `migrate` — open + apply pending migrations.
//! - `stats`   — print row counts per table.
//!
//! Read paths into specific rows live under `xvn show-decision`,
//! `xvn show-briefing`, etc. Arbitrary writes are deliberately not exposed —
//! the harness is the single writer for `briefings`, `decisions`,
//! `risk_outcomes`, and `traces` so the eval substrate stays trustworthy.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use xianvec_core::store::Store;

#[derive(Args, Debug)]
pub struct StoreCmd {
    #[command(subcommand)]
    action: StoreAction,
}

#[derive(Subcommand, Debug)]
enum StoreAction {
    /// Open the database (creating the file if missing) and run pending migrations.
    Migrate {
        #[arg(long, default_value = "data/store.db")]
        db: PathBuf,
    },
    /// Print row counts per table.
    Stats {
        #[arg(long, default_value = "data/store.db")]
        db: PathBuf,
    },
}

pub async fn run(cmd: StoreCmd) -> anyhow::Result<()> {
    match cmd.action {
        StoreAction::Migrate { db } => migrate(db).await,
        StoreAction::Stats { db } => stats(db).await,
    }
}

async fn migrate(db: PathBuf) -> anyhow::Result<()> {
    let url = format!("sqlite://{}", db.display());
    let store = Store::open(&url).await?;
    store.migrate().await?;
    println!("ok ({})", db.display());
    Ok(())
}

async fn stats(db: PathBuf) -> anyhow::Result<()> {
    let url = format!("sqlite://{}", db.display());
    let store = Store::open(&url).await?;
    let counts = store
        .counts(&["cycles", "briefings", "decisions", "risk_outcomes", "traces"])
        .await?;
    println!("XIANVEC store — {}", db.display());
    for (name, n) in counts {
        println!("  {name:<15} {n:>8}");
    }
    Ok(())
}
