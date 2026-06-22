//! `byreal_spot` inherits the SafetyGate via `GatedBrokerSurface` — the same
//! single production seam every live venue flows through (`build_live_executor`
//! wraps every broker in `GatedBrokerSurface`). Proves the spec's Phase-A
//! safety claims at the gate boundary, with a recording mock `ByrealSpotApi`
//! (no subprocess, no funds):
//!
//! (a) PAUSED gate ⇒ ZERO swaps reach the inner `ByrealSpotSurface`.
//! (b) Paper-run + Live-broker label mismatch ⇒ blocked, ZERO swaps.
//! (c) allow_all + matching labels ⇒ the swap reaches the inner surface once.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use xvision_core::config::{SpotAssetConfig, SpotAssetEntry, SpotAssetKind};
use xvision_execution::broker_surface::{BrokerSurface, OrderRequest, Side};
use xvision_execution::executor::ExecutorError;
use xvision_execution::{ByrealSpotApi, ByrealSpotMode, ByrealSpotSurface, SwapResult};

use xvision_engine::eval::executor::GatedBrokerSurface;
use xvision_engine::safety::{AuthContext, SafetyGate, SafetyManager, VenueLabel};

// --- recording mock ByrealSpotApi (counts swaps) ---------------------------

#[derive(Default, Clone)]
struct CountingSpotApi {
    swaps: Arc<Mutex<u32>>,
}

#[async_trait]
impl ByrealSpotApi for CountingSpotApi {
    async fn swap(
        &self,
        _input: &str,
        _output: &str,
        _amount: f64,
        _slippage_bps: u32,
        _mode: ByrealSpotMode,
    ) -> Result<SwapResult, ExecutorError> {
        *self.swaps.lock().unwrap() += 1;
        Ok(SwapResult {
            mode: Some("dry-run".into()),
            order_id: Some("sig".into()),
            transaction: Some(String::new()),
            ui_out_amount: Some("1.0".into()),
            price_impact_pct: Some("0.01".into()),
        })
    }
    async fn token_price(&self, _mint: &str) -> Result<f64, ExecutorError> {
        Ok(150.0)
    }
    async fn token_balance(&self, _mint: &str) -> Result<f64, ExecutorError> {
        Ok(10.0)
    }
}

fn curated() -> SpotAssetConfig {
    SpotAssetConfig {
        usdc_mint: "USDC1111111111111111111111111111111111111111".into(),
        assets: vec![SpotAssetEntry {
            symbol: "SOL".into(),
            mint: "So11111111111111111111111111111111111111112".into(),
            kind: SpotAssetKind::Spl,
            decimals: 9,
        }],
    }
}

fn buy() -> OrderRequest {
    OrderRequest {
        asset: "SOL".into(),
        side: Side::Buy,
        size: 1.0,
        reference_price_usd: 150.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: "k1".into(),
    }
}

async fn paused_gate() -> SafetyGate {
    use sqlx::sqlite::SqlitePoolOptions;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/030_safety_state_and_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let mgr = SafetyManager::new(pool);
    mgr.bootstrap(false).await.unwrap();
    mgr.pause(Some("test".into()), &AuthContext::system())
        .await
        .unwrap();
    SafetyGate::new(mgr)
}

async fn unpaused_gate() -> SafetyGate {
    use sqlx::sqlite::SqlitePoolOptions;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/030_safety_state_and_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let mgr = SafetyManager::new(pool);
    mgr.bootstrap(false).await.unwrap();
    SafetyGate::new(mgr)
}

#[tokio::test]
async fn paused_gate_blocks_spot_swaps() {
    let api = CountingSpotApi::default();
    let swaps = api.swaps.clone();
    let inner: Arc<dyn BrokerSurface> =
        Arc::new(ByrealSpotSurface::new(api, curated()).with_mode(ByrealSpotMode::Live));
    // Matching Paper/Paper labels: the ONLY denial reason is the pause state.
    let gated = GatedBrokerSurface::new(
        inner,
        paused_gate().await,
        VenueLabel::Paper,
        VenueLabel::Paper,
        AuthContext::system(),
        None,
    );
    assert!(gated.submit_order(buy()).await.is_err());
    assert_eq!(*swaps.lock().unwrap(), 0, "paused gate must block all swaps");
}

#[tokio::test]
async fn venue_label_mismatch_blocks_spot_swaps() {
    let api = CountingSpotApi::default();
    let swaps = api.swaps.clone();
    let inner: Arc<dyn BrokerSurface> =
        Arc::new(ByrealSpotSurface::new(api, curated()).with_mode(ByrealSpotMode::Live));
    // Not paused; Paper run + Live broker label ⇒ VenueLabelMismatch.
    let gated = GatedBrokerSurface::new(
        inner,
        unpaused_gate().await,
        VenueLabel::Paper,
        VenueLabel::Live,
        AuthContext::system(),
        None,
    );
    assert!(gated.submit_order(buy()).await.is_err());
    assert_eq!(
        *swaps.lock().unwrap(),
        0,
        "venue-label mismatch must block the swap"
    );
}

#[tokio::test]
async fn allow_all_gate_lets_spot_swap_through() {
    let api = CountingSpotApi::default();
    let swaps = api.swaps.clone();
    let inner: Arc<dyn BrokerSurface> =
        Arc::new(ByrealSpotSurface::new(api, curated()).with_mode(ByrealSpotMode::Preview));
    let gated = GatedBrokerSurface::new(
        inner,
        SafetyGate::allow_all(),
        VenueLabel::Paper,
        VenueLabel::Paper,
        AuthContext::system(),
        None,
    );
    gated.submit_order(buy()).await.unwrap();
    assert_eq!(
        *swaps.lock().unwrap(),
        1,
        "allow_all gate must let the swap through"
    );
}
