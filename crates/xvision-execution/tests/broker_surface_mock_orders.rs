//! Regression tests for MockBrokerSurface order recording.
//!
//! Pins three invariants of the in-memory mock that downstream crates
//! (eval engine, wizard) rely on:
//!   1. A market buy is recorded in submitted() with correct side/asset.
//!   2. A sell after a buy produces a reduced (negative delta) position.
//!   3. submitted() is empty when no orders have been placed (the "hold"
//!      path — the upstream executor short-circuits before calling
//!      submit_order; this test verifies the mock's side of that contract).
//!      The actual hold short-circuit logic is pinned in
//!      crates/xvision-engine/tests/eval_executor_live_real_broker_fills.rs.

use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface, OrderRequest, Side};

fn btc_buy(size: f64) -> OrderRequest {
    OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Buy,
        size,
        reference_price_usd: 50_000.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: format!("buy-{size}"),
    }
}

fn btc_sell(size: f64) -> OrderRequest {
    OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Sell,
        size,
        reference_price_usd: 51_000.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: format!("sell-{size}"),
    }
}

#[tokio::test]
async fn mock_broker_market_buy_records_submission() {
    let mock = MockBrokerSurface::new(100_000.0);

    mock.submit_order(btc_buy(0.05)).await.unwrap();

    let submitted = mock.submitted();
    assert_eq!(submitted.len(), 1, "one buy must produce exactly one submission");
    assert_eq!(submitted[0].asset, "BTC/USD");
    assert!(matches!(submitted[0].side, Side::Buy));
}

#[tokio::test]
async fn mock_broker_market_sell_reduces_position() {
    let mock = MockBrokerSurface::new(100_000.0);

    mock.submit_order(btc_buy(0.3)).await.unwrap();
    mock.submit_order(btc_sell(0.1)).await.unwrap();

    let pos = mock.position("BTC/USD").await.unwrap();
    assert!(
        pos < 0.3,
        "position after sell must be less than the initial long size; got {pos}"
    );
    assert!(
        pos >= 0.0,
        "position must be non-negative after a partial close; got {pos}"
    );
}

#[tokio::test]
async fn mock_broker_hold_submits_nothing() {
    // A "hold" decision causes the upstream executor to short-circuit
    // without calling submit_order. This test verifies the mock's contract:
    // submitted() must be empty when nothing has been placed.
    let mock = MockBrokerSurface::new(100_000.0);

    assert!(
        mock.submitted().is_empty(),
        "no submit_order calls → submitted() must be empty"
    );
    assert_eq!(
        mock.position("BTC/USD").await.unwrap(),
        0.0,
        "position must be zero with no orders"
    );
}
