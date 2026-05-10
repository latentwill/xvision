//! Ignored live-network integration test for AlpacaPaperSurface.
//!
//! Run manually:
//!
//! ```bash
//! APCA_API_KEY_ID=xxx APCA_API_SECRET_KEY=yyy \
//!   cargo test -p xvision-execution --test broker_surface_alpaca_live -- --ignored
//! ```
//!
//! With those env vars set the test exercises:
//! - `balance()` — returns the paper account's equity (must be >= 0)
//! - `position("BTC/USD")` — returns the BTC position size (or 0 if none)
//!
//! `submit_order` is intentionally NOT exercised here because doing so would
//! place a real (paper) order on the operator's Alpaca account, which has
//! side effects (open positions, generated client_order_ids visible in the
//! Alpaca dashboard). Operators can manually test submit by writing a
//! one-off script that calls `from_env` + `submit_order` with a tiny size.
//!
//! See: docs/superpowers/plans/2026-05-08-strategy-engine-2c-scheduler-live-exec.md#task-7-brokersurface-trait--dispatch
//!      team/briefings/broker-surface.md

use xvision_execution::broker_surface::{AlpacaPaperSurface, BrokerSurface};

#[tokio::test]
#[ignore = "requires live Alpaca paper credentials (APCA_API_KEY_ID, APCA_API_SECRET_KEY)"]
async fn alpaca_paper_balance_returns_positive() {
    let surface = AlpacaPaperSurface::from_env().expect("from_env must succeed");
    let bal = surface.balance().await.expect("balance must succeed");
    assert!(
        bal >= 0.0,
        "alpaca paper equity must be non-negative, got {bal}"
    );
    eprintln!("alpaca paper equity: ${bal:.2}");
}

#[tokio::test]
#[ignore = "requires live Alpaca paper credentials (APCA_API_KEY_ID, APCA_API_SECRET_KEY)"]
async fn alpaca_paper_position_btc_returns_finite() {
    let surface = AlpacaPaperSurface::from_env().expect("from_env must succeed");
    let qty = surface
        .position("BTC/USD")
        .await
        .expect("position must succeed");
    assert!(qty.is_finite(), "position must be finite, got {qty}");
    eprintln!("alpaca paper BTC/USD position: {qty}");
}
