//! Hermetic integration coverage for the live loop — §3 single-asset (L1)
//! and §4 multi-asset fanout (L2). No network, no Node sidecar.
//!
//! Wires [`LiveStream::new_for_test`] sub-streams (mock websocket
//! subscription + scripted poll) into a [`MultiLiveStream`], hands that to
//! [`Executor::live`] alongside a recording [`BrokerSurface`] mock through
//! `RealBrokerFills`, and asserts:
//!
//!   - a mock stream emitting one bar drives exactly one decision cycle;
//!   - the mock broker RECEIVES the expected order (action / asset / size);
//!   - NO injected bars are required — the run starts from the stream;
//!   - fills come from broker-reported data, not simulated bar fills;
//!   - the loop exits cleanly on (a) stream end, (b) cancellation,
//!     (c) a broker error — without hanging or panicking;
//!   - a pyramid-blocked `hold` preserves the open position (no close);
//!   - §4: a 2-asset MultiLiveStream drives BOTH assets, with per-asset
//!     decision isolation, one shared pooled NAV, a single monotonic
//!     decision-index series, and a sub-stream that closes early does not
//!     halt the others.
//!
//! A single-asset run is driven through a 1-element `MultiLiveStream`,
//! which the executor consumes byte-identically to the L1 single
//! `LiveStream` — `single_asset_stream()` wraps that.

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

use xvision_core::trading::AssetSymbol;
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::eval::executor::{Executor, LiveStream, MultiLiveStream, RunExecutor, WallClock};
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

/// Build a [`LiveStream`] for `asset` that emits the given websocket bars
/// then closes (no warmup, poll returns `Empty`).
fn live_stream_for(asset: &str, bars: Vec<MarketBar>) -> LiveStream {
    live_stream_with_warmup_for(asset, Vec::new(), bars)
}

/// Build a [`LiveStream`] with historical warmup bars and websocket live
/// bars. Warmup must seed history only; it must not produce decisions.
fn live_stream_with_warmup_for(
    asset: &str,
    warmup: Vec<MarketBar>,
    bars: Vec<MarketBar>,
) -> LiveStream {
    let ws_items: Vec<LiveBarItem> = bars.into_iter().map(LiveBarItem::Bar).collect();
    let ws = client().subscription_from_stream(BarGranularity::Minute1, stream::iter(ws_items));
    let poll = AlpacaLivePoll::new(Arc::new(EmptyFetcher), asset.into(), BarGranularity::Minute1)
        .with_poll_interval(Duration::ZERO);
    LiveStream::new_for_test(
        warmup
            .into_iter()
            .map(|b| xvision_core::market::Ohlcv {
                timestamp: b.timestamp,
                open: b.open,
                high: b.high,
                low: b.low,
                close: b.close,
                volume: b.volume,
            })
            .collect(),
        ws,
        poll,
    )
}

/// Single-asset (BTC) [`MultiLiveStream`] — a 1-element fanout, which the
/// executor consumes exactly like the L1 single `LiveStream`. This is what
/// the single-asset live loop tests drive.
fn single_asset_stream(bars: Vec<MarketBar>) -> MultiLiveStream {
    MultiLiveStream::new(vec![(AssetSymbol::Btc, live_stream_for("BTC/USD", bars))])
}

/// Broker mock that records orders PER ASSET and returns a per-asset fixed
/// fill price. Used by the multi-asset fanout test to prove BTC and ETH each
/// reach the broker with their own price.
struct PerAssetRecordingBroker {
    submitted: Mutex<Vec<OrderRequest>>,
    fill_price_by_asset: std::collections::HashMap<String, f64>,
}

impl PerAssetRecordingBroker {
    fn new(prices: &[(&str, f64)]) -> Arc<Self> {
        Arc::new(Self {
            submitted: Mutex::new(Vec::new()),
            fill_price_by_asset: prices.iter().map(|(a, p)| ((*a).to_string(), *p)).collect(),
        })
    }
    fn submitted(&self) -> Vec<OrderRequest> {
        self.submitted.lock().unwrap().clone()
    }
}

#[async_trait]
impl BrokerSurface for PerAssetRecordingBroker {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        self.submitted.lock().unwrap().push(req.clone());
        let price = self.fill_price_by_asset.get(&req.asset).copied().unwrap_or(1.0);
        Ok(OrderConfirmation {
            broker_order_id: format!("recorded-{}", req.idempotency_key),
            fill_price: Some(price),
            fill_size: req.size,
            fee: None,
        })
    }
    async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
        Ok(0.0)
    }
    async fn balance(&self) -> anyhow::Result<f64> {
        Ok(0.0)
    }
}

/// Build a strategy over a two-asset (BTC + ETH) universe for the
/// multi-asset live fanout test.
fn build_multi_asset_strategy(agent_id: &str) -> Strategy {
    let mut s = build_strategy(agent_id);
    s.manifest.asset_universe = vec!["BTC/USD".into(), "ETH/USD".into()];
    s
}

/// A two-asset LiveConfig (BTC + ETH).
fn multi_asset_live_config() -> LiveConfig {
    let mut cfg = live_config();
    cfg.assets = vec![
        AssetRef {
            class: AssetClass::Crypto,
            symbol: "BTC/USD".into(),
            venue_symbol: "BTC/USD".into(),
        },
        AssetRef {
            class: AssetClass::Crypto,
            symbol: "ETH/USD".into(),
            venue_symbol: "ETH/USD".into(),
        },
    ];
    cfg
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
        decision_mode: Default::default(),
        mechanistic_config: None,
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
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();
    (store, strategy, scenario, run, dir)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn one_live_bar_drives_exactly_one_decision_through_the_broker() {
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = RecordingBroker::new(50_123.0);
    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0)]);

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
async fn live_warmup_seeds_history_without_trading_historical_bars() {
    let (store, strategy, mut scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    scenario.warmup_bars = 2;
    let broker = RecordingBroker::new(50_123.0);
    let stream = MultiLiveStream::new(vec![(
        AssetSymbol::Btc,
        live_stream_with_warmup_for(
            "BTC/USD",
            vec![market_bar_at(60, 49_800.0), market_bar_at(120, 49_900.0)],
            vec![market_bar_at(180, 50_000.0)],
        ),
    )]);

    let executor = Executor::live(
        &live_config(),
        broker.clone(),
        stream,
        WallClock::with_now_fn(|| ts(180)),
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

    assert_eq!(
        metrics.n_decisions, 1,
        "warmup bars must not be tradable decisions"
    );
    assert_eq!(store.read_decisions(&run.id).await.unwrap().len(), 1);
    assert_eq!(
        broker.submitted().len(),
        1,
        "only the live bar reaches the broker"
    );
}

#[tokio::test]
async fn live_run_requires_no_injected_bars_and_sources_from_stream() {
    // Three stream bars => three decisions, with zero injected/fixture
    // bars. This proves the live branch does not depend on
    // injected_asset_bars / the fixture loader.
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = RecordingBroker::new(50_000.0);
    let stream = single_asset_stream(vec![
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
    let stream = single_asset_stream(vec![
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
    let stream = single_asset_stream(vec![
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
    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_100.0)]);

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

/// Broker that opens a long on its first (Buy) submit, and on that SAME
/// submit cancels the run via a captured store handle. The next loop
/// iteration's `is_terminal` checkpoint then fires the A2 close-on-cancel
/// path, which submits a Sell to flatten — recorded here as the second
/// order. Every fill is reported with a per-call fill price so the close's
/// realized PnL is observable.
struct CancelAfterOpenBroker {
    submitted: Mutex<Vec<OrderRequest>>,
    open_fill_price: f64,
    close_fill_price: f64,
    cancel_hook: Mutex<Option<(RunStore, String)>>,
}

impl CancelAfterOpenBroker {
    fn new(open_fill_price: f64, close_fill_price: f64) -> Arc<Self> {
        Arc::new(Self {
            submitted: Mutex::new(Vec::new()),
            open_fill_price,
            close_fill_price,
            cancel_hook: Mutex::new(None),
        })
    }
    /// Arm the cancel hook so the first Buy cancels `run_id` on `store`.
    fn arm(&self, store: RunStore, run_id: String) {
        *self.cancel_hook.lock().unwrap() = Some((store, run_id));
    }
    fn submitted(&self) -> Vec<OrderRequest> {
        self.submitted.lock().unwrap().clone()
    }
}

#[async_trait]
impl BrokerSurface for CancelAfterOpenBroker {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let is_buy = matches!(req.side, Side::Buy);
        self.submitted.lock().unwrap().push(req.clone());
        // On the opening Buy, cancel the run so the NEXT loop iteration sees a
        // cancelled run while holding an open position.
        if is_buy {
            let hook = self.cancel_hook.lock().unwrap().clone();
            if let Some((store, run_id)) = hook {
                store
                    .cancel_active(&run_id, "operator cancel mid-run")
                    .await
                    .unwrap();
            }
        }
        let fill_price = if is_buy {
            self.open_fill_price
        } else {
            self.close_fill_price
        };
        Ok(OrderConfirmation {
            broker_order_id: format!("recorded-{}", req.idempotency_key),
            fill_price: Some(fill_price),
            fill_size: req.size,
            fee: None,
        })
    }
    async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
        Ok(0.0)
    }
    async fn balance(&self) -> anyhow::Result<f64> {
        Ok(0.0)
    }
}

#[tokio::test]
async fn live_cancel_closes_open_positions_through_the_broker() {
    // Bar 1 opens a long (and the broker cancels the run on that submit).
    // Bar 2's loop-top `is_terminal` check fires while a long is held -> the
    // A2 close-on-cancel path must submit a Sell to flatten the position
    // through the broker and record the closing fill (realized PnL settled).
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    // Open at 50_000, close higher at 51_000 so the realized PnL is positive
    // and observable on the closing decision row.
    let broker = CancelAfterOpenBroker::new(50_000.0, 51_000.0);
    broker.arm(store.clone(), run.id.clone());
    let stream =
        single_asset_stream(vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_500.0)]);

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

    // Cancellation still terminates the run (Err on the cancel path).
    assert!(result.is_err(), "a cancelled run must not complete");

    // The broker saw TWO orders: the opening Buy, then the cancel-driven
    // Sell that flattened the position.
    let submitted = broker.submitted();
    assert_eq!(
        submitted.len(),
        2,
        "expected an opening Buy then a cancel-close Sell, got {submitted:?}",
    );
    assert!(matches!(submitted[0].side, Side::Buy), "bar-1 opens long");
    assert!(
        matches!(submitted[1].side, Side::Sell),
        "cancel must flatten the long with a Sell",
    );
    assert!(
        (submitted[0].size - submitted[1].size).abs() < 1e-9,
        "the close must flatten exactly the open size",
    );

    // A `flat` closing decision row was recorded with the realized PnL from
    // the broker-reported close (50_000 -> 51_000 on the open units).
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let close_row = decisions
        .iter()
        .find(|d| d.action == "flat")
        .expect("a flat close decision must be recorded on cancel");
    assert_eq!(
        close_row.fill_price,
        Some(51_000.0),
        "close fill price comes from the broker's reported close price",
    );
    let realized = close_row.pnl_realized.expect("close settles realized PnL");
    assert!(
        realized > 0.0,
        "long 50_000 -> 51_000 must realize positive PnL, got {realized}",
    );
}

#[tokio::test]
async fn live_cancel_with_flat_book_makes_no_broker_calls() {
    // Cancel BEFORE any bar opens a position -> the book is flat, so the A2
    // close path must be a no-op: no broker orders, no flat decision rows.
    // This is the regression guard for the existing flat-cancel behavior.
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = CancelAfterOpenBroker::new(50_000.0, 51_000.0);
    let stream =
        single_asset_stream(vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_500.0)]);

    // Cancel up front; no position is ever opened.
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

    assert!(result.is_err(), "a cancelled run must not complete");
    assert!(
        broker.submitted().is_empty(),
        "a flat cancel must make NO broker calls (no dangling positions to close)",
    );
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert!(
        decisions.iter().all(|d| d.action != "flat"),
        "no close decision rows when the book is already flat",
    );
}

#[tokio::test]
async fn live_loop_surfaces_broker_error_as_run_failure() {
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = Arc::new(ErrorBroker);
    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0)]);

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

// ---------------------------------------------------------------------------
// §3-review nit: hold-preservation (pyramid-block rewrite must NOT close)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pyramid_block_rewrites_to_hold_and_preserves_the_open_position() {
    // Bar 1: trader emits long_open while flat -> opens long (one order).
    // Bar 2: trader emits long_open again while ALREADY long -> the pyramid
    // guardrail rewrites it to `hold`. The L1 hold short-circuit must NOT
    // forward `hold` to the broker (which would otherwise classify it as
    // want_flat and CLOSE the position). Assert: exactly one order ever
    // reaches the broker (the bar-1 open), and the open long position is
    // preserved (bar-2 records no fill, no close).
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = RecordingBroker::new(50_000.0);
    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_100.0)]);

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
        .expect("live run completes on stream end");

    assert_eq!(metrics.n_decisions, 2, "two bars => two decisions");

    // Exactly ONE order reached the broker — the bar-1 long_open. The
    // bar-2 long_open was guardrail-rewritten to hold and short-circuited
    // BEFORE the broker, so it produced no close/flatten order.
    let submitted = broker.submitted();
    assert_eq!(
        submitted.len(),
        1,
        "only the bar-1 open crosses the broker; the pyramid-blocked hold must not submit",
    );
    assert!(
        matches!(submitted[0].side, Side::Buy),
        "the single order is the long open (Buy)",
    );

    // The bar-2 decision recorded NO fill (the hold preserved the position
    // rather than closing it).
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 2);
    assert_eq!(decisions[0].action, "long_open");
    assert!(
        decisions[0].fill_price.is_some(),
        "bar-1 open fills at the broker price",
    );
    // The bar-2 row's action is the guardrail-rewritten value is not
    // surfaced as `action` (the trader's original action is recorded), but
    // crucially it produced NO fill — the open position is intact.
    assert!(
        decisions[1].fill_price.is_none(),
        "bar-2 pyramid-block hold must not fill (position preserved, not closed)",
    );
    // Only the bar-1 open counts as a trade.
    assert_eq!(metrics.n_trades, 1, "exactly one fill across the run");
}

// ---------------------------------------------------------------------------
// §4 L2: multi-asset live fanout
// ---------------------------------------------------------------------------

/// A queued multi-asset (BTC + ETH) Live run + store + strategy + scenario.
async fn multi_asset_live_fixtures(initial: f64) -> (RunStore, Strategy, Scenario, Run, tempfile::TempDir) {
    let (store, dir) = fresh_store().await;
    let strategy = build_multi_asset_strategy("01TESTLIVEMULTI");
    let scenario = live_scenario(initial);
    let mut run = Run::new_queued(strategy.manifest.id.clone(), String::new(), RunMode::Live);
    run.live_config = Some(multi_asset_live_config());
    store.create(&run).await.unwrap();
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();
    (store, strategy, scenario, run, dir)
}

#[tokio::test]
async fn multi_asset_fanout_both_assets_decide_and_order_with_per_asset_isolation() {
    // Two assets, one bar each. With per-asset open-direction memory, BOTH
    // BTC and ETH open their own long leg, so BOTH reach the broker. If the
    // guardrail flip-memory were shared across assets (a bleed), ETH's
    // long_open would be rewritten to `hold` because BTC already opened
    // long — and only ONE order would reach the broker. Two orders proves
    // isolation.
    let (store, strategy, scenario, mut run, _dir) = multi_asset_live_fixtures(100_000.0).await;
    let broker = PerAssetRecordingBroker::new(&[("BTC/USD", 50_000.0), ("ETH/USD", 3_000.0)]);

    let multi = MultiLiveStream::new(vec![
        (
            AssetSymbol::Btc,
            live_stream_for("BTC/USD", vec![market_bar_at(60, 50_000.0)]),
        ),
        (
            AssetSymbol::Eth,
            live_stream_for("ETH/USD", vec![market_bar_at(60, 3_000.0)]),
        ),
    ]);

    let executor = Executor::live(
        &multi_asset_live_config(),
        broker.clone(),
        multi,
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
        .expect("multi-asset live run completes on stream end");

    // Two bars total (one per asset) => two decisions.
    assert_eq!(metrics.n_decisions, 2, "one bar per asset => two decisions");

    // BOTH assets produced a decision row.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 2, "two decision rows (one per asset)");
    let assets: std::collections::BTreeSet<&str> = decisions.iter().map(|d| d.asset.as_str()).collect();
    assert!(assets.contains("BTC/USD"), "BTC must have decided");
    assert!(assets.contains("ETH/USD"), "ETH must have decided");

    // Decision indices are a single monotonic counter shared across assets
    // (matching the backtest), so they are distinct — no (run_id,
    // decision_index) PK collision.
    let mut idxs: Vec<u32> = decisions.iter().map(|d| d.decision_index).collect();
    idxs.sort_unstable();
    assert_eq!(
        idxs,
        vec![0, 1],
        "shared monotonic decision indices, no collision"
    );

    // PER-ASSET ISOLATION: both legs opened (two orders to the broker), one
    // per asset. A shared flip-memory would have blocked the second.
    let submitted = broker.submitted();
    assert_eq!(submitted.len(), 2, "both assets' opens reached the broker");
    let order_assets: std::collections::BTreeSet<&str> = submitted.iter().map(|o| o.asset.as_str()).collect();
    assert!(order_assets.contains("BTC/USD"));
    assert!(order_assets.contains("ETH/USD"));
    // No simulated-fill fallback: fills carry the BROKER-reported per-asset
    // prices (50_000 for BTC, 3_000 for ETH), not a bar-derived price.
    for d in &decisions {
        let expected = if d.asset == "ETH/USD" { 3_000.0 } else { 50_000.0 };
        assert_eq!(
            d.fill_price,
            Some(expected),
            "{} must fill at the broker-reported price (no simulated fallback)",
            d.asset,
        );
    }

    // ONE pooled NAV series, keyed by timestamp. Both assets shared bar
    // ts=60, so the upsert collapses them to a SINGLE equity row at that
    // timestamp (no PK collision, no double series).
    let curve = store.read_equity_curve(&run.id).await.unwrap();
    assert_eq!(
        curve.len(),
        1,
        "two assets at the same bar timestamp => one pooled equity row",
    );
}

#[tokio::test]
async fn multi_asset_fanout_continues_when_one_substream_ends_early() {
    // BTC emits one bar then closes; ETH emits three. The merged stream
    // must keep yielding ETH's bars after BTC's sub-stream closes (a closed
    // sub-stream is dropped, not a stop condition). Run ends only when ALL
    // sub-streams have closed.
    let (store, strategy, scenario, mut run, _dir) = multi_asset_live_fixtures(100_000.0).await;
    let broker = PerAssetRecordingBroker::new(&[("BTC/USD", 50_000.0), ("ETH/USD", 3_000.0)]);

    let multi = MultiLiveStream::new(vec![
        (
            AssetSymbol::Btc,
            live_stream_for("BTC/USD", vec![market_bar_at(60, 50_000.0)]),
        ),
        (
            AssetSymbol::Eth,
            live_stream_for(
                "ETH/USD",
                vec![
                    market_bar_at(60, 3_000.0),
                    market_bar_at(120, 3_010.0),
                    market_bar_at(180, 3_020.0),
                ],
            ),
        ),
    ]);

    let executor = Executor::live(
        &multi_asset_live_config(),
        broker,
        multi,
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
        .expect("run completes only after ALL sub-streams close");

    // 1 BTC bar + 3 ETH bars = 4 decisions. The early-closed BTC stream did
    // not stop the run; ETH's later bars still produced decisions.
    assert_eq!(
        metrics.n_decisions, 4,
        "BTC(1) + ETH(3) bars => 4 decisions; the early BTC close must not halt the run",
    );
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let eth_decisions = decisions.iter().filter(|d| d.asset == "ETH/USD").count();
    let btc_decisions = decisions.iter().filter(|d| d.asset == "BTC/USD").count();
    assert_eq!(btc_decisions, 1, "BTC decided once before its stream closed");
    assert_eq!(eth_decisions, 3, "ETH kept deciding after BTC closed");
}
