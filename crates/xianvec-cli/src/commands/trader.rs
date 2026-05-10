//! `xvn trader …` — run Stage 2 in isolation.
//!
//! - `run`     — call the configured OpenAI-compat backend, print `TraderDecision` JSON.
//! - `preview` — render the trader prompt without calling the backend.
//!
//! Inputs:
//! - `--briefing  path-to-InternBriefing.json`
//! - `--portfolio path-to-PortfolioState.json` (omit on `preview` to use a flat
//!   $100k portfolio).

use std::path::PathBuf;

use clap::{Args, Subcommand};
use xianvec_core::trading::{InternBriefing, PortfolioState};
use xianvec_trader::{
    preview_prompt, run_trader, OpenAiCompatBackend, TraderBackend, TraderParams,
};

#[derive(Args, Debug)]
pub struct TraderCmd {
    #[command(subcommand)]
    action: TraderAction,
}

#[derive(Subcommand, Debug)]
enum TraderAction {
    /// Render the trader prompt for a briefing + portfolio without calling
    /// any backend.
    Preview {
        #[arg(long)]
        briefing: PathBuf,
        /// Optional: path to a serialized `PortfolioState` JSON. Defaults to a
        /// flat $100k portfolio with zero open positions.
        #[arg(long)]
        portfolio: Option<PathBuf>,
    },
    /// Call the OpenAI-compatible Trader backend and print the
    /// `TraderDecision` as JSON. Reads `--api-key-env` from the environment
    /// (set the value to the empty string for endpoints that don't require auth).
    Run {
        #[arg(long)]
        briefing: PathBuf,
        #[arg(long)]
        portfolio: PathBuf,
        /// OpenAI-compat base URL (e.g. https://api.openai.com/v1 or a local
        /// llama.cpp / vLLM / Ollama endpoint).
        #[arg(long, default_value = "https://api.openai.com/v1")]
        base_url: String,
        #[arg(long, default_value = "gpt-4o-mini")]
        model: String,
        /// Env var holding the API key. Use the empty string for unauthenticated
        /// local endpoints.
        #[arg(long, default_value = "OPENAI_API_KEY")]
        api_key_env: String,
        /// Sampling temperature. v1 mandates 0.0 for backtest pairing.
        #[arg(long, default_value_t = 0.0)]
        temperature: f64,
    },
}

pub async fn run(cmd: TraderCmd) -> anyhow::Result<()> {
    match cmd.action {
        TraderAction::Preview {
            briefing,
            portfolio,
        } => preview(briefing, portfolio).await,
        TraderAction::Run {
            briefing,
            portfolio,
            base_url,
            model,
            api_key_env,
            temperature,
        } => run_one(briefing, portfolio, base_url, model, api_key_env, temperature).await,
    }
}

fn flat_portfolio() -> PortfolioState {
    PortfolioState {
        equity_usd: 100_000.0,
        realized_pnl_today_usd: 0.0,
        day_index: 0,
        open_positions: Default::default(),
        as_of: chrono::Utc::now(),
    }
}

async fn preview(briefing_path: PathBuf, portfolio_path: Option<PathBuf>) -> anyhow::Result<()> {
    let briefing: InternBriefing = serde_json::from_slice(&std::fs::read(&briefing_path)?)?;
    let portfolio = match portfolio_path {
        Some(p) => serde_json::from_slice::<PortfolioState>(&std::fs::read(&p)?)?,
        None => flat_portfolio(),
    };
    let params = TraderParams::default();
    let prompt = preview_prompt(&briefing, &portfolio, &params);
    println!("{prompt}");
    Ok(())
}

async fn run_one(
    briefing_path: PathBuf,
    portfolio_path: PathBuf,
    base_url: String,
    model: String,
    api_key_env: String,
    temperature: f64,
) -> anyhow::Result<()> {
    let briefing: InternBriefing = serde_json::from_slice(&std::fs::read(&briefing_path)?)?;
    let portfolio: PortfolioState = serde_json::from_slice(&std::fs::read(&portfolio_path)?)?;
    let backend = OpenAiCompatBackend::from_env(&base_url, &model, &api_key_env)?;
    let params = TraderParams {
        temperature,
        ..TraderParams::default()
    };
    let decision = run_trader(&backend as &dyn TraderBackend, &briefing, &portfolio, &params).await?;
    println!("{}", serde_json::to_string_pretty(&decision)?);
    Ok(())
}
