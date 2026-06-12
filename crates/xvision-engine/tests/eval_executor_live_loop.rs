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
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
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
use xvision_filters::{parse_toml, Filter};

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
fn live_stream_with_warmup_for(asset: &str, warmup: Vec<MarketBar>, bars: Vec<MarketBar>) -> LiveStream {
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

fn sequence_dispatch(payloads: &[&str]) -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::sequence(
        payloads
            .iter()
            .map(|payload| LlmResponse {
                content: vec![ContentBlock::Text {
                    text: (*payload).to_string(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            })
            .collect(),
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

fn always_blocking_filter() -> Filter {
    parse_toml(
        r#"
[filter]
id = "f_live_loop_blocks"
strategy_id = "s_live_loop_blocks"
display_name = "Blocks normal prices"
asset_scope = ["BTC/USD"]
timeframe = "1m"
scan_cadence = "bar_close"
cooldown_bars = 0
wake_when_in_position = "always"
agent_context_template = "compact_trade_context_v1"

[[filter.conditions.all]]
lhs = "close"
op  = ">"
rhs = 999999.0
"#,
    )
    .expect("valid blocking filter")
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
async fn live_loop_honors_strategy_decision_cadence() {
    let (store, mut strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    strategy.manifest.decision_cadence_minutes = 60;
    let broker = RecordingBroker::new(50_000.0);
    let stream = single_asset_stream(vec![
        market_bar_at(0, 50_000.0),
        market_bar_at(60, 50_100.0),
        market_bar_at(120, 50_200.0),
    ]);

    let executor = Executor::live(
        &live_config(),
        broker.clone(),
        stream,
        WallClock::with_now_fn(|| ts(0)),
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

    assert_eq!(
        metrics.n_decisions, 1,
        "60-minute cadence on 1-minute bars should dispatch only on the cadence boundary",
    );
    assert_eq!(
        store.read_decisions(&run.id).await.unwrap().len(),
        1,
        "non-cadence bars must not persist decision rows",
    );
    assert_eq!(
        broker.submitted().len(),
        1,
        "non-cadence bars must not reach the broker",
    );
}

#[tokio::test]
async fn live_loop_honors_deterministic_filter_gate() {
    let (store, mut strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    strategy.activation_mode = xvision_filters::ActivationMode::FilterGated;
    strategy.filter = Some(always_blocking_filter());

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

    assert_eq!(metrics.n_decisions, 0, "blocked filter bars must not dispatch");
    assert!(
        store.read_decisions(&run.id).await.unwrap().is_empty(),
        "blocked filter bars must not persist decision rows",
    );
    assert!(
        broker.submitted().is_empty(),
        "blocked filter bars must not reach the broker",
    );
    let filter_events = store.read_filter_events(&run.id).await.unwrap();
    assert_eq!(
        filter_events.len(),
        2,
        "live filter evaluation should be persisted for each scanned bar",
    );
    assert!(
        filter_events.iter().all(|event| !event.triggered),
        "the blocking filter fixture should never trigger",
    );
}

#[tokio::test]
async fn live_loop_enforces_sltp_before_dispatch_and_counts_realized_round_trips() {
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = PerAssetRecordingBroker::new(&[("BTC/USD", 50_000.0)]);
    let stream = single_asset_stream(vec![
        MarketBar {
            timestamp: ts(60),
            open: 50_000.0,
            high: 50_500.0,
            low: 49_900.0,
            close: 50_000.0,
            volume: 1_000.0,
        },
        MarketBar {
            timestamp: ts(120),
            open: 50_000.0,
            high: 50_100.0,
            low: 49_000.0,
            close: 49_100.0,
            volume: 1_000.0,
        },
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
            sequence_dispatch(&[
                r#"{"action":"long_open","conviction":0.9,"justification":"open with stop","stop_loss_pct":1.0,"take_profit_pct":5.0}"#,
                r#"{"action":"hold","conviction":0.1,"justification":"this should not dispatch when stop fires"}"#,
            ]),
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await
        .expect("live run completes on stream end");

    let submitted = broker.submitted();
    assert_eq!(
        submitted.len(),
        2,
        "second bar should trigger a broker flat before consulting the model",
    );
    assert!(matches!(submitted[0].side, Side::Buy));
    assert!(matches!(submitted[1].side, Side::Sell));

    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(
        decisions.iter().map(|d| d.action.as_str()).collect::<Vec<_>>(),
        vec!["long_open", "stop_loss"],
        "stop-hit bar should record the deterministic SLTP exit, not the scripted hold",
    );
    assert_eq!(
        metrics.n_decisions, 2,
        "one model decision plus one deterministic SLTP decision should be counted",
    );
    assert_eq!(metrics.n_trades, 2, "open plus SLTP close should both count as fills");
    assert_eq!(
        metrics.win_rate, 0.0,
        "closed losing round trip should produce a non-null zero win_rate",
    );
}

#[tokio::test]
async fn live_loop_checks_sltp_on_non_cadence_bars() {
    let (store, mut strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    strategy.manifest.decision_cadence_minutes = 60;
    let broker = PerAssetRecordingBroker::new(&[("BTC/USD", 50_000.0)]);
    let stream = single_asset_stream(vec![
        MarketBar {
            timestamp: ts(0),
            open: 50_000.0,
            high: 50_500.0,
            low: 49_900.0,
            close: 50_000.0,
            volume: 1_000.0,
        },
        MarketBar {
            timestamp: ts(60),
            open: 50_000.0,
            high: 50_100.0,
            low: 49_000.0,
            close: 49_100.0,
            volume: 1_000.0,
        },
    ]);

    let executor = Executor::live(
        &live_config(),
        broker.clone(),
        stream,
        WallClock::with_now_fn(|| ts(0)),
        None,
    )
    .unwrap();

    let metrics = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            sequence_dispatch(&[
                r#"{"action":"long_open","conviction":0.9,"justification":"open with stop","stop_loss_pct":1.0,"take_profit_pct":5.0}"#,
                r#"{"action":"hold","conviction":0.1,"justification":"non-cadence stop should not consult this"}"#,
            ]),
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await
        .expect("live run completes on stream end");

    let submitted = broker.submitted();
    assert_eq!(
        submitted.len(),
        2,
        "SLTP must flatten on the non-cadence bar even though normal dispatch is skipped",
    );
    assert!(matches!(submitted[0].side, Side::Buy));
    assert!(matches!(submitted[1].side, Side::Sell));
    assert_eq!(
        store
            .read_decisions(&run.id)
            .await
            .unwrap()
            .iter()
            .map(|d| d.action.as_str())
            .collect::<Vec<_>>(),
        vec!["long_open", "stop_loss"],
    );
    assert_eq!(metrics.n_decisions, 2);
}

#[tokio::test]
async fn live_loop_applies_broker_min_notional_before_submit() {
    let (store, mut strategy, mut scenario, mut run, _dir) = live_fixtures(10_000.0).await;
    strategy.risk.risk_pct_per_trade = 0.000001;
    scenario.capital.initial = 10_000.0;
    let broker = RecordingBroker::new(100_000.0);
    let stream = single_asset_stream(vec![market_bar_at(60, 100_000.0)]);

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
        .expect("live run should complete when a deterministic min-notional veto fires");

    assert!(
        broker.submitted().is_empty(),
        "below-min-notional live orders must be vetoed before BrokerSurface::submit_order",
    );
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 1, "the trader intent should still be recorded");
    assert_eq!(decisions[0].action, "long_open");
    assert!(decisions[0].fill_price.is_none());
    assert!(decisions[0].fill_size.is_none());
    assert_eq!(metrics.n_trades, 0);
}

#[tokio::test]
async fn live_loop_applies_short_borrow_cost_on_realized_close() {
    let (store, strategy, mut scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    scenario.venue.fees.maker_bps = 0;
    scenario.venue.fees.taker_bps = 0;
    scenario.venue.borrow_bps_per_day = 1_000_000.0;
    let broker = RecordingBroker::new(50_000.0);
    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_000.0)]);

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
            sequence_dispatch(&[
                r#"{"action":"short_open","conviction":0.9,"justification":"open short"}"#,
                r#"{"action":"flat","conviction":0.9,"justification":"close short"}"#,
            ]),
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await
        .expect("live run completes on stream end");

    assert_eq!(broker.submitted().len(), 2, "short open and close should reach broker");
    assert_eq!(metrics.n_trades, 2);
    assert_eq!(metrics.win_rate, 0.0);
    assert!(
        metrics.total_return_pct < 0.0,
        "flat-price short close should still lose money once borrow cost is applied; got {}",
        metrics.total_return_pct,
    );
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
    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_500.0)]);

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

/// Per-asset variant of [`CancelAfterOpenBroker`]: records every order with
/// its asset + side, opens both legs, and cancels the run on the FIRST Buy so
/// the next loop iteration fires the cancel-close path while BOTH assets hold
/// an open long. Used by the multi-asset cancel test to prove BOTH legs are
/// flattened in one cancel.
struct PerAssetCancelAfterOpenBroker {
    submitted: Mutex<Vec<OrderRequest>>,
    open_fill_by_asset: std::collections::HashMap<String, f64>,
    close_fill_by_asset: std::collections::HashMap<String, f64>,
    cancel_hook: Mutex<Option<(RunStore, String)>>,
    cancelled: Mutex<bool>,
    /// Cancel only AFTER this many distinct assets have opened a long, so both
    /// legs are held when the cancel-close path fires (cancelling on the first
    /// Buy would short-circuit the loop before the second asset opens).
    cancel_after_open_assets: usize,
    opened_assets: Mutex<std::collections::BTreeSet<String>>,
}

impl PerAssetCancelAfterOpenBroker {
    fn new(opens: &[(&str, f64)], closes: &[(&str, f64)], cancel_after_open_assets: usize) -> Arc<Self> {
        Arc::new(Self {
            submitted: Mutex::new(Vec::new()),
            open_fill_by_asset: opens.iter().map(|(a, p)| ((*a).to_string(), *p)).collect(),
            close_fill_by_asset: closes.iter().map(|(a, p)| ((*a).to_string(), *p)).collect(),
            cancel_hook: Mutex::new(None),
            cancelled: Mutex::new(false),
            cancel_after_open_assets,
            opened_assets: Mutex::new(std::collections::BTreeSet::new()),
        })
    }
    fn arm(&self, store: RunStore, run_id: String) {
        *self.cancel_hook.lock().unwrap() = Some((store, run_id));
    }
    fn submitted(&self) -> Vec<OrderRequest> {
        self.submitted.lock().unwrap().clone()
    }
}

#[async_trait]
impl BrokerSurface for PerAssetCancelAfterOpenBroker {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let is_buy = matches!(req.side, Side::Buy);
        self.submitted.lock().unwrap().push(req.clone());
        // Cancel the run exactly once, only AFTER `cancel_after_open_assets`
        // distinct assets have opened a long. Cancelling on the first Buy
        // would short-circuit the loop before the second asset's bar is
        // processed; waiting until both legs are open guarantees the
        // cancel-close path has two legs to flatten.
        if is_buy {
            let distinct_opened = {
                let mut opened = self.opened_assets.lock().unwrap();
                opened.insert(req.asset.clone());
                opened.len()
            };
            if distinct_opened >= self.cancel_after_open_assets {
                let already = {
                    let mut c = self.cancelled.lock().unwrap();
                    let was = *c;
                    *c = true;
                    was
                };
                if !already {
                    let hook = self.cancel_hook.lock().unwrap().clone();
                    if let Some((store, run_id)) = hook {
                        store
                            .cancel_active(&run_id, "operator cancel mid-run")
                            .await
                            .unwrap();
                    }
                }
            }
        }
        let table = if is_buy {
            &self.open_fill_by_asset
        } else {
            &self.close_fill_by_asset
        };
        let fill_price = table.get(&req.asset).copied().unwrap_or(1.0);
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
async fn live_cancel_closes_open_positions_in_both_assets() {
    // Two assets (BTC + ETH), one opening bar each. Both legs open long, then
    // the run is cancelled mid-run. The next loop-top `is_terminal` check must
    // fire the A2 cancel-close path and flatten BOTH legs in one pass:
    //   - the broker receives two opening Buys then two closing Sells (one per
    //     asset);
    //   - two `flat` decision rows are recorded (one per asset);
    //   - each close's size matches the held position (the open size).
    let (store, strategy, scenario, mut run, _dir) = multi_asset_live_fixtures(100_000.0).await;
    let broker = PerAssetCancelAfterOpenBroker::new(
        &[("BTC/USD", 50_000.0), ("ETH/USD", 3_000.0)],
        &[("BTC/USD", 51_000.0), ("ETH/USD", 3_100.0)],
        2,
    );
    broker.arm(store.clone(), run.id.clone());

    let multi = MultiLiveStream::new(vec![
        (
            AssetSymbol::Btc,
            live_stream_for(
                "BTC/USD",
                vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_500.0)],
            ),
        ),
        (
            AssetSymbol::Eth,
            live_stream_for(
                "ETH/USD",
                vec![market_bar_at(60, 3_000.0), market_bar_at(120, 3_050.0)],
            ),
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

    // Cancellation still terminates the run.
    assert!(result.is_err(), "a cancelled run must not complete");
    assert!(
        store.is_cancelled(&run.id).await.unwrap(),
        "the run must end Cancelled",
    );

    // Group the broker orders by asset; each asset must have exactly ONE Buy
    // (the open) and ONE Sell (the cancel-close flatten).
    let submitted = broker.submitted();
    let buys: Vec<&OrderRequest> = submitted.iter().filter(|o| matches!(o.side, Side::Buy)).collect();
    let sells: Vec<&OrderRequest> = submitted
        .iter()
        .filter(|o| matches!(o.side, Side::Sell))
        .collect();
    assert_eq!(
        buys.len(),
        2,
        "both assets opened a long (two Buys), got {submitted:?}"
    );
    assert_eq!(
        sells.len(),
        2,
        "the cancel must flatten BOTH legs (two closing Sells), got {submitted:?}",
    );

    let buy_assets: std::collections::BTreeSet<&str> = buys.iter().map(|o| o.asset.as_str()).collect();
    let sell_assets: std::collections::BTreeSet<&str> = sells.iter().map(|o| o.asset.as_str()).collect();
    assert!(buy_assets.contains("BTC/USD") && buy_assets.contains("ETH/USD"));
    assert!(
        sell_assets.contains("BTC/USD") && sell_assets.contains("ETH/USD"),
        "both BTC and ETH must be flattened on cancel, got sells for {sell_assets:?}",
    );

    // Each close must flatten EXACTLY the held position: the Sell size equals
    // the matching Buy size for that asset.
    for asset in ["BTC/USD", "ETH/USD"] {
        let buy = buys.iter().find(|o| o.asset == asset).unwrap();
        let sell = sells.iter().find(|o| o.asset == asset).unwrap();
        assert!(
            (buy.size - sell.size).abs() < 1e-9,
            "{asset}: close size {} must match the held open size {}",
            sell.size,
            buy.size,
        );
    }

    // TWO `flat` decision rows recorded — one per asset.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let flats: Vec<_> = decisions.iter().filter(|d| d.action == "flat").collect();
    assert_eq!(
        flats.len(),
        2,
        "two flat close rows (one per asset) must be recorded on cancel, got {:?}",
        decisions
            .iter()
            .map(|d| (&d.asset, &d.action))
            .collect::<Vec<_>>(),
    );
    let flat_assets: std::collections::BTreeSet<&str> = flats.iter().map(|d| d.asset.as_str()).collect();
    assert!(
        flat_assets.contains("BTC/USD") && flat_assets.contains("ETH/USD"),
        "a flat row must exist for BOTH assets, got {flat_assets:?}",
    );
}

#[tokio::test]
async fn live_cancel_with_flat_book_makes_no_broker_calls() {
    // Cancel BEFORE any bar opens a position -> the book is flat, so the A2
    // close path must be a no-op: no broker orders, no flat decision rows.
    // This is the regression guard for the existing flat-cancel behavior.
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = CancelAfterOpenBroker::new(50_000.0, 51_000.0);
    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_500.0)]);

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

/// Broker that fills the opening Buy normally, then REJECTS the cancel-close
/// Sell with an error. Exercises the A2 cancel-close rejection branch: the
/// close fails, the leg must remain open (exposure visible), a warn note is
/// recorded, and the run still ends Cancelled — without panicking.
struct RejectCloseBroker {
    submitted: Mutex<Vec<OrderRequest>>,
    open_fill_price: f64,
    cancel_hook: Mutex<Option<(RunStore, String)>>,
}

impl RejectCloseBroker {
    fn new(open_fill_price: f64) -> Arc<Self> {
        Arc::new(Self {
            submitted: Mutex::new(Vec::new()),
            open_fill_price,
            cancel_hook: Mutex::new(None),
        })
    }
    fn arm(&self, store: RunStore, run_id: String) {
        *self.cancel_hook.lock().unwrap() = Some((store, run_id));
    }
    fn submitted(&self) -> Vec<OrderRequest> {
        self.submitted.lock().unwrap().clone()
    }
}

#[async_trait]
impl BrokerSurface for RejectCloseBroker {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let is_buy = matches!(req.side, Side::Buy);
        self.submitted.lock().unwrap().push(req.clone());
        if is_buy {
            // Fill the open, and cancel the run so the next loop iteration
            // fires the cancel-close path while a long is held.
            let hook = self.cancel_hook.lock().unwrap().clone();
            if let Some((store, run_id)) = hook {
                store
                    .cancel_active(&run_id, "operator cancel mid-run")
                    .await
                    .unwrap();
            }
            Ok(OrderConfirmation {
                broker_order_id: format!("recorded-{}", req.idempotency_key),
                fill_price: Some(self.open_fill_price),
                fill_size: req.size,
                fee: None,
            })
        } else {
            // REJECT the closing Sell — the broker refuses to flatten.
            Err(anyhow::anyhow!("alpaca create_order: order rejected by exchange"))
        }
    }
    async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
        Ok(0.0)
    }
    async fn balance(&self) -> anyhow::Result<f64> {
        Ok(0.0)
    }
}

#[tokio::test]
async fn live_cancel_close_rejection_retains_leg_and_warns_without_panicking() {
    // Bar 1 opens a long (broker fills it and cancels the run). Bar 2's
    // loop-top `is_terminal` check fires the A2 cancel-close path, which
    // submits a Sell to flatten — but the broker REJECTS the close. The core
    // safety guarantee: the executor must NOT panic, must leave the leg
    // RETAINED (no `flat` close row, no successful closing fill), must record
    // a `warn` supervisor note, and the run must still end Cancelled.
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = RejectCloseBroker::new(50_000.0);
    broker.arm(store.clone(), run.id.clone());
    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_500.0)]);

    let executor = Executor::live(
        &live_config(),
        broker.clone(),
        stream,
        WallClock::with_now_fn(|| ts(60)),
        None,
    )
    .unwrap();

    // Must not panic; the run returns an Err on the cancel path.
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
    // The run still ends Cancelled despite the failed close.
    assert!(
        store.is_cancelled(&run.id).await.unwrap(),
        "the run must end Cancelled even when the cancel-close was rejected",
    );

    // The broker saw the opening Buy AND the attempted closing Sell (which it
    // rejected). The Sell reaching the broker proves the flatten was tried.
    let submitted = broker.submitted();
    assert_eq!(
        submitted.len(),
        2,
        "expected an opening Buy then a (rejected) close Sell, got {submitted:?}",
    );
    assert!(matches!(submitted[0].side, Side::Buy), "bar-1 opens long");
    assert!(
        matches!(submitted[1].side, Side::Sell),
        "the cancel-close must attempt a Sell",
    );

    // The leg is RETAINED: the rejected close did NOT settle, so NO `flat`
    // close decision row exists (the exposure remains open). A successful
    // close would have recorded one.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert!(
        decisions
            .iter()
            .all(|d| d.action != "flat" && d.action != "flat_partial"),
        "a rejected close must NOT record a flat/partial close row (leg retained), got {:?}",
        decisions
            .iter()
            .map(|d| (&d.asset, &d.action))
            .collect::<Vec<_>>(),
    );

    // A `warn` supervisor note flags the dangling exposure.
    let notes = store.read_supervisor_notes(&run.id).await.unwrap();
    let warn = notes.iter().find(|(_role, severity, content)| {
        severity == "warn" && content.contains("BTC/USD") && content.contains("may remain open")
    });
    assert!(
        warn.is_some(),
        "a warn supervisor note must flag the un-flattened exposure, got notes {notes:?}",
    );
}

/// Broker that fills the opening Buy fully, cancels the run, then on the
/// cancel-close Sell reports a PARTIAL fill (`close_fill_size` < requested
/// size). The book settles realized PnL on the partial close and retains the
/// residual leg (recorded as `flat_partial`). Exercises the A2 cancel-close
/// PARTIAL branch: the equity/equity_curve recompute must fire on ANY close
/// fill (full OR partial), not only on a full flatten — otherwise the
/// cancelled run's persisted partial metrics use stale equity that ignores the
/// realized PnL from the partial close.
struct PartialCloseOnCancelBroker {
    submitted: Mutex<Vec<OrderRequest>>,
    open_fill_price: f64,
    close_fill_price: f64,
    /// Absolute units the broker actually fills on the close Sell. Must be
    /// strictly less than the requested size to produce a partial close.
    close_fill_size: f64,
    cancel_hook: Mutex<Option<(RunStore, String)>>,
}

impl PartialCloseOnCancelBroker {
    fn new(open_fill_price: f64, close_fill_price: f64, close_fill_size: f64) -> Arc<Self> {
        Arc::new(Self {
            submitted: Mutex::new(Vec::new()),
            open_fill_price,
            close_fill_price,
            close_fill_size,
            cancel_hook: Mutex::new(None),
        })
    }
    fn arm(&self, store: RunStore, run_id: String) {
        *self.cancel_hook.lock().unwrap() = Some((store, run_id));
    }
    fn submitted(&self) -> Vec<OrderRequest> {
        self.submitted.lock().unwrap().clone()
    }
}

#[async_trait]
impl BrokerSurface for PartialCloseOnCancelBroker {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        self.submitted.lock().unwrap().push(req.clone());
        let is_buy = matches!(req.side, Side::Buy);
        // On the opening Buy, cancel the run so the NEXT loop iteration fires
        // the cancel-close path while a long is held.
        if is_buy {
            let hook = self.cancel_hook.lock().unwrap().clone();
            if let Some((store, run_id)) = hook {
                store
                    .cancel_active(&run_id, "operator cancel mid-run")
                    .await
                    .unwrap();
            }
        }
        // Buy fills fully at open price; the close Sell fills only PARTIALLY
        // (reduced fill_size) so residual exposure remains -> flat_partial.
        if is_buy {
            Ok(OrderConfirmation {
                broker_order_id: format!("recorded-{}", req.idempotency_key),
                fill_price: Some(self.open_fill_price),
                fill_size: req.size,
                fee: None,
            })
        } else {
            Ok(OrderConfirmation {
                broker_order_id: format!("recorded-{}", req.idempotency_key),
                fill_price: Some(self.close_fill_price),
                fill_size: self.close_fill_size,
                fee: None,
            })
        }
    }
    async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
        Ok(0.0)
    }
    async fn balance(&self) -> anyhow::Result<f64> {
        Ok(0.0)
    }
}

#[tokio::test]
async fn live_cancel_partial_close_recomputes_equity_from_realized_pnl() {
    // Bar 1 opens a long (broker fills it fully and cancels the run). Bar 2's
    // loop-top `is_terminal` check fires the A2 cancel-close path, which
    // submits a Sell to flatten — but the broker only PARTIALLY fills the
    // close (residual exposure remains -> `flat_partial`). The partial close
    // still settles realized PnL on the book.
    //
    // REGRESSION: the cancel checkpoint must recompute `equity` + push to
    // `equity_curve` whenever ANY close fill lands (full OR partial), not only
    // when a leg fully flattens. With the old `closed > 0` gate a partial
    // close left `closed == 0`, so the recompute was skipped and the persisted
    // partial metrics used STALE equity (== initial, 0% return) that ignored
    // the realized PnL. Assert the persisted metrics reflect the realized PnL.
    let initial = 100_000.0;
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(initial).await;
    // Open at 50_000, close higher at 51_000 -> positive realized PnL. Fill
    // only a tiny sliver (0.001 units) on the close so it stays partial.
    let broker = PartialCloseOnCancelBroker::new(50_000.0, 51_000.0, 0.001);
    broker.arm(store.clone(), run.id.clone());
    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_500.0)]);

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
    assert!(
        store.is_cancelled(&run.id).await.unwrap(),
        "the run must end Cancelled",
    );

    // The broker saw the opening Buy then the cancel-close Sell.
    let submitted = broker.submitted();
    assert_eq!(
        submitted.len(),
        2,
        "expected an opening Buy then a cancel-close Sell, got {submitted:?}",
    );
    assert!(
        matches!(submitted[1].side, Side::Sell),
        "cancel flattens with a Sell"
    );

    // The close was PARTIAL: a `flat_partial` row (NOT a full `flat`) is
    // recorded, carrying the realized PnL from the portion that settled.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let partial_row = decisions
        .iter()
        .find(|d| d.action == "flat_partial")
        .expect("a flat_partial close row must be recorded on the partial cancel-close");
    let realized = partial_row
        .pnl_realized
        .expect("a partial close still settles realized PnL");
    assert!(
        realized > 0.0,
        "long 50_000 -> 51_000 must realize positive PnL even on a partial close, got {realized}",
    );

    // CRUX: the cancelled run's persisted PARTIAL metrics must reflect the
    // realized PnL from the partial close — equity was recomputed after the
    // partial fill, not left stale at the pre-close value. Stale equity would
    // leave `total_return_pct == 0` (final equity == initial); the realized
    // gain pushes it positive.
    let persisted = store.get(&run.id).await.unwrap();
    let metrics = persisted
        .metrics
        .expect("the cancelled run persists partial metrics");
    assert!(
        metrics.total_return_pct > 0.0,
        "partial-close cancel metrics must reflect realized PnL (equity recomputed), \
         got total_return_pct = {} (stale equity would be ~0)",
        metrics.total_return_pct,
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

// ---------------------------------------------------------------------------
// A3: one-shot "flatten positions" (flatten now, keep running)
// ---------------------------------------------------------------------------

/// Echoes `hold` on every cycle (the run never opens a position). Used by the
/// flat-book flatten no-op test.
fn hold_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.5,"justification":"live-loop-test-hold"}"#,
    ))
}

/// Broker that opens a long on its FIRST (Buy) submit and, on that SAME submit,
/// requests a one-shot flatten on `run_id` via a captured store handle (exactly
/// once — armed flag flips off so later Buys don't re-request). The NEXT loop
/// iteration's A3 flatten checkpoint then fires while a long is held, submitting
/// a Sell to flatten — recorded here as the second order — WITHOUT terminating
/// the run. Mirrors `CancelAfterOpenBroker` but requests a flatten instead of a
/// cancel.
struct FlattenAfterOpenBroker {
    submitted: Mutex<Vec<OrderRequest>>,
    open_fill_price: f64,
    close_fill_price: f64,
    flatten_hook: Mutex<Option<(RunStore, String)>>,
    requested: Mutex<bool>,
}

impl FlattenAfterOpenBroker {
    fn new(open_fill_price: f64, close_fill_price: f64) -> Arc<Self> {
        Arc::new(Self {
            submitted: Mutex::new(Vec::new()),
            open_fill_price,
            close_fill_price,
            flatten_hook: Mutex::new(None),
            requested: Mutex::new(false),
        })
    }
    /// Arm the flatten hook so the first Buy requests a flatten on `run_id`.
    fn arm(&self, store: RunStore, run_id: String) {
        *self.flatten_hook.lock().unwrap() = Some((store, run_id));
    }
    fn submitted(&self) -> Vec<OrderRequest> {
        self.submitted.lock().unwrap().clone()
    }
}

#[async_trait]
impl BrokerSurface for FlattenAfterOpenBroker {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let is_buy = matches!(req.side, Side::Buy);
        self.submitted.lock().unwrap().push(req.clone());
        // On the FIRST opening Buy, request a one-shot flatten so the NEXT loop
        // iteration sees `flatten_requested` while holding an open position.
        // Request exactly once so the reopen-after-flatten Buy doesn't re-arm.
        if is_buy {
            let already = {
                let mut r = self.requested.lock().unwrap();
                let was = *r;
                *r = true;
                was
            };
            if !already {
                let hook = self.flatten_hook.lock().unwrap().clone();
                if let Some((store, run_id)) = hook {
                    store.request_flatten(&run_id).await.unwrap();
                }
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
async fn live_flatten_closes_open_position_and_keeps_running() {
    // Bar 1 opens a long (and the broker requests a one-shot flatten on that
    // submit). Bar 2's loop-top A3 flatten checkpoint fires while a long is
    // held -> the shared close path submits a Sell to flatten the position
    // through the broker, records a `flat` decision row, CLEARS the flag, and
    // CONTINUES the run (the loop does NOT bail). The run then keeps deciding
    // (bar 2's long_open reopens) and exits cleanly on stream end (Ok).
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = FlattenAfterOpenBroker::new(50_000.0, 51_000.0);
    broker.arm(store.clone(), run.id.clone());
    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_500.0)]);

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

    // CRUCIAL: the run KEEPS RUNNING — a flatten does NOT terminate it. The
    // run completes normally on stream end (Ok), unlike the cancel path which
    // ends Err.
    assert!(
        result.is_ok(),
        "a flatten must NOT terminate the run; it should complete on stream end, got {result:?}",
    );
    assert!(
        !store.is_cancelled(&run.id).await.unwrap(),
        "a flatten must not cancel the run",
    );

    // The one-shot flag was cleared by the executor after flattening.
    assert!(
        !store.flatten_requested(&run.id).await.unwrap(),
        "the executor must clear flatten_requested after flattening (one-shot)",
    );

    // The broker saw the opening Buy, then the flatten-driven Sell that closed
    // the position. (Because the run keeps running, bar-2's long_open reopens
    // afterwards — a third Buy — proving the loop is still alive.)
    let submitted = broker.submitted();
    assert!(
        submitted.len() >= 2,
        "expected at least an opening Buy then a flatten Sell, got {submitted:?}",
    );
    assert!(matches!(submitted[0].side, Side::Buy), "bar-1 opens long");
    assert!(
        matches!(submitted[1].side, Side::Sell),
        "flatten must close the long with a Sell, got {:?}",
        submitted[1].side,
    );
    // The run kept running and reopened after the flatten (loop is alive).
    assert!(
        submitted.len() >= 3 && matches!(submitted[2].side, Side::Buy),
        "the run must keep running after flatten (bar-2 reopens with a Buy), got {submitted:?}",
    );

    // A `flat` closing decision row was recorded with the realized PnL from the
    // broker-reported close (50_000 -> 51_000 on the open units).
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let close_row = decisions
        .iter()
        .find(|d| d.action == "flat")
        .expect("a flat close decision must be recorded on flatten");
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
    // The flatten decision's justification is `flatten:`-prefixed (distinct
    // from the cancel path's `cancel:` prefix).
    assert!(
        close_row
            .justification
            .as_deref()
            .unwrap_or_default()
            .starts_with("flatten:"),
        "flatten decision must carry a flatten:-prefixed justification, got {:?}",
        close_row.justification,
    );
}

#[tokio::test]
async fn live_flatten_on_a_flat_book_makes_no_broker_calls() {
    // A flatten requested while the book holds NO positions must be a no-op:
    // the flatten checkpoint sees an empty book, submits nothing to the broker,
    // clears the flag, and the run keeps running. Regression guard mirroring
    // A2's `live_cancel_with_flat_book_makes_no_broker_calls`.
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = RecordingBroker::new(50_000.0);
    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_100.0)]);

    // Request a flatten BEFORE the run starts. The book is flat (no position
    // opened yet) and the dispatch only ever holds, so the flatten checkpoint
    // must never reach the broker.
    store.begin_running(&run.id).await.unwrap();
    store.request_flatten(&run.id).await.unwrap();

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
            hold_dispatch(),
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await;

    assert!(
        result.is_ok(),
        "a flatten on a flat book must be a no-op and the run completes on stream end, got {result:?}",
    );
    assert!(
        broker.submitted().is_empty(),
        "no orders should reach the broker: the book was flat and the dispatch only holds, got {:?}",
        broker.submitted(),
    );
    // The flag was still cleared (one-shot consumption even with nothing to do).
    assert!(
        !store.flatten_requested(&run.id).await.unwrap(),
        "flatten_requested must be cleared even when the book was already flat",
    );
    // No `flat` decision row was recorded (nothing was closed).
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert!(
        decisions.iter().all(|d| d.action != "flat"),
        "no flat decision row should be recorded for a flat-book flatten",
    );
}

/// Broker that, on its FIRST (Buy) submit, both PAUSES the run AND requests a
/// one-shot flatten on `run_id` (exactly once — armed flag flips off). This
/// reproduces the spec UX: the operator pauses with a position held, THEN
/// clicks Flatten. The NEXT loop iteration's A3 flatten checkpoint fires while
/// the run is PAUSED and a long is held, and MUST still submit the close Sell
/// through the broker — the A1 paused-submit-skip applies only to the
/// per-cycle decision submit, NOT to the flatten close path. Mirrors
/// `FlattenAfterOpenBroker` but also pauses the run.
struct PauseFlattenAfterOpenBroker {
    submitted: Mutex<Vec<OrderRequest>>,
    open_fill_price: f64,
    close_fill_price: f64,
    hook: Mutex<Option<(RunStore, String)>>,
    requested: Mutex<bool>,
}

impl PauseFlattenAfterOpenBroker {
    fn new(open_fill_price: f64, close_fill_price: f64) -> Arc<Self> {
        Arc::new(Self {
            submitted: Mutex::new(Vec::new()),
            open_fill_price,
            close_fill_price,
            hook: Mutex::new(None),
            requested: Mutex::new(false),
        })
    }
    /// Arm the hook so the first Buy pauses the run AND requests a flatten.
    fn arm(&self, store: RunStore, run_id: String) {
        *self.hook.lock().unwrap() = Some((store, run_id));
    }
    fn submitted(&self) -> Vec<OrderRequest> {
        self.submitted.lock().unwrap().clone()
    }
}

#[async_trait]
impl BrokerSurface for PauseFlattenAfterOpenBroker {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let is_buy = matches!(req.side, Side::Buy);
        self.submitted.lock().unwrap().push(req.clone());
        // On the FIRST opening Buy (position now held), pause the run AND
        // request a one-shot flatten, exactly once, so the NEXT loop iteration
        // sees BOTH `paused = true` AND `flatten_requested = true` while a long
        // is open. The flatten checkpoint must still reach the broker.
        if is_buy {
            let already = {
                let mut r = self.requested.lock().unwrap();
                let was = *r;
                *r = true;
                was
            };
            if !already {
                let hook = self.hook.lock().unwrap().clone();
                if let Some((store, run_id)) = hook {
                    store.set_paused(&run_id, true).await.unwrap();
                    store.request_flatten(&run_id).await.unwrap();
                }
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
async fn live_flatten_while_paused_still_closes_position_and_stays_paused() {
    // Primary real-world path (spec §2.7 UX): the operator PAUSES a live run
    // that holds an open position, THEN clicks [Flatten positions]. A1 makes a
    // paused run SKIP the normal per-cycle broker submit while continuing to
    // iterate; A3's flatten checkpoint closes open positions directly (NOT
    // through the per-cycle submit gate). So a flatten issued while paused MUST
    // still submit the broker close — the pause must NOT suppress it.
    //
    // Bar 1 opens a long; on that submit the broker sets BOTH `paused = true`
    // and `flatten_requested = true`. Bar 2's loop-top flatten checkpoint fires
    // while the run is paused and a long is held -> the shared close path
    // submits a Sell to flatten through the broker, records a `flat` decision
    // row, clears `flatten_requested`, and CONTINUES the run. Because the run
    // is now paused, bar 2's long_open is submit-skipped (no reopen Buy), so
    // the broker sees exactly two orders: the opening Buy and the flatten Sell.
    // The run is STILL PAUSED and STILL RUNNING (Ok on stream end, not
    // cancelled).
    let (store, strategy, scenario, mut run, _dir) = live_fixtures(100_000.0).await;
    let broker = PauseFlattenAfterOpenBroker::new(50_000.0, 51_000.0);
    broker.arm(store.clone(), run.id.clone());
    let stream = single_asset_stream(vec![market_bar_at(60, 50_000.0), market_bar_at(120, 50_500.0)]);

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

    // The run KEEPS RUNNING — a flatten does NOT terminate it, even when paused.
    // It completes normally on stream end (Ok), unlike the cancel path (Err).
    assert!(
        result.is_ok(),
        "a flatten-while-paused must NOT terminate the run; it should complete on stream end, got {result:?}",
    );
    assert!(
        !store.is_cancelled(&run.id).await.unwrap(),
        "a flatten-while-paused must not cancel the run",
    );

    // STILL PAUSED: the flatten consumes its own one-shot flag but leaves the
    // independent A1 pause flag untouched. The run iterated to stream end while
    // paused.
    assert!(
        store.is_paused(&run.id).await.unwrap(),
        "the run must REMAIN paused after the flatten (flatten and pause are independent flags)",
    );

    // The one-shot flatten flag WAS cleared by the executor after flattening.
    assert!(
        !store.flatten_requested(&run.id).await.unwrap(),
        "the executor must clear flatten_requested after flattening (one-shot), even while paused",
    );

    // CRUX: the broker DID receive the flattening close even though the run was
    // paused. The paused-submit-skip suppresses bar-2's per-cycle long_open
    // reopen, so the broker sees EXACTLY two orders: the opening Buy, then the
    // flatten-driven Sell. There is NO third reopen Buy (it was paused-skipped).
    let submitted = broker.submitted();
    assert_eq!(
        submitted.len(),
        2,
        "expected exactly an opening Buy then a flatten Sell (no paused reopen), got {submitted:?}",
    );
    assert!(matches!(submitted[0].side, Side::Buy), "bar-1 opens long");
    assert!(
        matches!(submitted[1].side, Side::Sell),
        "the flatten close MUST reach the broker as a Sell even while paused, got {:?}",
        submitted[1].side,
    );
    assert!(
        (submitted[0].size - submitted[1].size).abs() < 1e-9,
        "the flatten close must flatten exactly the open size",
    );

    // A `flat` closing decision row was recorded with the realized PnL from the
    // broker-reported close (50_000 -> 51_000 on the open units).
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let close_row = decisions
        .iter()
        .find(|d| d.action == "flat")
        .expect("a flat close decision must be recorded on flatten-while-paused");
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
    assert!(
        close_row
            .justification
            .as_deref()
            .unwrap_or_default()
            .starts_with("flatten:"),
        "flatten decision must carry a flatten:-prefixed justification, got {:?}",
        close_row.justification,
    );
}
