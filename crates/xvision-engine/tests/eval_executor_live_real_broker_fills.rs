//! Unit tests for [`xvision_engine::eval::executor::RealBrokerFills`].
//!
//! Uses the existing [`xvision_execution::broker_surface::MockBrokerSurface`]
//! plus a scripted error mock to pin:
//!   - market-buy translation
//!   - market-sell translation
//!   - no-op handling (`action == "hold"`)
//!   - broker-error classification

mod support;

use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use tracing_subscriber::prelude::*;
use xvision_execution::broker_surface::{
    BrokerPosition, BrokerSurface, OrderConfirmation, OrderRequest, Side,
};

use xvision_engine::eval::executor::traits::{FillRequest, FillSink};
use xvision_engine::eval::executor::RealBrokerFills;
use xvision_engine::eval::orders::OrderState;
use xvision_engine::eval::scenario::{FeeSource, SlippageModel};
use xvision_engine::safety::{AuthContext, SafetyGate, SafetyManager, VenueLabel};

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
        bar_close: next_open,
        decision_to_fill_ms: 0,
        bar_duration_ms: 3_600_000,
    }
}

struct RecordingBroker {
    submitted: Mutex<Vec<OrderRequest>>,
    fill_price: f64,
    fill_size: Option<f64>,
    fee: Option<f64>,
    balances: Mutex<Vec<f64>>,
}

impl RecordingBroker {
    fn new(fill_price: f64) -> Arc<Self> {
        Arc::new(Self {
            submitted: Mutex::new(Vec::new()),
            fill_price,
            fill_size: None,
            fee: None,
            balances: Mutex::new(vec![0.0]),
        })
    }

    fn with_fill_size(fill_price: f64, fill_size: f64) -> Arc<Self> {
        Arc::new(Self {
            submitted: Mutex::new(Vec::new()),
            fill_price,
            fill_size: Some(fill_size),
            fee: None,
            balances: Mutex::new(vec![0.0]),
        })
    }

    fn with_balances(fill_price: f64, balances: Vec<f64>) -> Arc<Self> {
        Arc::new(Self {
            submitted: Mutex::new(Vec::new()),
            fill_price,
            fill_size: None,
            fee: None,
            balances: Mutex::new(balances),
        })
    }

    fn submitted(&self) -> Vec<OrderRequest> {
        self.submitted.lock().unwrap().clone()
    }
}

#[async_trait]
impl BrokerSurface for RecordingBroker {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        self.submitted.lock().unwrap().push(req.clone());
        Ok(OrderConfirmation {
            broker_order_id: format!("recorded-{}", req.idempotency_key),
            fill_price: Some(self.fill_price),
            fill_size: self.fill_size.unwrap_or(req.size),
            fee: self.fee,
        })
    }

    async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
        Ok(0.0)
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        let mut balances = self.balances.lock().unwrap();
        if balances.len() > 1 {
            Ok(balances.remove(0))
        } else {
            Ok(*balances.first().unwrap_or(&0.0))
        }
    }

    async fn open_positions(&self, _assets: &[String]) -> anyhow::Result<Vec<BrokerPosition>> {
        Ok(Vec::new())
    }

    async fn cancel_open_orders(&self, _asset: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn long_open_translates_to_market_buy_with_risk_sized_quantity() {
    let mock = RecordingBroker::new(50_123.0);
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
    assert_eq!(record.fill_price, Some(50_123.0));
    assert_eq!(record.fill_size, Some(0.04));
}

#[tokio::test]
async fn account_equity_sizing_reads_broker_balance_before_sizing_open() {
    let mock = RecordingBroker::with_balances(50_000.0, vec![120_000.0]);
    let mut sink = RealBrokerFills::new(mock.clone()).with_account_equity_sizing(true);

    let record = sink
        .submit(req("long_open", 0.0, 100_000.0, 0.02, 50_000.0))
        .await;

    let submitted = mock.submitted();
    assert_eq!(submitted.len(), 1, "broker must see the account-sized order");
    assert!((submitted[0].size - 0.048).abs() < 1e-9);
    assert_eq!(record.order_state, Some(OrderState::Filled));
}

#[tokio::test]
async fn short_open_translates_to_market_sell() {
    let mock = RecordingBroker::new(50_000.0);
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
    let mock = RecordingBroker::new(50_000.0);
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
    let mock = RecordingBroker::new(50_000.0);
    let mut sink = RealBrokerFills::new(mock.clone());

    let record = sink
        .submit(req("long_open", 0.04, 100_000.0, 0.02, 50_000.0))
        .await;

    assert!(mock.submitted().is_empty(), "already-long must not pile on");
    assert_eq!(record.order_state, None);
}

#[tokio::test]
async fn short_open_from_long_submits_close_plus_new_short_quantity() {
    let mock = RecordingBroker::with_fill_size(49_500.0, 0.06);
    let mut sink = RealBrokerFills::new(mock.clone());
    let mut request = req("short_open", 0.04, 100_000.0, 0.01, 50_000.0);
    request.entry = 48_000.0;

    let record = sink.submit(request).await;

    let submitted = mock.submitted();
    assert_eq!(submitted.len(), 1);
    assert!(matches!(submitted[0].side, Side::Sell));
    assert!(
        (submitted[0].size - 0.06).abs() < 1e-9,
        "reversal order must close 0.04 long and open 0.02 short, got {}",
        submitted[0].size
    );
    assert!((record.new_pos - -0.02).abs() < 1e-9);
    assert_eq!(record.new_entry, 49_500.0);
    assert_eq!(record.fill_size, Some(0.06));
    assert_eq!(record.order_state, Some(OrderState::Filled));
}

#[tokio::test]
async fn flat_from_long_submits_market_sell_for_abs_position() {
    let mock = RecordingBroker::new(51_000.0);
    let mut sink = RealBrokerFills::new(mock.clone());

    let record = sink.submit(req("flat", 0.04, 100_000.0, 0.02, 50_000.0)).await;

    let submitted = mock.submitted();
    assert_eq!(submitted.len(), 1, "flat close must submit exactly one order");
    assert!(matches!(submitted[0].side, Side::Sell));
    assert!((submitted[0].size - 0.04).abs() < 1e-9);
    assert_eq!(record.new_pos, 0.0);
    assert_eq!(record.new_entry, 0.0);
    assert_eq!(record.fill_size, Some(0.04));
    assert_eq!(record.order_state, Some(OrderState::Filled));
}

#[tokio::test]
async fn flat_from_short_submits_market_buy_for_abs_position() {
    let mock = RecordingBroker::new(49_000.0);
    let mut sink = RealBrokerFills::new(mock.clone());

    let record = sink.submit(req("flat", -0.03, 100_000.0, 0.02, 50_000.0)).await;

    let submitted = mock.submitted();
    assert_eq!(submitted.len(), 1, "flat close must submit exactly one order");
    assert!(matches!(submitted[0].side, Side::Buy));
    assert!((submitted[0].size - 0.03).abs() < 1e-9);
    assert_eq!(record.new_pos, 0.0);
    assert_eq!(record.new_entry, 0.0);
    assert_eq!(record.fill_size, Some(0.03));
    assert_eq!(record.order_state, Some(OrderState::Filled));
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

    async fn open_positions(&self, _assets: &[String]) -> anyhow::Result<Vec<BrokerPosition>> {
        Ok(Vec::new())
    }

    async fn cancel_open_orders(&self, _asset: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct CapturedErrorClasses {
    values: Arc<Mutex<Vec<String>>>,
}

struct ErrorClassLayer {
    captured: CapturedErrorClasses,
}

impl<S> tracing_subscriber::Layer<S> for ErrorClassLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let mut visitor = ErrorClassVisitor::default();
        event.record(&mut visitor);
        if let Some(value) = visitor.value {
            self.captured.values.lock().unwrap().push(value);
        }
    }
}

#[derive(Default)]
struct ErrorClassVisitor {
    value: Option<String>,
}

impl tracing::field::Visit for ErrorClassVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "error_class" {
            self.value = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "error_class" {
            self.value = Some(format!("{value:?}").trim_matches('"').to_string());
        }
    }
}

#[tokio::test]
async fn broker_rejection_maps_to_rejected_order_state() {
    let mock = Arc::new(ErrorBroker {
        message: "alpaca create_order: insufficient buying power for this order".into(),
    });
    let mut sink = RealBrokerFills::new(mock);
    let captured = CapturedErrorClasses::default();
    let subscriber = tracing_subscriber::registry().with(ErrorClassLayer {
        captured: captured.clone(),
    });
    let _guard = tracing::subscriber::set_default(subscriber);

    let record = sink
        .submit(req("long_open", 0.0, 100_000.0, 0.02, 50_000.0))
        .await;

    // The trait is infallible — the broker error surfaces as a
    // Rejected no-fill record.
    assert_eq!(record.order_state, Some(OrderState::Rejected));
    assert!(record.fill_price.is_none());
    assert!(record.fill_size.is_none());
    assert_eq!(record.realized_pnl, 0.0);
    assert_eq!(
        captured.values.lock().unwrap().as_slice(),
        &["broker_insufficient_funds"]
    );
}

#[tokio::test]
async fn paused_safety_gate_blocks_submit_before_broker_call() {
    let pool = support::safety_pool_with_migrations().await;
    let manager = SafetyManager::new(pool);
    manager.bootstrap(false).await.unwrap();
    manager
        .pause(Some("operator pause".into()), &AuthContext::api_anonymous())
        .await
        .unwrap();
    let gate = SafetyGate::new(manager);
    let mock = RecordingBroker::new(50_000.0);
    let mut sink =
        RealBrokerFills::new(mock.clone()).with_safety_gate(gate, VenueLabel::Paper, VenueLabel::Paper, None);

    let record = sink
        .submit(req("long_open", 0.0, 100_000.0, 0.02, 50_000.0))
        .await;

    assert!(
        mock.submitted().is_empty(),
        "SafetyGate denial must not reach broker"
    );
    assert_eq!(record.order_state, Some(OrderState::Rejected));
    let (_, reason) = record
        .broker_error
        .expect("denial must be surfaced as broker_error");
    assert!(reason.contains("safety_paused"), "unexpected reason: {reason}");
}
