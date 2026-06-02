use std::sync::Arc;

use xvision_execution::bybit::{to_bybit_symbol, BybitPaperSurface, MockBybitClient};
use xvision_execution::{BrokerOrderRequest, BrokerSurface, Side};

#[tokio::test]
async fn bybit_paper_surface_buy_records_correct_symbol() {
    let mock = Arc::new(MockBybitClient::new());
    let surface = BybitPaperSurface::with_api(Arc::clone(&mock));
    let req = BrokerOrderRequest {
        asset: "BTC/USD".to_string(),
        side: Side::Buy,
        size: 0.01,
        reference_price_usd: 50_000.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: "test-buy-btc".to_string(),
    };
    surface
        .submit_order(req)
        .await
        .expect("submit_order must succeed");
    let calls = mock.place_order_calls();
    assert_eq!(calls.len(), 1, "exactly one place_order call expected");
    assert_eq!(calls[0].symbol, "BTCUSDT");
    assert_eq!(calls[0].side, "Buy");
    assert_eq!(calls[0].time_in_force, "IOC");
}

#[tokio::test]
async fn bybit_paper_surface_sell_records_correct_symbol() {
    let mock = Arc::new(MockBybitClient::new());
    let surface = BybitPaperSurface::with_api(Arc::clone(&mock));
    let req = BrokerOrderRequest {
        asset: "ETH/USD".to_string(),
        side: Side::Sell,
        size: 0.5,
        reference_price_usd: 3_000.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: "test-sell-eth".to_string(),
    };
    surface
        .submit_order(req)
        .await
        .expect("submit_order must succeed");
    let calls = mock.place_order_calls();
    assert_eq!(calls.len(), 1, "exactly one place_order call expected");
    assert_eq!(calls[0].symbol, "ETHUSDT");
    assert_eq!(calls[0].side, "Sell");
}

#[tokio::test]
async fn bybit_paper_surface_position_passthrough() {
    let mock = Arc::new(MockBybitClient::new());
    let surface = BybitPaperSurface::with_api(Arc::clone(&mock));
    let _ = surface.position("BTC/USD").await.expect("position must succeed");
    let calls = mock.positions_calls();
    assert_eq!(calls.len(), 1, "exactly one positions call expected");
    assert_eq!(calls[0], "BTCUSDT");
}

#[tokio::test]
async fn bybit_paper_surface_balance_passthrough() {
    let mock = Arc::new(MockBybitClient::new());
    let surface = BybitPaperSurface::with_api(Arc::clone(&mock));
    let balance = surface.balance().await.expect("balance must succeed");
    assert_eq!(
        mock.wallet_balance_call_count(),
        1,
        "wallet_balance must be called once"
    );
    assert_eq!(balance, 100_000.0);
}

#[test]
fn bybit_asset_symbol_mapping_btc() {
    assert_eq!(to_bybit_symbol("BTC/USD"), "BTCUSDT");
    assert_eq!(to_bybit_symbol("BTCUSD"), "BTCUSDT");
    assert_eq!(to_bybit_symbol("BTCUSDT"), "BTCUSDT");
    assert_eq!(to_bybit_symbol("ETH/USD"), "ETHUSDT");
    assert_eq!(to_bybit_symbol("SOL/USD"), "SOLUSDT");
}
