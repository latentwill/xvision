//! Shared `--venue` parser plus the `xvn portfolio` / `xvn close-position`
//! commands that read from / write to a live executor.
//!
//! # Real-money guards (`close-position`, Phase 4 + Phase 5)
//! When `--venue byreal` and `BYREAL_NETWORK` resolves to mainnet (the default),
//! `close_position` requires `--i-understand-real-money`. Without that flag it
//! exits with an error before touching the network (Phase 4).
//!
//! Additionally (Phase 5), the global SafetyManager pause-gate is checked: if
//! the operator has paused trading via the dashboard or the CLI kill-switch,
//! `close_position` refuses to proceed even with `--i-understand-real-money`.

use std::path::PathBuf;
use std::str::FromStr;

use xvision_core::AssetSymbol;
use xvision_execution::{
    AlpacaExecutor, ByrealPerpsExecutor, Executor, OrderlyExecutor, SubprocessByrealApi,
};

use crate::commands::asset::parse_asset;
use crate::commands::live_guard::{check_not_paused, require_real_money_ack};

#[derive(Debug, Clone, Copy)]
pub enum Venue {
    Alpaca,
    Orderly,
    Byreal,
}

impl FromStr for Venue {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "alpaca" => Ok(Venue::Alpaca),
            "orderly" => Ok(Venue::Orderly),
            "byreal" => Ok(Venue::Byreal),
            other => Err(format!("unknown venue '{other}'; want alpaca|orderly|byreal")),
        }
    }
}

/// Build an `Executor` for a venue from env. Returns a boxed trait object.
fn executor_from_env(venue: Venue) -> anyhow::Result<Box<dyn Executor>> {
    match venue {
        Venue::Alpaca => Ok(Box::new(AlpacaExecutor::from_env().map_err(|e| {
            anyhow::anyhow!(
                "AlpacaExecutor::from_env() failed: {e} — check APCA_API_KEY_ID, APCA_API_SECRET_KEY, APCA_API_BASE_URL"
            )
        })?)),
        Venue::Orderly => Ok(Box::new(OrderlyExecutor::from_env().map_err(|e| {
            anyhow::anyhow!(
                "OrderlyExecutor::from_env() failed: {e} — check ORDERLY_KEY, ORDERLY_SECRET, ORDERLY_ACCOUNT_ID, ORDERLY_BASE_URL"
            )
        })?)),
        Venue::Byreal => Ok(Box::new(ByrealPerpsExecutor::new(SubprocessByrealApi::from_env().map_err(|e| {
            anyhow::anyhow!(
                "SubprocessByrealApi::from_env() failed: {e} — check BYREAL_PRIVATE_KEY, BYREAL_NETWORK, BYREAL_ACCOUNT"
            )
        })?))),
    }
}

pub async fn portfolio(venue: Venue) -> anyhow::Result<()> {
    let exec = executor_from_env(venue)?;
    let p = exec.portfolio().await?;
    println!("{}", serde_json::to_string_pretty(&p)?);
    Ok(())
}

pub async fn close_position(
    venue: Venue,
    asset_str: String,
    i_understand_real_money: bool,
    xvn_home: PathBuf,
) -> anyhow::Result<()> {
    let byreal_network = std::env::var("BYREAL_NETWORK").ok();
    // Real-money mainnet applies to byreal perps only; Alpaca (paper) and Orderly
    // (testnet) are never real money, so the kill-switch fail-closed-on-missing-DB
    // must not gate them (the pause check still runs when a DB is present).
    let network_is_mainnet = matches!(venue, Venue::Byreal)
        && !byreal_network
            .as_deref()
            .map(|n| n.to_ascii_lowercase().contains("testnet"))
            .unwrap_or(false);

    // Phase 4 guard: refuse Byreal mainnet without the explicit ack flag.
    require_real_money_ack(venue, byreal_network.as_deref(), i_understand_real_money)?;

    // Phase 5 guard: refuse if the global safety kill-switch is active.
    check_not_paused(&xvn_home, network_is_mainnet).await?;

    let asset: AssetSymbol = parse_asset(&asset_str).map_err(anyhow::Error::msg)?;
    let exec = executor_from_env(venue)?;
    let receipt = exec.close_position(asset).await?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}
