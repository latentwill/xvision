//! `xvn risk …` — apply or inspect the deterministic risk layer.
//!
//! - `evaluate`     — feed a `TraderDecision` JSON + `PortfolioState` JSON
//!                    through the risk layer and print the `RiskDecision`.
//! - `show-config`  — dump the effective `risk.toml` + `whitelist.toml` content.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use xvision_core::trading::{PortfolioState, TraderDecision};

use crate::commands::asset::parse_asset;
use crate::commands::home::resolve_xvn_home;

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
        /// Optional asset override — overwrites the decision JSON's `asset`
        /// before evaluation. Useful for sanity-checking a fixture against a
        /// different asset without editing the file.
        #[arg(long, value_parser = parse_asset)]
        asset: Option<xvision_core::AssetSymbol>,
        /// Path to `risk.toml`. Defaults to `$XVN_HOME/config/risk.toml`.
        #[arg(long)]
        risk_config: Option<PathBuf>,
        /// Path to `whitelist.toml`. Defaults to `$XVN_HOME/config/whitelist.toml`.
        #[arg(long)]
        whitelist: Option<PathBuf>,
    },
    /// Print the effective risk + whitelist configuration TOMLs.
    ShowConfig {
        /// Path to `risk.toml`. Defaults to `$XVN_HOME/config/risk.toml`.
        #[arg(long)]
        risk_config: Option<PathBuf>,
        /// Path to `whitelist.toml`. Defaults to `$XVN_HOME/config/whitelist.toml`.
        #[arg(long)]
        whitelist: Option<PathBuf>,
    },
}

pub async fn run(cmd: RiskCmd) -> anyhow::Result<()> {
    let home = resolve_xvn_home(None)?;
    match cmd.action {
        RiskAction::Evaluate {
            decision,
            portfolio,
            asset,
            risk_config,
            whitelist,
        } => {
            let risk_config = risk_config.unwrap_or_else(|| home.join("config/risk.toml"));
            let whitelist = whitelist.unwrap_or_else(|| home.join("config/whitelist.toml"));
            evaluate(decision, portfolio, asset, &risk_config, &whitelist)
        }
        RiskAction::ShowConfig {
            risk_config,
            whitelist,
        } => {
            let risk_config = risk_config.unwrap_or_else(|| home.join("config/risk.toml"));
            let whitelist = whitelist.unwrap_or_else(|| home.join("config/whitelist.toml"));
            show_config(&risk_config, &whitelist)
        }
    }
}

fn evaluate(
    decision_path: PathBuf,
    portfolio_path: PathBuf,
    asset_override: Option<xvision_core::AssetSymbol>,
    risk_path: &Path,
    whitelist_path: &Path,
) -> anyhow::Result<()> {
    let mut decision: TraderDecision = serde_json::from_slice(&std::fs::read(&decision_path)?)?;
    let portfolio: PortfolioState = serde_json::from_slice(&std::fs::read(&portfolio_path)?)?;
    if let Some(asset) = asset_override {
        decision.asset = asset;
    }
    let layer = xvision_risk::RiskLayer::from_config(risk_path, whitelist_path)?;
    let outcome = layer.evaluate(decision, &portfolio);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Regression test: when `--risk-config` / `--whitelist` are omitted,
    /// the resolved defaults must be under `$XVN_HOME/config/`, NOT under cwd.
    ///
    /// Before the fix the handler used clap `default_value = "config/risk.toml"`
    /// (CWD-relative), so the defaults were unaffected by `XVN_HOME` and would
    /// fail with "No such file or directory" when the operator ran the command
    /// from any directory that isn't the xvn home.
    #[test]
    fn show_config_defaults_resolve_under_xvn_home() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_dir = dir.path().join("config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("risk.toml"), "[risk]\n").unwrap();
        fs::write(config_dir.join("whitelist.toml"), "[whitelist]\n").unwrap();

        // Point XVN_HOME at the temp dir; the resolver picks this up.
        std::env::set_var("XVN_HOME", dir.path());

        let home = resolve_xvn_home(None).expect("resolve_xvn_home");
        let risk_default = home.join("config/risk.toml");
        let whitelist_default = home.join("config/whitelist.toml");

        // Both defaults must exist — proving they live under XVN_HOME, not cwd.
        assert!(
            risk_default.exists(),
            "risk default {:?} not found; defaults are still CWD-relative",
            risk_default
        );
        assert!(
            whitelist_default.exists(),
            "whitelist default {:?} not found; defaults are still CWD-relative",
            whitelist_default
        );

        // Calling the handler itself must succeed (reads the files we wrote).
        show_config(&risk_default, &whitelist_default)
            .expect("show_config should succeed with XVN_HOME-relative defaults");

        // Cleanup env so parallel tests aren't affected.
        std::env::remove_var("XVN_HOME");
    }
}
