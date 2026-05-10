//! `xvn risk …` — apply or inspect the deterministic risk layer.
//!
//! - `evaluate`     — feed a `TraderDecision` JSON + `PortfolioState` JSON
//!                    through the risk layer and print the `RiskDecision`.
//! - `show-config`  — dump the effective `risk.toml` + `whitelist.toml` content.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use xianvec_core::trading::{PortfolioState, TraderDecision};

use crate::commands::asset::parse_asset;

#[derive(Args, Debug)]
pub struct RiskCmd {
    #[command(subcommand)]
    action: RiskAction,
}

#[derive(Subcommand, Debug)]
enum RiskAction {
    /// Apply the risk layer to a serialized `TraderDecision` + `PortfolioState`.
    Evaluate {
        #[arg(long)]
        decision: PathBuf,
        #[arg(long)]
        portfolio: PathBuf,
        /// BTC | ETH | SOL.
        #[arg(long, value_parser = parse_asset)]
        asset: xianvec_core::AssetSymbol,
        /// Path to `risk.toml`. Defaults to `config/risk.toml` under cwd.
        #[arg(long, default_value = "config/risk.toml")]
        risk_config: PathBuf,
        /// Path to `whitelist.toml`. Defaults to `config/whitelist.toml`.
        #[arg(long, default_value = "config/whitelist.toml")]
        whitelist: PathBuf,
    },
    /// Print the effective risk + whitelist configuration TOMLs.
    ShowConfig {
        #[arg(long, default_value = "config/risk.toml")]
        risk_config: PathBuf,
        #[arg(long, default_value = "config/whitelist.toml")]
        whitelist: PathBuf,
    },
}

pub async fn run(cmd: RiskCmd) -> anyhow::Result<()> {
    match cmd.action {
        RiskAction::Evaluate {
            decision,
            portfolio,
            asset,
            risk_config,
            whitelist,
        } => evaluate(decision, portfolio, asset, &risk_config, &whitelist),
        RiskAction::ShowConfig {
            risk_config,
            whitelist,
        } => show_config(&risk_config, &whitelist),
    }
}

fn evaluate(
    decision_path: PathBuf,
    portfolio_path: PathBuf,
    asset: xianvec_core::AssetSymbol,
    risk_path: &Path,
    whitelist_path: &Path,
) -> anyhow::Result<()> {
    let decision: TraderDecision = serde_json::from_slice(&std::fs::read(&decision_path)?)?;
    let portfolio: PortfolioState = serde_json::from_slice(&std::fs::read(&portfolio_path)?)?;
    let layer = xianvec_risk::RiskLayer::from_config(risk_path, whitelist_path)?;
    let outcome = layer.evaluate(decision, &portfolio, asset);
    println!("{}", serde_json::to_string_pretty(&outcome)?);
    Ok(())
}

fn show_config(risk_path: &Path, whitelist_path: &Path) -> anyhow::Result<()> {
    let risk = std::fs::read_to_string(risk_path)?;
    let wl = std::fs::read_to_string(whitelist_path)?;
    println!("# {}", risk_path.display());
    println!("{risk}");
    println!("# {}", whitelist_path.display());
    println!("{wl}");
    Ok(())
}
