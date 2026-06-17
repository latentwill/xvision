//! `xvn spot` — gated one-shot Solana-spot swap via byreal-cli.
//!
//! Default is a no-funds `--dry-run` preview. `--i-understand-real-money`
//! flips to a real `--confirm` swap, but ONLY after the global kill-switch
//! (`check_not_paused`) passes. Symbol resolves to a mint via the curated
//! `byreal_spot_assets.toml` under `xvn_home`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use xvision_core::config::{load_spot_assets, spot_assets_path};
use xvision_execution::broker_surface::{OrderRequest, Side};
use xvision_execution::{
    BrokerSurface, ByrealSpotApi, ByrealSpotMode, ByrealSpotSurface, SubprocessByrealSpotApi,
};

use crate::commands::live_guard::check_not_paused;

/// Map the ack flag to a swap mode. Pure + unit-tested.
fn mode_for(i_understand_real_money: bool) -> ByrealSpotMode {
    if i_understand_real_money {
        ByrealSpotMode::Live
    } else {
        ByrealSpotMode::Preview
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SpotSide {
    Buy,
    Sell,
}

impl std::str::FromStr for SpotSide {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "buy" => Ok(SpotSide::Buy),
            "sell" => Ok(SpotSide::Sell),
            other => Err(format!("unknown side '{other}'; want buy|sell")),
        }
    }
}

/// `xvn spot --side buy|sell --symbol <ticker> --amount <usd> [--slippage <bps>]
///   [--i-understand-real-money] [--xvn-home <path>]`.
pub async fn run(
    side: SpotSide,
    symbol: String,
    amount_usd: f64,
    slippage_bps: u32,
    i_understand_real_money: bool,
    xvn_home: PathBuf,
) -> Result<()> {
    let mode = mode_for(i_understand_real_money);

    // Kill-switch: gate only a real (Live) swap; a dry-run moves no funds.
    // Live spot is real money → fail-closed if the DB is missing.
    if mode == ByrealSpotMode::Live {
        check_not_paused(&xvn_home, true).await?;
    }

    let cfg_path = spot_assets_path(&xvn_home);
    let assets = load_spot_assets(&cfg_path)
        .with_context(|| format!("load curated spot set at {}", cfg_path.display()))?;
    let entry = assets
        .resolve(&symbol)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "'{symbol}' is not in the curated spot set ({})",
                cfg_path.display()
            )
        })?
        .clone();

    let api = SubprocessByrealSpotApi::from_env();
    // Live token price → base size = USD / price.
    let price = api
        .token_price(&entry.mint)
        .await
        .map_err(|e| anyhow::anyhow!("byreal_spot price for {symbol}: {e}"))?;
    anyhow::ensure!(
        price > 0.0,
        "byreal_spot returned non-positive price for {symbol}"
    );
    let size = amount_usd / price;

    let surface = ByrealSpotSurface::new(api, assets)
        .with_mode(mode)
        .with_slippage_bps(slippage_bps);

    let order = OrderRequest {
        asset: symbol.clone(),
        side: match side {
            SpotSide::Buy => Side::Buy,
            SpotSide::Sell => Side::Sell,
        },
        size,
        reference_price_usd: price,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: format!("xvn-spot-{symbol}"),
    };

    let label = if mode == ByrealSpotMode::Live {
        "LIVE (real funds)"
    } else {
        "preview (dry-run)"
    };
    println!(
        "→ {label}: {side:?} {symbol} ~${amount_usd} ({size:.6} @ ${price:.4}, slippage {slippage_bps}bps)"
    );

    let conf = surface.submit_order(order).await?;
    println!("\n--- swap result ---\n{}", serde_json::to_string_pretty(&conf)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_ack_is_preview_ack_is_live() {
        assert_eq!(mode_for(false), ByrealSpotMode::Preview);
        assert_eq!(mode_for(true), ByrealSpotMode::Live);
    }
}
