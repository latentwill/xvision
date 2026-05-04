//! `xvn` — XIANVEC CLI surface (Phase 9 + 10).
//!
//! Subcommands:
//! - `show-metrics` — render a `BacktestResult` JSON's headline numbers.
//! - `show-decision` — pretty-print a cached `TraderDecision` from SQLite.
//! - `explain-vectors` — print a vector manifest sidecar with highlights.
//! - `run-setup` — run a single setup through Intern → Risk (Trader path
//!    stays in `xianvec-trader`'s `smoke-pipeline` binary).
//! - `report` — render a Markdown report from a `BacktestResult`.
//! - `ab-compare` — run an N-arm backtest A/B over a setups + bars JSON.

pub mod commands;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(
    name = "xvn",
    version,
    about = "XIANVEC: vectors-on vs vectors-off trading agent"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Render a `BacktestResult` JSON's headline numbers per arm.
    ShowMetrics {
        #[arg(long)]
        report: PathBuf,
    },
    /// Pretty-print a cached `TraderDecision` by setup_id (SQLite store).
    ShowDecision {
        #[arg(long)]
        setup_id: Uuid,
        #[arg(long, default_value = "data/store.db")]
        db: PathBuf,
    },
    /// Print the active vector manifest sidecar with highlights.
    ExplainVectors {
        #[arg(long)]
        manifest: PathBuf,
    },
    /// Run a single setup through Intern → Risk slice. Trader path is
    /// in `xianvec-trader`'s smoke-pipeline binary.
    RunSetup {
        /// Path to a serialized `MarketSnapshot` (JSON).
        #[arg(long)]
        snapshot: PathBuf,
        /// Intern provider — "anthropic" or "openai-compat".
        #[arg(long, default_value = "anthropic")]
        intern: String,
        /// Intern model.
        #[arg(long, default_value = "claude-haiku-4-5-20251001")]
        model: String,
    },
    /// Render the headline Markdown report for a backtest run.
    Report {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Run an N-arm backtest A/B comparison and emit `BacktestResult` JSON.
    AbCompare {
        /// Path to a JSON file containing a `Vec<MarketSnapshot>`.
        #[arg(long)]
        setups: PathBuf,
        /// Path to a JSON file containing a `Vec<MarketBar>`.
        #[arg(long)]
        bars: PathBuf,
        /// Comma-separated arm specs: off, on:<npz>:<manifest>:<alpha>,
        /// random:layer=20:dim=5120:alpha=1.0:seed=42,
        /// orthogonal:axis=conviction:path=<npz>:alpha=1.0:seed=42
        #[arg(long, default_value = "off")]
        arms: String,
        /// Output path for the `BacktestResult` JSON.
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 100_000.0)]
        initial_nav_usd: f64,
        #[arg(long, default_value_t = 10)]
        fee_bps: u32,
        #[arg(long, default_value_t = 24)]
        step_hours: u32,
        #[arg(long, default_value_t = 24)]
        horizon_hours: u32,
        #[arg(long, default_value = "BTC")]
        asset: String,
        /// Path to a Qwen3 GGUF.
        #[arg(long)]
        model: PathBuf,
        /// Path to a tokenizer.json.
        #[arg(long)]
        tokenizer: PathBuf,
        #[arg(long, default_value = "anthropic")]
        intern: String,
        #[arg(long, default_value = "claude-haiku-4-5-20251001")]
        intern_model: String,
    },
}

impl Cli {
    pub async fn run(self) -> anyhow::Result<()> {
        match self.command {
            Command::ShowMetrics { report } => commands::show_metrics::run(report),
            Command::ShowDecision { setup_id, db } => {
                commands::show_decision::run(setup_id, db).await
            }
            Command::ExplainVectors { manifest } => commands::explain_vectors::run(manifest),
            Command::RunSetup {
                snapshot,
                intern,
                model,
            } => commands::run_setup::run(snapshot, intern, model).await,
            Command::Report { input, output } => commands::report::run(input, output),
            Command::AbCompare {
                setups,
                bars,
                arms,
                output,
                initial_nav_usd,
                fee_bps,
                step_hours,
                horizon_hours,
                asset,
                model,
                tokenizer,
                intern,
                intern_model,
            } => {
                commands::ab_compare::run(
                    setups,
                    bars,
                    arms,
                    output,
                    initial_nav_usd,
                    fee_bps,
                    step_hours,
                    horizon_hours,
                    asset,
                    model,
                    tokenizer,
                    intern,
                    intern_model,
                )
                .await
            }
        }
    }
}
