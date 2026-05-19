//! Ignored live-network integration test for AlpacaPaperSurface.
//!
//! Run read-only live checks manually:
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
//! Run the submit_order regression separately because it places a real
//! paper order on the operator's Alpaca account:
//!
//! ```bash
//! APCA_API_KEY_ID=xxx APCA_API_SECRET_KEY=yyy XVN_ALPACA_LIVE_SUBMIT=1 \
//!   cargo test -p xvision-execution --test broker_surface_alpaca_live \
//!   alpaca_paper_crypto_submit_simple_market -- --ignored
//! ```
//!
//! See: docs/superpowers/plans/2026-05-08-strategy-engine-2c-scheduler-live-exec.md#task-7-brokersurface-trait--dispatch
//!      team/briefings/broker-surface.md

use std::str::FromStr;

use xvision_core::AssetSymbol;
use xvision_execution::broker_surface::{AlpacaPaperSurface, BrokerSurface};

#[test]
fn alpaca_paper_surface_accepts_unlocked_crypto_symbols() {
    assert_eq!(AssetSymbol::from_str("ETH").unwrap().as_alpaca_pair(), "ETH/USD");
    assert_eq!(
        AssetSymbol::from_str("SOL/USD").unwrap().as_alpaca_pair(),
        "SOL/USD"
    );
}

#[test]
fn alpaca_paper_surface_rejects_unsupported_crypto_symbols() {
    let err = AssetSymbol::from_str("XRP").unwrap_err();
    assert!(err.contains("not in the Alpaca crypto whitelist"));
}

#[tokio::test]
#[ignore = "requires live Alpaca paper credentials (APCA_API_KEY_ID, APCA_API_SECRET_KEY)"]
async fn alpaca_paper_balance_returns_positive() {
    let surface = AlpacaPaperSurface::from_env().expect("from_env must succeed");
    let bal = surface.balance().await.expect("balance must succeed");
    assert!(bal >= 0.0, "alpaca paper equity must be non-negative, got {bal}");
    eprintln!("alpaca paper equity: ${bal:.2}");
}

#[tokio::test]
#[ignore = "requires live Alpaca paper credentials (APCA_API_KEY_ID, APCA_API_SECRET_KEY)"]
async fn alpaca_paper_position_btc_returns_finite() {
    let surface = AlpacaPaperSurface::from_env().expect("from_env must succeed");
    let qty = surface.position("BTC/USD").await.expect("position must succeed");
    assert!(qty.is_finite(), "position must be finite, got {qty}");
    eprintln!("alpaca paper BTC/USD position: {qty}");
}

/// Operator-run regression for the crypto bracket-omission fix. Submits a
/// minimal BTC/USD buy with TP/SL pcts on the `OrderRequest` and asserts
/// Alpaca paper accepts it (the surface drops the bracket legs before
/// submission). Run once after a deploy to verify the fix end-to-end.
///
/// `--ignored` and double-gated by an env-var opt-in so the test is
/// inert by default — it places a real paper order on the operator's
/// account.
#[tokio::test]
#[ignore = "requires live Alpaca paper credentials and places a real (paper) order; opt in with XVN_ALPACA_LIVE_SUBMIT=1"]
async fn alpaca_paper_crypto_submit_simple_market() {
    use xvision_execution::broker_surface::{OrderRequest, Side};

    if std::env::var("XVN_ALPACA_LIVE_SUBMIT").ok().as_deref() != Some("1") {
        eprintln!("skipping: set XVN_ALPACA_LIVE_SUBMIT=1 to opt in");
        return;
    }

    let surface = AlpacaPaperSurface::from_env().expect("from_env must succeed");

    let key = format!(
        "xvn-live-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );

    let req = OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Buy,
        size: 0.0001,
        reference_price_usd: 70_000.0,
        stop_loss_pct: Some(2.0),
        take_profit_pct: Some(5.0),
        idempotency_key: key.clone(),
    };

    let conf = surface
        .submit_order(req)
        .await
        .expect("crypto buy with bracket pcts must succeed (bracket legs are dropped)");
    eprintln!(
        "alpaca paper crypto submit_order ok: broker_order_id={} fill_size={} fill_price={:?}",
        conf.broker_order_id, conf.fill_size, conf.fill_price
    );
    assert!(
        !conf.broker_order_id.is_empty(),
        "expected a non-empty broker_order_id"
    );
}
