//! Unit tests for [`xvision_engine::eval::executor::RealBrokerFills`].
//!
//! Uses the existing [`xvision_execution::broker_surface::MockBrokerSurface`]
//! plus a scripted error mock to pin:
//!   - market-buy translation
//!   - market-sell translation
//!   - no-op handling (`action == "hold"`)
//!   - broker-error classification

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use xvision_execution::broker_surface::{
    BrokerSurface, MockBrokerSurface, OrderConfirmation, OrderRequest, Side,
};

use xvision_engine::eval::executor::traits::{FillRequest, FillSink};
use xvision_engine::eval::executor::RealBrokerFills;
use xvision_engine::eval::orders::OrderState;
use xvision_engine::eval::scenario::{FeeSource, SlippageModel};

fn ts() -> DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000, 0).unwrap()
}

fn req(action: &str, pos: f64, equity: f64, risk_pct: f64, next_open: f64) -> FillRequest {
    FillRequest {
        pos,
        entry: 0.0,
        action: action.into(),
        next_open,
        bar_volume: 1_000.0,
        slip_bps: 0.0,
        spread_bps: 0.0,
        taker_bps: 10.0,
        maker_bps: 5.0,
        equity,
        risk_pct,
        slippage_model: SlippageModel::None,
        fee_source: FeeSource::Default,
        asset: "BTC/USD".into(),
        bar_ts: ts(),
        bar_open: next_open,
        bar_high: next_open * 1.01,
        bar_low: next_open * 0.99,
    }
}

#[tokio::test]
async fn long_open_translates_to_market_buy_with_risk_sized_quantity() {
    let mock = Arc::new(MockBrokerSurface::new(100_000.0).with_fill_price(50_000.0));
    let mut sink = RealBrokerFills::new(mock.clone());

    let equity = 100_000.0;
    let risk_pct = 0.02;
    let next_open = 50_000.0;
    let record = sink
        .submit(req("long_open", 0.0, equity, risk_pct, next_open))
        .await;

    let submitted = mock.submitted();
    assert_eq!(submitted.len(), 1, "broker must see exactly one order");
    let order = &submitted[0];
    assert!(matches!(order.side, Side::Buy));
    // Expected size = (equity * risk_pct) / next_open = 2000 / 50_000 = 0.04
    assert!(
        (order.size - 0.04).abs() < 1e-9,
        "expected size ~0.04, got {}",
        order.size
    );
    // Successful fill must surface as OrderState::Filled with a fill_price.
    assert_eq!(record.order_state, Some(OrderState::Filled));
    assert!(record.fill_price.is_some());
}

#[tokio::test]
async fn short_open_translates_to_market_sell() {
    let mock = Arc::new(MockBrokerSurface::new(100_000.0).with_fill_price(50_000.0));
    let mut sink = RealBrokerFills::new(mock.clone());

    let record = sink
        .submit(req("short_open", 0.0, 100_000.0, 0.01, 50_000.0))
        .await;

    let submitted = mock.submitted();
    assert_eq!(submitted.len(), 1);
    assert!(matches!(submitted[0].side, Side::Sell));
    assert_eq!(record.order_state, Some(OrderState::Filled));
}

#[tokio::test]
async fn hold_action_short_circuits_without_calling_broker() {
    let mock = Arc::new(MockBrokerSurface::new(100_000.0));
    let mut sink = RealBrokerFills::new(mock.clone());

    let record = sink.submit(req("hold", 0.0, 100_000.0, 0.01, 50_000.0)).await;

    assert!(mock.submitted().is_empty(), "broker must NOT see hold actions");
    // No-op record: order_state None, fill_price None.
    assert_eq!(record.order_state, None);
    assert!(record.fill_price.is_none());
    assert!(record.fill_size.is_none());
}

#[tokio::test]
async fn matching_direction_long_open_while_already_long_is_noop() {
    let mock = Arc::new(MockBrokerSurface::new(100_000.0));
    let mut sink = RealBrokerFills::new(mock.clone());

    let record = sink
        .submit(req("long_open", 0.04, 100_000.0, 0.02, 50_000.0))
        .await;

    assert!(mock.submitted().is_empty(), "already-long must not pile on");
    assert_eq!(record.order_state, None);
}

/// Scripted broker that returns a configured error message verbatim
/// on `submit_order`. Used to pin the classifier wiring.
struct ErrorBroker {
    message: String,
}

#[async_trait]
impl BrokerSurface for ErrorBroker {
    async fn submit_order(&self, _req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        Err(anyhow::anyhow!("{}", self.message))
    }
    async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
        Ok(0.0)
    }
    async fn balance(&self) -> anyhow::Result<f64> {
        Ok(0.0)
    }
}

#[tokio::test]
async fn broker_rejection_maps_to_rejected_order_state() {
    let mock = Arc::new(ErrorBroker {
        message: "alpaca create_order: insufficient buying power for this order".into(),
    });
    let mut sink = RealBrokerFills::new(mock);

    let record = sink
        .submit(req("long_open", 0.0, 100_000.0, 0.02, 50_000.0))
        .await;

    // The trait is infallible — the broker error surfaces as a
    // Rejected no-fill record.
    assert_eq!(record.order_state, Some(OrderState::Rejected));
    assert!(record.fill_price.is_none());
    assert!(record.fill_size.is_none());
    assert_eq!(record.realized_pnl, 0.0);
}
