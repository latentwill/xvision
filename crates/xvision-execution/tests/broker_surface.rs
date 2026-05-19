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
    expected_order_id: String,
    account: AlpacaAccount,
    positions: Vec<AlpacaPosition>,
    captured: Arc<Mutex<Option<ApacOrderRequest>>>,
    get_order_ids: Arc<Mutex<Vec<String>>>,
    get_position_symbols: Arc<Mutex<Vec<String>>>,
}

impl MockAlpacaApi {
    fn new(
        account: AlpacaAccount,
        positions: Vec<AlpacaPosition>,
        pending: AlpacaOrder,
        filled: AlpacaOrder,
    ) -> Self {
        let expected_order_id = pending.id.clone();
        Self {
            create_order_result: Arc::new(Mutex::new(Some(pending))),
            get_order_result: Arc::new(Mutex::new(Some(filled))),
            expected_order_id,
            account,
            positions,
            captured: Arc::new(Mutex::new(None)),
            get_order_ids: Arc::new(Mutex::new(Vec::new())),
            get_position_symbols: Arc::new(Mutex::new(Vec::new())),
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

    async fn get_order(&self, order_id: &str) -> Result<AlpacaOrder, ExecutorError> {
        self.get_order_ids.lock().unwrap().push(order_id.to_string());
        if order_id != self.expected_order_id {
            return Err(ExecutorError::Internal(format!(
                "unexpected mock order lookup: {order_id}"
            )));
        }
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

    async fn get_position(&self, symbol: &str) -> Result<Option<AlpacaPosition>, ExecutorError> {
        self.get_position_symbols.lock().unwrap().push(symbol.to_string());
        Ok(self
            .positions
            .iter()
            .find(|position| position.symbol == symbol)
            .cloned())
    }
}

// ── Fixtures ─────────────────────────────────────────────────────────────────

fn fixture_account() -> AlpacaAccount {
    AlpacaAccount {
        equity: 100_000.0,
        last_equity: 99_500.0,
        cash: 100_000.0,
        buying_power: 100_000.0,
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
    let expected_order_id = pending.id.clone();
    let filled = fixture_filled_order(client_id);

    let mock = MockAlpacaApi::new(fixture_account(), vec![fixture_position()], pending, filled);
    let captured = Arc::clone(&mock.captured);
    let get_order_ids = Arc::clone(&mock.get_order_ids);

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

    // Crypto orders must NOT carry bracket legs — Alpaca's crypto API
    // rejects `Class::Bracket`. The TP/SL pcts in the OrderRequest are
    // dropped before submission and the simple market order goes through.
    let cap = captured.lock().unwrap().clone().unwrap();
    assert_eq!(cap.client_order_id, client_id);
    assert!(
        cap.take_profit_price.is_none(),
        "crypto submit must omit take_profit_price"
    );
    assert!(
        cap.stop_loss_price.is_none(),
        "crypto submit must omit stop_loss_price"
    );
    assert_eq!(*get_order_ids.lock().unwrap(), vec![expected_order_id]);
}

#[tokio::test]
async fn alpaca_paper_submit_crypto_drops_bracket_legs_when_flat() {
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
    // Even with TP/SL pcts supplied, crypto submissions strip them out
    // because Alpaca's crypto API does not support bracket orders.
    assert!(
        cap.take_profit_price.is_none(),
        "crypto submit must omit take_profit_price even with TP pct set"
    );
    assert!(
        cap.stop_loss_price.is_none(),
        "crypto submit must omit stop_loss_price even with SL pct set"
    );
}

#[tokio::test]
async fn alpaca_paper_submit_non_crypto_keeps_bracket_legs() {
    // Future-proof: non-crypto symbols (e.g. equities, once wired up)
    // must still carry bracket legs. The helper resolves crypto-ness
    // via `AssetSymbol::from_str`, so a symbol that doesn't parse —
    // here `"AAPL"` — falls through to the bracket-leg path.
    let client_id = "test-equity-buy-1";
    let pending = fixture_pending_order();
    let filled = fixture_filled_order(client_id);

    let mock = MockAlpacaApi::new(fixture_account(), vec![], pending, filled);
    let captured = Arc::clone(&mock.captured);

    let surface = AlpacaPaperSurface::with_api(Arc::new(mock));

    let req = OrderRequest {
        asset: "AAPL".into(),
        side: Side::Buy,
        size: 10.0,
        reference_price_usd: 200.0,
        stop_loss_pct: Some(2.0),
        take_profit_pct: Some(5.0),
        idempotency_key: client_id.into(),
    };

    surface.submit_order(req).await.expect("submit must succeed");

    let cap = captured.lock().unwrap().clone().unwrap();
    assert_eq!(cap.notional, 2_000.0);
    assert_eq!(cap.take_profit_price, Some(210.0));
    assert_eq!(cap.stop_loss_price, Some(196.0));
}

#[tokio::test]
async fn alpaca_paper_submit_crypto_short_from_flat_is_refused() {
    // Selling crypto without an existing long position is not supported
    // on Alpaca (crypto is long-only). The surface refuses the request
    // before round-tripping to Alpaca, with a classifier-friendly message.
    let mock = MockAlpacaApi::new(
        fixture_account(),
        vec![], // no open position
        fixture_pending_order(),
        fixture_filled_order("never-filled"),
    );
    let surface = AlpacaPaperSurface::with_api(Arc::new(mock));

    let req = OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Sell,
        size: 0.05,
        reference_price_usd: 70_000.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: "test-crypto-short-1".into(),
    };

    let err = surface
        .submit_order(req)
        .await
        .expect_err("crypto short_open from flat must be refused");
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("broker_unsupported"),
        "error must carry the broker_unsupported tag, got: {msg}"
    );
    assert!(
        msg.contains("short_open is not supported") || msg.contains("not shortable"),
        "error must explain why the order is refused, got: {msg}"
    );
}

#[tokio::test]
async fn alpaca_paper_submit_crypto_sell_closes_existing_long() {
    // Selling crypto WITH an existing long position is a position close,
    // not a short_open. The surface lets it through.
    let client_id = "test-crypto-close-1";
    let pending = fixture_pending_order();
    let filled = fixture_filled_order(client_id);

    let mock = MockAlpacaApi::new(
        fixture_account(),
        vec![fixture_position()], // long 0.5 BTC
        pending,
        filled,
    );
    let captured = Arc::clone(&mock.captured);
    let surface = AlpacaPaperSurface::with_api(Arc::new(mock));

    let req = OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Sell,
        size: 0.05,
        reference_price_usd: 70_000.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: client_id.into(),
    };

    surface
        .submit_order(req)
        .await
        .expect("crypto sell that closes an existing long must succeed");

    let cap = captured.lock().unwrap().clone().unwrap();
    assert_eq!(cap.client_order_id, client_id);
    assert!(cap.take_profit_price.is_none());
    assert!(cap.stop_loss_price.is_none());
}

#[tokio::test]
async fn alpaca_paper_submit_crypto_sell_oversize_long_is_refused() {
    // A sell larger than the open long would net into a short on fill,
    // which Alpaca crypto does not support. The preflight must refuse
    // the order before round-tripping to Alpaca rather than rely on the
    // server to reject it.
    let mut pos = fixture_position();
    pos.qty = 0.05; // long 0.05 BTC

    let mock = MockAlpacaApi::new(
        fixture_account(),
        vec![pos],
        fixture_pending_order(),
        fixture_filled_order("never-filled"),
    );
    let surface = AlpacaPaperSurface::with_api(Arc::new(mock));

    let req = OrderRequest {
        asset: "BTC/USD".into(),
        side: Side::Sell,
        size: 0.10, // > 0.05 long
        reference_price_usd: 70_000.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: "test-oversize-sell".into(),
    };

    let err = surface
        .submit_order(req)
        .await
        .expect_err("oversize crypto sell must be refused");
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("broker_unsupported"),
        "error must carry the broker_unsupported tag, got: {msg}"
    );
    assert!(
        msg.contains("exceeds open long position") || msg.contains("would net into a short"),
        "error must explain the oversize-sell case, got: {msg}"
    );
}

#[tokio::test]
async fn alpaca_paper_position_returns_qty() {
    let mock = MockAlpacaApi::new(
        fixture_account(),
        vec![fixture_position()],
        fixture_pending_order(),
        fixture_filled_order("ignored"),
    );
    let get_position_symbols = Arc::clone(&mock.get_position_symbols);
    let surface = AlpacaPaperSurface::with_api(Arc::new(mock));
    let qty = surface.position("BTC/USD").await.expect("position must succeed");
    assert_eq!(qty, 0.5);
    assert_eq!(*get_position_symbols.lock().unwrap(), vec!["BTC/USD"]);
}

#[tokio::test]
async fn alpaca_paper_position_ignores_different_symbol_holdings() {
    let mock = MockAlpacaApi::new(
        fixture_account(),
        vec![fixture_position()],
        fixture_pending_order(),
        fixture_filled_order("ignored"),
    );
    let get_position_symbols = Arc::clone(&mock.get_position_symbols);
    let surface = AlpacaPaperSurface::with_api(Arc::new(mock));
    let qty = surface.position("ETH/USD").await.expect("position must succeed");
    assert_eq!(qty, 0.0);
    assert_eq!(*get_position_symbols.lock().unwrap(), vec!["ETH/USD"]);
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

#[tokio::test]
async fn alpaca_paper_buying_power_routes_crypto_to_cash_equities_to_buying_power() {
    // Distinct values per field so a mis-wiring is impossible to miss.
    let account = AlpacaAccount {
        equity: 100_000.0,
        last_equity: 99_500.0,
        cash: 12_345.0,
        buying_power: 67_890.0,
    };
    let mock = MockAlpacaApi::new(
        account,
        vec![],
        fixture_pending_order(),
        fixture_filled_order("ignored"),
    );
    let surface = AlpacaPaperSurface::with_api(Arc::new(mock));

    // Crypto pair → settled cash (the field Alpaca actually validates
    // crypto buys against and reports in `insufficient balance for USD`).
    let crypto_bp = surface
        .buying_power("BTC/USD")
        .await
        .expect("crypto buying_power must succeed");
    assert_eq!(crypto_bp, 12_345.0, "crypto must route to cash");

    // Equity ticker → Alpaca's `buying_power` (which may include margin).
    let equity_bp = surface
        .buying_power("AAPL")
        .await
        .expect("equity buying_power must succeed");
    assert_eq!(equity_bp, 67_890.0, "equities must route to buying_power");

    // And equity (curve / metrics) is still untouched by the new method.
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
