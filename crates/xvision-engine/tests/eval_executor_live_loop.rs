//! Hermetic integration coverage for the §3 single-asset live loop
//! (cline-live-followups L1). No network, no Node sidecar.
//!
//! Wires a [`LiveStream::new_for_test`] (mock websocket subscription +
//! scripted poll) into [`Executor::live`] alongside a recording
//! [`BrokerSurface`] mock through [`RealBrokerFills`], and asserts:
//!
//!   - a mock stream emitting one bar drives exactly one decision cycle;
//!   - the mock broker RECEIVES the expected order (action / asset / size);
//!   - NO injected bars are required — the run starts from the stream;
//!   - fills come from broker-reported data, not simulated bar fills;
//!   - the loop exits cleanly on (a) stream end, (b) cancellation,
//!     (c) a broker error — without hanging or panicking.

mod common;

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use futures::stream;

use xvision_core::Capital;
use xvision_data::alpaca::{BarGranularity, MarketBar};
use xvision_data::alpaca_live::{AlpacaLiveClient, AlpacaLiveCredentials, LiveBarItem};
use xvision_data::alpaca_live_poll::{AlpacaLivePoll, AlpacaPollError, LivePollFetcher};
use xvision_execution::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest, Side};

use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::eval::executor::{Executor, LiveStream, RunExecutor, WallClock};
use xvision_engine::eval::live_config::{LiveConfig, StopPolicy};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::{AssetClass, AssetRef, Scenario};
use xvision_engine::eval::store::RunStore;
use xvision_engine::safety::VenueLabel;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

fn ts(seconds: i64) -> DateTime<Utc> {
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

fn client() -> AlpacaLiveClient {
    AlpacaLiveClient::new(AlpacaLiveCredentials {
        key_id: "test".into(),
        secret_key: "test".into(),
    })
}

/// Scripted poll fetcher that always returns `Empty` — closes the stream
/// deterministically once the websocket items drain.
struct EmptyFetcher;

#[async_trait]
impl LivePollFetcher for EmptyFetcher {
    async fn fetch_window(
        &self,
        _asset: &str,
        _granularity: BarGranularity,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<MarketBar>, AlpacaPollError> {
        Err(AlpacaPollError::Empty)
    }
}

/// Build a [`LiveStream`] that emits the given websocket bars then closes
/// (no warmup, poll returns `Empty`).
fn live_stream_from_bars(bars: Vec<MarketBar>) -> LiveStream {
    let ws_items: Vec<LiveBarItem> = bars.into_iter().map(LiveBarItem::Bar).collect();
    let ws = client().subscription_from_stream(BarGranularity::Minute1, stream::iter(ws_items));
    let poll = AlpacaLivePoll::new(Arc::new(EmptyFetcher), "BTC/USD".into(), BarGranularity::Minute1)
        .with_poll_interval(Duration::ZERO);
    LiveStream::new_for_test(Vec::new(), ws, poll)
}

/// Broker mock that records every submitted order and returns a fixed
/// fill price (and optional explicit fill size). Mirrors the recorder in
/// `eval_executor_live_real_broker_fills.rs`.
struct RecordingBroker {
    submitted: Mutex<Vec<OrderRequest>>,
    fill_price: f64,
    fill_size: Option<f64>,
    fee: Option<f64>,
}

impl RecordingBroker {
    fn new(fill_price: f64) -> Arc<Self> {
        Arc::new(Self {
            submitted: Mutex::new(Vec::new()),
            fill_price,
            fill_size: None,
            fee: None,
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
        Ok(0.0)
    }
}

/// Broker mock that always errors — drives the (d) broker-error exit.
struct ErrorBroker;

#[async_trait]
impl BrokerSurface for ErrorBroker {
    async fn submit_order(&self, _req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        Err(anyhow::anyhow!(
            "alpaca create_order: insufficient buying power for this order"
        ))
    }
    async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
        Ok(0.0)
    }
    async fn balance(&self) -> anyhow::Result<f64> {
        Ok(0.0)
    }
}

/// A fully-migrated `RunStore` backed by a temp `ApiContext`. The
/// `TempDir` MUST be kept alive for the DB file to persist, so it is
/// returned alongside the store. Using the real migrator (rather than
/// hand-applying a subset of `.sql` files) is required because Live runs
/// store a NULL `scenario_id`, which depends on the runtime rebuild of
/// `eval_runs` that migration 038's runtime migrator performs.
async fn fresh_store() -> (RunStore, tempfile::TempDir) {
    let (ctx, dir) = common::open_api_context().await;
    (RunStore::new(ctx.db.clone()), dir)
}

fn long_open_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.9,"justification":"live-loop-test"}"#,
    ))
}

fn build_strategy(agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "live-loop strategy".into(),
            plain_summary: "single-asset live loop coverage".into(),
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
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
    }
}

/// A live scenario shape (mirrors `api::eval::scenario_from_live_config`)
/// with a known initial capital. Built off a canonical scenario then
/// overridden so the test does not have to construct every field.
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
        strategy_id: "01TESTLIVELOOP".into(),
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
            // Large bar_limit so the natural exit in most tests is stream
            // end; individual tests override this to exercise the limit.
            bar_limit: Some(1_000),
            ..Default::default()
        },
        venue_label: VenueLabel::Paper,
        warmup_bars: Some(0),
        safety_limits: None,
        display_name: "live loop test".into(),
        description: None,
        tags: vec![],
        notes: None,
    }
}

/// Build a queued Live run + store + strategy + scenario, returning the
/// pieces a test drives the executor with. The `TempDir` keeps the
/// migrated DB alive for the test's lifetime.
async fn live_fixtures(initial: f64) -> (RunStore, Strategy, Scenario, Run, tempfile::TempDir) {
    let (store, dir) = fresh_store().await;
    let strategy = build_strategy("01TESTLIVELOOP");
    let scenario = live_scenario(initial);
    let mut run = Run::new_queued(strategy.manifest.id.clone(), String::new(), RunMode::Live);
    // Live runs must carry their LiveConfig (store invariant).
    run.live_config = Some(live_config());
    store.create(&run).await.unwrap();
    (store, strategy, scenario, run, dir)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn one_live_bar_drives_exactly_one_decision_through_the_broker() {
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = RecordingBroker::new(50_123.0);
    let stream = live_stream_from_bars(vec![market_bar_at(60, 50_000.0)]);

    let executor = Executor::live(
        &live_config(),
        broker.clone(),
        stream,
        WallClock::with_now_fn(|| ts(60)),
        None,
    )
    .expect("live executor builds");

    let metrics = executor
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
        .expect("live run completes on stream end");

    // Exactly one decision cycle.
    assert_eq!(metrics.n_decisions, 1, "one bar => one decision");
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 1, "exactly one decision row persisted");
    assert_eq!(decisions[0].action, "long_open");

    // The broker RECEIVED exactly one order with the expected shape.
    let submitted = broker.submitted();
    assert_eq!(submitted.len(), 1, "broker must see exactly one order");
    let order = &submitted[0];
    assert!(matches!(order.side, Side::Buy), "long_open => Buy");
    assert_eq!(order.asset, "BTC/USD");
    // size = risk_pct(balanced) * equity / reference. Just assert positive
    // and finite — exact risk_pct is a strategy-preset detail.
    assert!(order.size > 0.0 && order.size.is_finite(), "risk-sized qty");

    // The fill came from BROKER-REPORTED data (50_123.0), not a simulated
    // next-bar-open fill (which would be ~50_000 * (1 + slip)).
    assert_eq!(
        decisions[0].fill_price,
        Some(50_123.0),
        "fill price must be the broker's reported price, not a simulated bar fill",
    );
    assert!(metrics.n_trades >= 1, "the filled order counts as a trade");
}

#[tokio::test]
async fn live_run_requires_no_injected_bars_and_sources_from_stream() {
    // Three stream bars => three decisions, with zero injected/fixture
    // bars. This proves the live branch does not depend on
    // injected_asset_bars / the fixture loader.
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = RecordingBroker::new(50_000.0);
    let stream = live_stream_from_bars(vec![
        market_bar_at(60, 50_000.0),
        market_bar_at(120, 50_100.0),
        market_bar_at(180, 50_200.0),
    ]);

    let executor = Executor::live(
        &live_config(),
        broker.clone(),
        stream,
        WallClock::with_now_fn(|| ts(60)),
        None,
    )
    .unwrap();

    let metrics = executor
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
        .expect("live run completes from the stream alone");

    assert_eq!(metrics.n_decisions, 3, "three stream bars => three decisions");
    // First bar opens long; subsequent long_open while already long are
    // broker no-ops, so exactly one order reaches the broker.
    assert_eq!(
        broker.submitted().len(),
        1,
        "only the first long_open crosses the book; later ones are no-ops",
    );
}

#[tokio::test]
async fn live_loop_exits_on_bar_limit_stop_policy() {
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = RecordingBroker::new(50_000.0);
    // Five bars available, but bar_limit=2 must stop after two.
    let stream = live_stream_from_bars(vec![
        market_bar_at(60, 50_000.0),
        market_bar_at(120, 50_100.0),
        market_bar_at(180, 50_200.0),
        market_bar_at(240, 50_300.0),
        market_bar_at(300, 50_400.0),
    ]);

    let mut cfg = live_config();
    cfg.stop_policy = StopPolicy {
        bar_limit: Some(2),
        ..Default::default()
    };

    let executor = Executor::live(&cfg, broker, stream, WallClock::with_now_fn(|| ts(60)), None).unwrap();

    let metrics = executor
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
        .expect("live run completes at the bar limit");

    assert_eq!(metrics.n_decisions, 2, "bar_limit=2 => exactly two decisions");
}

#[tokio::test]
async fn live_loop_exits_on_decision_limit_stop_policy() {
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = RecordingBroker::new(50_000.0);
    let stream = live_stream_from_bars(vec![
        market_bar_at(60, 50_000.0),
        market_bar_at(120, 50_100.0),
        market_bar_at(180, 50_200.0),
    ]);

    let mut cfg = live_config();
    cfg.stop_policy = StopPolicy {
        decision_limit: Some(1),
        ..Default::default()
    };

    let executor = Executor::live(&cfg, broker, stream, WallClock::with_now_fn(|| ts(60)), None).unwrap();

    let metrics = executor
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
        .expect("live run completes at the decision limit");

    assert_eq!(metrics.n_decisions, 1, "decision_limit=1 => one decision");
}

#[tokio::test]
async fn live_loop_exits_cleanly_on_cancellation() {
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = RecordingBroker::new(50_000.0);
    let stream = live_stream_from_bars(vec![
        market_bar_at(60, 50_000.0),
        market_bar_at(120, 50_100.0),
    ]);

    // Cancel the run BEFORE the executor starts (the run row already
    // exists from `live_fixtures`). The executor's `begin_running` guard
    // and the loop's `is_terminal` check must short-circuit without
    // panicking / hanging / submitting orders.
    store.begin_running(&run.id).await.unwrap();
    store.cancel_active(&run.id, "operator cancel").await.unwrap();

    let executor = Executor::live(
        &live_config(),
        broker.clone(),
        stream,
        WallClock::with_now_fn(|| ts(60)),
        None,
    )
    .unwrap();

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

    // Cancellation surfaces as an Err (the run was already cancelled), and
    // crucially the loop did NOT submit any orders or hang.
    assert!(result.is_err(), "a cancelled run must not complete");
    assert!(
        broker.submitted().is_empty(),
        "no orders should reach the broker after cancellation",
    );
}

#[tokio::test]
async fn live_loop_surfaces_broker_error_as_run_failure() {
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = Arc::new(ErrorBroker);
    let stream = live_stream_from_bars(vec![market_bar_at(60, 50_000.0)]);

    let executor = Executor::live(
        &live_config(),
        broker,
        stream,
        WallClock::with_now_fn(|| ts(60)),
        None,
    )
    .unwrap();

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

    // The broker rejected the order; the run must FAIL (not hang, not
    // silently continue) with the classified broker error tag.
    let err = result.expect_err("broker error must fail the run");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("broker_insufficient_funds"),
        "failure must carry the classified broker error tag, got: {msg}",
    );

    // The decision row was still recorded (no fill) for the trace.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 1, "the rejected decision is still traced");
    assert!(
        decisions[0].fill_price.is_none(),
        "a rejected order produces no fill price",
    );
}
