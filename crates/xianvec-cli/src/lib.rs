//! `xvn` — XIANVEC CLI surface.
//!
//! Subcommands:
//! - `show-metrics` — render a `BacktestResult` JSON's headline numbers.
//! - `show-decision` — pretty-print a cached `TraderDecision` from SQLite.
//! - `run-setup` — run a single setup through Intern → Risk slice.
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
    about = "XIANVEC: multistrategy trading agent backtest harness"
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
    /// Run a single setup through Intern → Risk slice.
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
    /// Manual single-trade smoke test against Alpaca paper. Builds a synthetic
    /// `RiskDecision::Approved` from the args and submits via the executor.
    /// Reads APCA_API_KEY_ID / APCA_API_SECRET_KEY / APCA_API_BASE_URL from env.
    FireTrade {
        /// `buy` (long) or `sell` (short).
        #[arg(long)]
        side: commands::fire_trade::Side,
        /// Position size in basis points of equity (100 bps = 1%). Range: 0–2000.
        #[arg(long)]
        size_bps: u32,
        /// Stop-loss distance from mid as a percent. Range: 0.1–20.0.
        #[arg(long, default_value_t = 1.0)]
        stop_loss_pct: f32,
        /// Take-profit distance from mid as a percent. Range: 0.1–50.0.
        #[arg(long, default_value_t = 2.0)]
        take_profit_pct: f32,
        /// Audit-trail summary string written into the TraderDecision (10–500 chars).
        #[arg(long, default_value = "manual fire-trade smoke from xvn cli")]
        summary: String,
    },
    /// Run an N-arm backtest A/B comparison and emit `BacktestResult` JSON.
    AbCompare {
        /// Path to a JSON file containing a `Vec<MarketSnapshot>`.
        #[arg(long)]
        setups: PathBuf,
        /// Path to a JSON file containing a `Vec<MarketBar>`.
        #[arg(long)]
        bars: PathBuf,
        /// Comma-separated arm specs. Heads:
        /// `trader_arm`, `buy_and_hold`, `always_long`, `always_short`,
        /// `random_direction:seed=<u64>`, `rsi_mean_reversion`,
        /// `ma_crossover:fast=<usize>:slow=<usize>`, `macd_momentum`.
        /// Empty value selects `default_arms()` (trader_arm + buy_and_hold).
        #[arg(long, default_value = "")]
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
        #[arg(long, default_value = "anthropic")]
        intern: String,
        #[arg(long, default_value = "claude-haiku-4-5-20251001")]
        intern_model: String,
        /// Trader OpenAI-compat base URL (e.g. https://api.openai.com/v1
        /// or http://localhost:8080/v1 for llama.cpp / vLLM / Ollama).
        #[arg(long, default_value = "https://api.openai.com/v1")]
        trader_base_url: String,
        /// Trader model id (e.g. `gpt-4o-mini`, `Qwen/Qwen2.5-7B-Instruct`).
        #[arg(long, default_value = "gpt-4o-mini")]
        trader_model: String,
        /// Env var holding the Trader API key. Set to empty for local
        /// endpoints that don't require auth.
        #[arg(long, default_value = "OPENAI_API_KEY")]
        trader_api_key_env: String,
    },
}

impl Cli {
    pub async fn run(self) -> anyhow::Result<()> {
        match self.command {
            Command::ShowMetrics { report } => commands::show_metrics::run(report),
            Command::ShowDecision { setup_id, db } => {
                commands::show_decision::run(setup_id, db).await
            }
            Command::RunSetup {
                snapshot,
                intern,
                model,
            } => commands::run_setup::run(snapshot, intern, model).await,
            Command::Report { input, output } => commands::report::run(input, output),
            Command::FireTrade {
                side,
                size_bps,
                stop_loss_pct,
                take_profit_pct,
                summary,
            } => {
                commands::fire_trade::run(side, size_bps, stop_loss_pct, take_profit_pct, summary)
                    .await
            }
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
                intern,
                intern_model,
                trader_base_url,
                trader_model,
                trader_api_key_env,
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
                    intern,
                    intern_model,
                    trader_base_url,
                    trader_model,
                    trader_api_key_env,
                )
                .await
            }
        }
    }
}
