//! Integration test — safety gate is threaded through the live executor via
//! `GatedBrokerSurface`.
//!
//! Proves two properties (DoD 4):
//!
//! (a) **Paused gate → ZERO inner-broker calls.**
//!     A real `SafetyManager` is bootstrapped, paused, wrapped in a
//!     `SafetyGate`, then handed to `GatedBrokerSurface`. Even though a
//!     long_open signal fires every bar, no order reaches the inner
//!     `CountingBroker`.
//!
//! (b) **Unpaused (allow_all) gate with matching labels → inner broker
//!     DOES receive submits.**
//!     `SafetyGate::allow_all()` + matching Paper/Paper labels → the
//!     counting broker sees exactly one order from a single-bar stream.
//!
//! Unit tests for `broker_label_for` (DoD 3) live as `#[cfg(test)]` inline
//! tests inside `crates/xvision-engine/src/api/eval.rs`.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use futures::stream;

use xvision_core::Capital;
use xvision_data::alpaca::{BarGranularity, MarketBar};
use xvision_data::alpaca_live::{AlpacaLiveClient, AlpacaLiveCredentials, LiveBarItem};
use xvision_data::alpaca_live_poll::{AlpacaLivePoll, AlpacaPollError, LivePollFetcher};
use xvision_execution::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest};

use xvision_core::trading::AssetSymbol;
use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::eval::executor::{
    Executor, GatedBrokerSurface, LiveStream, MultiLiveStream, RunExecutor, WallClock,
};
use xvision_engine::eval::live_config::{LiveConfig, StopPolicy};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::{AssetClass, AssetRef, Scenario};
use xvision_engine::eval::store::RunStore;
use xvision_engine::safety::{AuthContext, SafetyGate, SafetyManager, VenueLabel};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

// ---------------------------------------------------------------------------
// Fixtures (mirrors eval_executor_live_loop.rs helpers)
// ---------------------------------------------------------------------------

fn ts(seconds: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(seconds, 0).single().expect("valid ts")
}

fn market_bar_at(seconds: i64, close: f64) -> MarketBar {
    MarketBar {
        timestamp: ts(seconds),
        open: close - 1.0,
        high: close + 1.0,
        low: close - 2.0,
        close,
        volume: 1_000.0,
    }
}

struct EmptyFetcher;

#[async_trait]
impl LivePollFetcher for EmptyFetcher {
    async fn fetch_window(
        &self,
        _asset: &str,
        _granularity: BarGranularity,
        _start: chrono::DateTime<Utc>,
        _end: chrono::DateTime<Utc>,
    ) -> Result<Vec<MarketBar>, AlpacaPollError> {
        Err(AlpacaPollError::Empty)
    }
}

fn live_client() -> AlpacaLiveClient {
    AlpacaLiveClient::new(AlpacaLiveCredentials {
        key_id: "test".into(),
        secret_key: "test".into(),
    })
}

fn live_stream_for(asset: &str, bars: Vec<MarketBar>) -> LiveStream {
    let ws_items: Vec<LiveBarItem> = bars.into_iter().map(LiveBarItem::Bar).collect();
    let ws = live_client().subscription_from_stream(BarGranularity::Minute1, stream::iter(ws_items));
    let poll = AlpacaLivePoll::new(Arc::new(EmptyFetcher), asset.into(), BarGranularity::Minute1)
        .with_poll_interval(Duration::ZERO);
    LiveStream::new_for_test(Vec::new(), ws, poll)
}

fn single_asset_stream(bars: Vec<MarketBar>) -> MultiLiveStream {
    MultiLiveStream::new(vec![(AssetSymbol::Btc, live_stream_for("BTC/USD", bars))])
}

fn long_open_dispatch() -> Arc<dyn xvision_engine::agent::llm::LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.9,"justification":"gated-submit-test"}"#,
    ))
}

fn build_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01TESTGATEDSUBMIT".into(),
            display_name: "gated-submit test strategy".into(),
            plain_summary: "safety-gate live loop coverage".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 1,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
        risk: RiskPreset::Balanced.expand(),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

fn live_scenario(initial: f64) -> Scenario {
    #[allow(deprecated)]
    let mut scenario = xvision_engine::eval::scenario::canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("canonical scenario must exist");
    scenario.capital = Capital {
        initial,
        currency: "USD".into(),
    };
    scenario.warmup_bars = 0;
    scenario
}

fn live_config() -> LiveConfig {
    LiveConfig {
        strategy_id: "01TESTGATEDSUBMIT".into(),
        assets: vec![AssetRef {
            class: AssetClass::Crypto,
            symbol: "BTC/USD".into(),
            venue_symbol: "BTC/USD".into(),
        }],
        capital: Capital {
            initial: 100_000.0,
            currency: "USD".into(),
        },
        broker_creds_ref: "alpaca".into(),
        stop_policy: StopPolicy {
            bar_limit: Some(1_000),
            ..Default::default()
        },
        venue_label: VenueLabel::Paper,
        warmup_bars: Some(0),
        safety_limits: None,
        display_name: "gated-submit test run".into(),
        description: None,
        tags: vec![],
        notes: None,
    }
}

async fn fresh_fixtures() -> (RunStore, Strategy, Scenario, Run, tempfile::TempDir) {
    let (ctx, dir) = common::open_api_context().await;
    let store = RunStore::new(ctx.db.clone());
    let strategy = build_strategy();
    let scenario = live_scenario(100_000.0);
    let mut run = Run::new_queued(strategy.manifest.id.clone(), String::new(), RunMode::Live);
    run.live_config = Some(live_config());
    store.create(&run).await.unwrap();
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();
    (store, strategy, scenario, run, dir)
}

// ---------------------------------------------------------------------------
// CountingBroker — records every submit_order call
// ---------------------------------------------------------------------------

/// Recording broker that counts every `submit_order` call and fills at a
/// fixed price. Used to prove the gate blocks (count == 0) or allows
/// (count >= 1) broker submits.
struct CountingBroker {
    calls: Mutex<u32>,
}

impl CountingBroker {
    fn new() -> Arc<Self> {
        Arc::new(Self { calls: Mutex::new(0) })
    }

    fn call_count(&self) -> u32 {
        *self.calls.lock().unwrap()
    }
}

#[async_trait]
impl BrokerSurface for CountingBroker {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        *self.calls.lock().unwrap() += 1;
        Ok(OrderConfirmation {
            broker_order_id: format!("counted-{}", req.idempotency_key),
            fill_price: Some(50_000.0),
            fill_size: req.size,
            fee: None,
        })
    }

    async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
        Ok(0.0)
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        Ok(100_000.0)
    }

    fn venue(&self) -> &str {
        "counting-mock"
    }

    fn signing_scheme(&self) -> &str {
        "mock"
    }

    fn is_perp_venue(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Helper: build a paused SafetyGate from the already-migrated test DB.
// ApiContext::open calls migrate_safety_state_and_audit, so the
// `safety_state` table already exists in the pool that backs the store.
// ---------------------------------------------------------------------------

async fn paused_safety_gate_from_pool(pool: &sqlx::SqlitePool) -> SafetyGate {
    let mgr = SafetyManager::new(pool.clone());
    // bootstrap(false) → seeds safety_state row as running (not paused).
    // Then pause() flips it to paused in both DB and in-memory lock.
    mgr.bootstrap(false).await.unwrap();
    mgr.pause(
        Some("test pause — blocks all submits".into()),
        &AuthContext::system(),
    )
    .await
    .unwrap();
    SafetyGate::new(mgr)
}

// ---------------------------------------------------------------------------
// DoD 4a: PAUSED gate → inner broker receives ZERO submit_order calls
// ---------------------------------------------------------------------------

#[tokio::test]
async fn paused_safety_gate_blocks_all_broker_submits() {
    let (store, strategy, scenario, mut run, _dir) = fresh_fixtures().await;

    // Reuse the same pool the store uses (already migrated by ApiContext::open).
    let gate = paused_safety_gate_from_pool(store.pool()).await;

    let inner = CountingBroker::new();
    // run_venue_label=Paper, broker_venue_label=Paper — no mismatch; the only
    // denial reason is the pause state. This isolates the pause gate path.
    let gated_broker: Arc<dyn BrokerSurface> = Arc::new(GatedBrokerSurface::new(
        inner.clone(),
        gate,
        VenueLabel::Paper, // run_venue_label
        VenueLabel::Paper, // broker_venue_label — matches, no VenueLabelMismatch
        AuthContext::system(),
    ));

    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0)]);
    let executor = Executor::live(
        &live_config(),
        gated_broker,
        stream,
        WallClock::with_now_fn(|| ts(60)),
        None,
    )
    .expect("live executor builds");

    // A paused gate causes every submit_order to return
    // Err("safety_gate_denied: safety_paused: …"). The executor treats a
    // broker Err as a run failure, so the result must be Err.
    let result = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            long_open_dispatch(),
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await;

    assert!(
        result.is_err(),
        "paused gate must cause the run to fail (broker submit denied by gate)"
    );

    // The INNER broker must receive ZERO calls.
    assert_eq!(
        inner.call_count(),
        0,
        "inner broker must receive ZERO submit_order calls when gate is paused, got {}",
        inner.call_count()
    );
}

// ---------------------------------------------------------------------------
// DoD 4b: allow_all gate + matching labels → inner broker receives submits
// ---------------------------------------------------------------------------

#[tokio::test]
async fn allow_all_gate_with_matching_labels_delegates_to_inner_broker() {
    let (store, strategy, scenario, mut run, _dir) = fresh_fixtures().await;

    let inner = CountingBroker::new();
    let gated_broker: Arc<dyn BrokerSurface> = Arc::new(GatedBrokerSurface::new(
        inner.clone(),
        SafetyGate::allow_all(),
        VenueLabel::Paper,
        VenueLabel::Paper,
        AuthContext::system(),
    ));

    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0)]);
    let executor = Executor::live(
        &live_config(),
        gated_broker,
        stream,
        WallClock::with_now_fn(|| ts(60)),
        None,
    )
    .expect("live executor builds");

    let _metrics = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            long_open_dispatch(),
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await
        .expect("allow_all gated run completes on stream end");

    // The inner broker must have received at least one call (the long_open).
    assert!(
        inner.call_count() >= 1,
        "inner broker must receive at least 1 submit_order call through allow_all gate, got {}",
        inner.call_count()
    );
}
