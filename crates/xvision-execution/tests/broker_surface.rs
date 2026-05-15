//! Integration tests for the unified BrokerSurface trait.
//!
//! Exercises the trait shape, the AlpacaPaperSurface impl using a mock
//! AlpacaApi (so no network calls), and the public MockBrokerSurface that
//! downstream crates (eval-engine) consume in their own tests.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use xvision_execution::alpaca::{
    AlpacaAccount, AlpacaApi, AlpacaOrder, AlpacaPosition, OrderRequest as ApacOrderRequest,
};
use xvision_execution::broker_surface::{
    AlpacaPaperSurface, BrokerKind, BrokerSurface, MockBrokerSurface, OrderConfirmation, OrderRequest, Side,
};
use xvision_execution::executor::ExecutorError;

// ── Mock AlpacaApi for integration tests ─────────────────────────────────────

struct MockAlpacaApi {
    create_order_result: Arc<Mutex<Option<AlpacaOrder>>>,
    get_order_result: Arc<Mutex<Option<AlpacaOrder>>>,
    account: AlpacaAccount,
    positions: Vec<AlpacaPosition>,
    captured: Arc<Mutex<Option<ApacOrderRequest>>>,
}

impl MockAlpacaApi {
    fn new(
        account: AlpacaAccount,
        positions: Vec<AlpacaPosition>,
        pending: AlpacaOrder,
        filled: AlpacaOrder,
    ) -> Self {
        Self {
            create_order_result: Arc::new(Mutex::new(Some(pending))),
            get_order_result: Arc::new(Mutex::new(Some(filled))),
            account,
            positions,
            captured: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl AlpacaApi for MockAlpacaApi {
    async fn create_order(&self, req: ApacOrderRequest) -> Result<AlpacaOrder, ExecutorError> {
        *self.captured.lock().unwrap() = Some(req);
        self.create_order_result
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| ExecutorError::Internal("no mock order".into()))
    }

    async fn get_order(&self, _order_id: &str) -> Result<AlpacaOrder, ExecutorError> {
        self.get_order_result
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| ExecutorError::Internal("no mock filled order".into()))
    }

    async fn get_account(&self) -> Result<AlpacaAccount, ExecutorError> {
        Ok(self.account.clone())
    }

    async fn list_positions(&self) -> Result<Vec<AlpacaPosition>, ExecutorError> {
        Ok(self.positions.clone())
    }

    async fn get_position(&self, _symbol: &str) -> Result<Option<AlpacaPosition>, ExecutorError> {
        Ok(self.positions.first().cloned())
    }
}

// ── Fixtures ─────────────────────────────────────────────────────────────────

fn fixture_account() -> AlpacaAccount {
    AlpacaAccount {
        equity: 100_000.0,
        last_equity: 99_500.0,
    }
}

fn fixture_position() -> AlpacaPosition {
    AlpacaPosition {
        symbol: "BTC/USD".into(),
        side: "long".into(),
        avg_entry_price: 70_000.0,
        current_price: Some(71_000.0),
        qty: 0.5,
        market_value: Some(35_500.0),
        equity_usd: None,
    }
}

fn fixture_pending_order() -> AlpacaOrder {
    AlpacaOrder {
        id: Uuid::new_v4().to_string(),
        client_order_id: Uuid::new_v4().to_string(),
        status: "new".into(),
        filled_qty: 0.0,
        avg_fill_price: None,
        submitted_at: Some(Utc::now()),
        filled_at: None,
    }
}

fn fixture_filled_order(client_id: &str) -> AlpacaOrder {
    AlpacaOrder {
        id: Uuid::new_v4().to_string(),
        client_order_id: client_id.to_string(),
        status: "filled".into(),
        filled_qty: 0.05,
        avg_fill_price: Some(70_500.0),
        submitted_at: Some(Utc::now()),
        filled_at: Some(Utc::now()),
    }
}

// ── Trait shape tests ────────────────────────────────────────────────────────

#[test]
fn broker_kind_covers_all_variants() {
    use BrokerKind::*;
    let _ = [AlpacaPaper, AlpacaLive, OrderlyLive];
}

#[test]
fn order_request_has_expected_fields() {
    let req = OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Buy,
        size: 0.05,
        reference_price_usd: 71_000.0,
        stop_loss_pct: Some(2.0),
        take_profit_pct: Some(5.0),
        idempotency_key: "test-key-1".into(),
    };
    assert_eq!(req.asset, "BTC/USD");
    assert_eq!(req.size, 0.05);
    assert!(matches!(req.side, Side::Buy));
}

#[test]
fn order_confirmation_has_expected_fields() {
    let conf = OrderConfirmation {
        broker_order_id: "abc-123".into(),
        fill_price: Some(70_500.0),
        fill_size: 0.05,
        fee: None,
    };
    assert_eq!(conf.broker_order_id, "abc-123");
    assert_eq!(conf.fill_size, 0.05);
}

// ── AlpacaPaperSurface tests (no network) ────────────────────────────────────

#[tokio::test]
async fn alpaca_paper_submit_buy_returns_confirmation() {
    let client_id = "test-buy-1";
    let pending = fixture_pending_order();
    let filled = fixture_filled_order(client_id);

    let mock = MockAlpacaApi::new(fixture_account(), vec![fixture_position()], pending, filled);
    let captured = Arc::clone(&mock.captured);

    let surface = AlpacaPaperSurface::with_api(Arc::new(mock));

    let req = OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Buy,
        size: 0.05,
        reference_price_usd: 71_000.0,
        stop_loss_pct: Some(2.0),
        take_profit_pct: Some(5.0),
        idempotency_key: client_id.into(),
    };

    let conf = surface.submit_order(req).await.expect("submit must succeed");

    assert_eq!(conf.fill_size, 0.05, "fill_size matches the mock filled qty");
    assert_eq!(conf.fill_price, Some(70_500.0));

    // Captured request should have the idempotency key as client_order_id
    // and bracket legs derived from current_price.
    let cap = captured.lock().unwrap().clone().unwrap();
    assert_eq!(cap.client_order_id, client_id);
    assert!(cap.take_profit_price.is_some());
    assert!(cap.stop_loss_price.is_some());
}

#[tokio::test]
async fn alpaca_paper_submit_uses_order_reference_price_when_flat() {
    let client_id = "test-flat-buy-1";
    let pending = fixture_pending_order();
    let filled = fixture_filled_order(client_id);

    let mock = MockAlpacaApi::new(fixture_account(), vec![], pending, filled);
    let captured = Arc::clone(&mock.captured);

    let surface = AlpacaPaperSurface::with_api(Arc::new(mock));

    let req = OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Buy,
        size: 0.05,
        reference_price_usd: 70_000.0,
        stop_loss_pct: Some(2.0),
        take_profit_pct: Some(5.0),
        idempotency_key: client_id.into(),
    };

    let conf = surface.submit_order(req).await.expect("submit must succeed");
    assert_eq!(conf.fill_size, 0.05);

    let cap = captured.lock().unwrap().clone().unwrap();
    assert_eq!(cap.client_order_id, client_id);
    assert_eq!(cap.notional, 3_500.0);
    assert_eq!(cap.take_profit_price, Some(73_500.0));
    assert_eq!(cap.stop_loss_price, Some(68_600.0));
}

#[tokio::test]
async fn alpaca_paper_position_returns_qty() {
    let mock = MockAlpacaApi::new(
        fixture_account(),
        vec![fixture_position()],
        fixture_pending_order(),
        fixture_filled_order("ignored"),
    );
    let surface = AlpacaPaperSurface::with_api(Arc::new(mock));
    let qty = surface.position("BTC/USD").await.expect("position must succeed");
    assert_eq!(qty, 0.5);
}

#[tokio::test]
async fn alpaca_paper_position_returns_zero_when_no_holdings() {
    let mock = MockAlpacaApi::new(
        fixture_account(),
        vec![],
        fixture_pending_order(),
        fixture_filled_order("ignored"),
    );
    let surface = AlpacaPaperSurface::with_api(Arc::new(mock));
    let qty = surface.position("BTC/USD").await.expect("position must succeed");
    assert_eq!(qty, 0.0);
}

#[tokio::test]
async fn alpaca_paper_balance_returns_equity() {
    let mock = MockAlpacaApi::new(
        fixture_account(),
        vec![],
        fixture_pending_order(),
        fixture_filled_order("ignored"),
    );
    let surface = AlpacaPaperSurface::with_api(Arc::new(mock));
    let bal = surface.balance().await.expect("balance must succeed");
    assert_eq!(bal, 100_000.0);
}

// ── MockBrokerSurface tests (downstream pattern) ─────────────────────────────

#[tokio::test]
async fn mock_broker_surface_records_submissions() {
    let mock = MockBrokerSurface::new(150_000.0);
    let req = OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Buy,
        size: 0.1,
        reference_price_usd: 71_250.0,
        stop_loss_pct: Some(2.0),
        take_profit_pct: Some(5.0),
        idempotency_key: "mock-1".into(),
    };

    let conf = mock.submit_order(req.clone()).await.unwrap();
    assert_eq!(conf.fill_size, 0.1);
    assert!(conf.fill_price.is_some());

    let submitted = mock.submitted();
    assert_eq!(submitted.len(), 1);
    assert_eq!(submitted[0].asset, "BTC/USD");
    assert_eq!(submitted[0].idempotency_key, "mock-1");
}

#[tokio::test]
async fn mock_broker_surface_balance_matches_seed() {
    let mock = MockBrokerSurface::new(75_000.0);
    assert_eq!(mock.balance().await.unwrap(), 75_000.0);
}

#[tokio::test]
async fn mock_broker_surface_position_starts_zero() {
    let mock = MockBrokerSurface::new(75_000.0);
    assert_eq!(mock.position("BTC/USD").await.unwrap(), 0.0);
}

#[tokio::test]
async fn mock_broker_surface_buy_increments_position() {
    let mock = MockBrokerSurface::new(75_000.0);
    let req = OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Buy,
        size: 0.2,
        reference_price_usd: 71_250.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: "k".into(),
    };
    mock.submit_order(req).await.unwrap();
    assert_eq!(mock.position("BTC/USD").await.unwrap(), 0.2);
}

#[tokio::test]
async fn mock_broker_surface_sell_decrements_position() {
    let mock = MockBrokerSurface::new(75_000.0);

    let buy = OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Buy,
        size: 0.5,
        reference_price_usd: 71_250.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: "buy".into(),
    };
    mock.submit_order(buy).await.unwrap();

    let sell = OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Sell,
        size: 0.2,
        reference_price_usd: 71_500.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: "sell".into(),
    };
    mock.submit_order(sell).await.unwrap();

    assert_eq!(mock.position("BTC/USD").await.unwrap(), 0.3);
}
