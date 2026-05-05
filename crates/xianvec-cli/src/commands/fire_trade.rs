//! `xvn fire-trade` — manual single-trade smoke test against Alpaca paper.
//!
//! Builds a synthetic `RiskDecision::Approved` from CLI args and submits via
//! `AlpacaExecutor::from_env()`. Used to validate the executor wiring without
//! standing up the full Intern → Risk → Trader pipeline.
//!
//! Reads APCA_API_KEY_ID, APCA_API_SECRET_KEY, APCA_API_BASE_URL from the env.
//! Defaults: BTC/USD, side required, size in basis points of equity (100 bps = 1%).
//!
//! # Example
//! ```text
//! APCA_API_KEY_ID=PK... APCA_API_SECRET_KEY=... \
//!   APCA_API_BASE_URL=https://paper-api.alpaca.markets \
//!   xvn fire-trade --side buy --size-bps 5
//! ```
//!
//! Idempotent on retries: AlpacaExecutor sets client_order_id = setup_id.

use anyhow::{Context, Result};
use uuid::Uuid;

use xianvec_core::{Action, Direction, RiskDecision, TraderDecision};
use xianvec_execution::{AlpacaExecutor, Executor};

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
    side: Side,
    size_bps: u32,
    stop_loss_pct: f32,
    take_profit_pct: f32,
    summary: String,
) -> Result<()> {
    let (action, direction) = match side {
        Side::Buy => (Action::Buy, Direction::Long),
        Side::Sell => (Action::Sell, Direction::Short),
    };

    let setup_id = Uuid::new_v4();
    let decision = TraderDecision {
        setup_id,
        action,
        size_bps,
        direction,
        stop_loss_pct,
        take_profit_pct,
        trader_summary: summary,
    };
    let risk = RiskDecision::Approved { decision };

    println!(
        "→ submitting setup_id={setup_id} side={side:?} size_bps={size_bps} \
         sl={stop_loss_pct}% tp={take_profit_pct}% via Alpaca paper",
    );

    let executor = AlpacaExecutor::from_env().context(
        "AlpacaExecutor::from_env() failed — check APCA_API_KEY_ID, \
         APCA_API_SECRET_KEY, APCA_API_BASE_URL",
    )?;

    let receipt = executor
        .submit(&risk)
        .await
        .context("submit to Alpaca failed")?;

    // Pretty-print the receipt as JSON so the operator can see venue_order_id,
    // filled size/price, fees, timestamps without parsing text.
    let json = serde_json::to_string_pretty(&receipt)
        .context("serializing ExecutionReceipt to JSON")?;
    println!("\n--- ExecutionReceipt ---\n{json}");

    Ok(())
}
