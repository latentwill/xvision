//! `xvn fire-trade` — manual single-trade smoke test against a live venue.
//!
//! Builds a synthetic `RiskDecision::Approved` from CLI args and submits via
//! the venue-specific executor. Used to validate executor wiring without
//! standing up the full Risk → Trader pipeline.
//!
//! # Real-money guard (Phase 4 + Phase 5)
//! When `--venue byreal` and `BYREAL_NETWORK` resolves to mainnet (the default),
//! this command requires `--i-understand-real-money`. Without that flag it
//! exits with an error before touching the network.
//!
//! Additionally (Phase 5), the global SafetyManager pause-gate is checked: if
//! the operator has paused trading via the dashboard or the CLI kill-switch,
//! this command refuses to proceed even with `--i-understand-real-money`.
//!
//! Venues:
//! - `alpaca`  — reads APCA_API_KEY_ID, APCA_API_SECRET_KEY, APCA_API_BASE_URL.
//! - `orderly` — reads ORDERLY_KEY, ORDERLY_SECRET, ORDERLY_ACCOUNT_ID, ORDERLY_BASE_URL.
//! - `byreal`  — reads BYREAL_PRIVATE_KEY, BYREAL_NETWORK, BYREAL_ACCOUNT.
//!
//! Idempotent on retries: every executor uses `cycle_id` as the venue-side
//! client order id so duplicate retries collapse to a single fill.

use std::path::PathBuf;

use anyhow::Result;
use uuid::Uuid;

use xvision_core::{Action, AssetSymbol, Direction, RiskDecision, TraderDecision};
use xvision_execution::{
    AlpacaExecutor, ByrealPerpsExecutor, Executor, OrderlyExecutor, SubprocessByrealApi,
};

use crate::commands::live_guard::{check_not_paused, require_real_money_ack};
use crate::commands::venue::Venue;

#[derive(Debug, Clone, Copy)]
pub enum Side {
    Buy,
    Sell,
}

impl std::str::FromStr for Side {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "buy" | "long" => Ok(Side::Buy),
            "sell" | "short" => Ok(Side::Sell),
            other => Err(format!("unknown side '{other}'; want buy|sell")),
        }
    }
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Side::Buy => f.write_str("buy"),
            Side::Sell => f.write_str("sell"),
        }
    }
}

pub async fn run(
    venue: Venue,
    side: Side,
    size_bps: u32,
    stop_loss_pct: f32,
    take_profit_pct: f32,
    summary: String,
    asset: AssetSymbol,
    i_understand_real_money: bool,
    xvn_home: PathBuf,
) -> Result<()> {
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

    let (action, direction) = match side {
        Side::Buy => (Action::Buy, Direction::Long),
        Side::Sell => (Action::Sell, Direction::Short),
    };

    let cycle_id = Uuid::new_v4();
    let decision = TraderDecision {
        cycle_id,
        action,
        size_bps,
        direction,
        stop_loss_pct,
        take_profit_pct,
        trader_summary: summary,
        asset,
        trailing_stop_pct: None,
        breakeven_trigger_pct: None,
        breakeven_offset_pct: None,
        fade_sl_bars: None,
        fade_sl_start_pct: None,
        fade_sl_end_pct: None,
        max_bars_held: None,
        sl_atr_mult: None,
        tp_atr_mult: None,
        tp1_pct: None,
        tp1_close_fraction: None,
        tp2_pct: None,
    };
    let risk = RiskDecision::Approved {
        decision,
        warnings: Vec::new(),
    };

    println!(
        "→ submitting cycle_id={cycle_id} venue={venue:?} side={side:?} \
         size_bps={size_bps} sl={stop_loss_pct}% tp={take_profit_pct}%",
    );

    let receipt = match venue {
        Venue::Alpaca => {
            let exec = AlpacaExecutor::from_env().map_err(|e| {
                anyhow::anyhow!(
                    "AlpacaExecutor::from_env() failed: {e} — check APCA_API_KEY_ID, APCA_API_SECRET_KEY, APCA_API_BASE_URL"
                )
            })?;
            exec.submit(&risk).await?
        }
        Venue::Orderly => {
            let exec = OrderlyExecutor::from_env().map_err(|e| {
                anyhow::anyhow!(
                    "OrderlyExecutor::from_env() failed: {e} — check ORDERLY_KEY, ORDERLY_SECRET, ORDERLY_ACCOUNT_ID, ORDERLY_BASE_URL"
                )
            })?;
            exec.submit(&risk).await?
        }
        Venue::Byreal => {
            let exec = ByrealPerpsExecutor::new(SubprocessByrealApi::from_env().map_err(|e| {
                anyhow::anyhow!(
                    "SubprocessByrealApi::from_env() failed: {e} — check BYREAL_PRIVATE_KEY, BYREAL_NETWORK, BYREAL_ACCOUNT"
                )
            })?);
            exec.submit(&risk).await?
        }
    };

    println!(
        "\n--- ExecutionReceipt ---\n{}",
        serde_json::to_string_pretty(&receipt)?
    );
    Ok(())
}
