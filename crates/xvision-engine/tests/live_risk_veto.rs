//! Integration test for T1.4: R3 risk-veto block in the live executor path.
//!
//! Covers the `max_concurrent_positions` check inside `decide_one_live`:
//! a two-asset strategy with `max_concurrent_positions = 1` emits a
//! `long_open` on BTC first (fills), then a `long_open` on ETH (vetoed
//! because one position is already open).  The second open is rewritten to
//! `hold`, the broker receives only one order, and a `supervisor_notes` row
//! records the reason `max_concurrent_positions`.

mod common;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use futures::stream;

use xvision_core::Capital;
use xvision_data::alpaca::{BarGranularity, MarketBar};
use xvision_data::alpaca_live::{AlpacaLiveClient, AlpacaLiveCredentials, LiveBarItem};
use xvision_data::alpaca_live_poll::{AlpacaLivePoll, AlpacaPollError, LivePollFetcher};
use xvision_execution::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest};

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
// Helpers shared with other live-loop tests (copied locally to avoid
// depending on test-only internal visibility)
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

fn alpaca_client() -> AlpacaLiveClient {
    AlpacaLiveClient::new(AlpacaLiveCredentials {
        key_id: "test".into(),
        secret_key: "test".into(),
    })
}

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

fn live_stream_for(asset: &str, bars: Vec<MarketBar>) -> LiveStream {
    let ws_items: Vec<LiveBarItem> = bars.into_iter().map(LiveBarItem::Bar).collect();
    let ws = alpaca_client().subscription_from_stream(BarGranularity::Minute1, stream::iter(ws_items));
    let poll = AlpacaLivePoll::new(Arc::new(EmptyFetcher), asset.into(), BarGranularity::Minute1)
        .with_poll_interval(Duration::ZERO);
    LiveStream::new_for_test(vec![], ws, poll)
}

// ---------------------------------------------------------------------------
// Simple recording broker
// ---------------------------------------------------------------------------

use std::sync::Mutex;

struct RecordingBroker {
    submitted: Mutex<Vec<OrderRequest>>,
    /// Per-order fill prices, consumed front-to-back; the LAST price is
    /// sticky (reused for every order past the end of the queue).
    fill_prices: Mutex<Vec<f64>>,
}

impl RecordingBroker {
    fn new(fill_price: f64) -> Arc<Self> {
        Self::with_prices(vec![fill_price])
    }
    /// Broker whose Nth order fills at `prices[N]` (last price sticky).
    fn with_prices(prices: Vec<f64>) -> Arc<Self> {
        assert!(!prices.is_empty(), "need at least one fill price");
        Arc::new(Self {
            submitted: Mutex::new(Vec::new()),
            fill_prices: Mutex::new(prices),
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
        let fill_price = {
            let mut prices = self.fill_prices.lock().unwrap();
            if prices.len() > 1 {
                prices.remove(0)
            } else {
                prices[0]
            }
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

// ---------------------------------------------------------------------------
// Strategy / scenario / store fixtures
// ---------------------------------------------------------------------------

async fn fresh_store() -> (RunStore, tempfile::TempDir) {
    let (ctx, dir) = common::open_api_context().await;
    (RunStore::new(ctx.db.clone()), dir)
}

/// Two-asset strategy (BTC + ETH) with `max_concurrent_positions = 1`.
fn two_asset_strategy_max1() -> Strategy {
    let mut risk = RiskPreset::Balanced.expand();
    risk.max_concurrent_positions = 1;
    Strategy {
        manifest: PublicManifest {
            id: "01TESTLIVERISKVETO0000001".into(),
            display_name: "live risk-veto test".into(),
            plain_summary: "max_concurrent_positions=1 live veto".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into(), "ETH/USD".into()],
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
        risk,
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

fn two_asset_live_config() -> LiveConfig {
    LiveConfig {
        strategy_id: "01TESTLIVERISKVETO0000001".into(),
        assets: vec![
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
        ],
        capital: Capital {
            initial: 100_000.0,
            currency: "USD".into(),
        },
        broker_creds_ref: "alpaca".into(),
        stop_policy: StopPolicy {
            bar_limit: Some(1_000),
            ..Default::default()
        },
        granularity: BarGranularity::Minute1,
        venue_label: VenueLabel::Paper,
        warmup_bars: Some(0),
        safety_limits: None,
        display_name: "live risk-veto test".into(),
        description: None,
        tags: vec![],
        notes: None,
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

fn long_open_dispatch() -> Arc<dyn LlmDispatch> {
    Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.9,"justification":"go long"}"#,
    ))
}

/// One canned text response per decision; the LAST response is sticky.
fn action_sequence_dispatch(actions: &[&str]) -> Arc<dyn LlmDispatch> {
    use xvision_engine::agent::llm::{ContentBlock, LlmResponse, StopReason};
    let responses = actions
        .iter()
        .map(|action| LlmResponse {
            content: vec![ContentBlock::Text {
                text: format!(r#"{{"action":"{action}","conviction":0.9,"justification":"scripted"}}"#),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        })
        .collect();
    Arc::new(MockDispatch::sequence(responses))
}

/// Single-asset strategy (BTC) tuned for the daily-loss-kill veto:
/// `daily_loss_kill_pct = 0.05` (5% of starting capital), generous
/// per-trade sizing so a single losing round-trip breaches the kill
/// threshold, and `max_concurrent_positions = 0` (disabled) so the test
/// isolates the daily-loss check.
fn one_asset_strategy_daily_kill() -> Strategy {
    let mut strategy = two_asset_strategy_max1();
    strategy.manifest.id = "01TESTLIVERISKVETO0000002".into();
    strategy.manifest.asset_universe = vec!["BTC/USD".into()];
    strategy.risk.max_concurrent_positions = 0; // disabled — isolate daily-loss
    strategy.risk.daily_loss_kill_pct = 0.05; // 5% of 100k = 5_000 USD
    strategy.risk.risk_pct_per_trade = 0.10; // 10k notional @ 50k = 0.2 units
    strategy
}

fn one_asset_live_config() -> LiveConfig {
    let mut cfg = two_asset_live_config();
    cfg.strategy_id = "01TESTLIVERISKVETO0000002".into();
    cfg.assets.truncate(1); // BTC only
    cfg.display_name = "live daily-loss-kill test".into();
    cfg
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

/// With `max_concurrent_positions = 1`, the FIRST `long_open` (BTC) is
/// allowed and opens a position. The SECOND `long_open` (ETH) is vetoed by
/// the risk gate because BTC already holds an open position and the cap is 1.
///
/// Evidence:
///   - the broker receives exactly ONE order (BTC's open);
///   - a `supervisor_notes` row records reason `max_concurrent_positions`;
///   - the ETH decision row records the trader's action (`long_open`) but
///     has no fill price (the position was NOT opened).
#[tokio::test]
async fn live_max_concurrent_positions_veto_blocks_second_open() {
    let (store, dir) = fresh_store().await;
    let strategy = two_asset_strategy_max1();
    let scenario = live_scenario(100_000.0);
    let cfg = two_asset_live_config();

    let mut run = Run::new_queued(strategy.manifest.id.clone(), String::new(), RunMode::Live);
    run.live_config = Some(cfg.clone());
    store.create(&run).await.unwrap();
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();

    let broker = RecordingBroker::new(50_000.0);

    // BTC bar at t=60, ETH bar at t=120.  The live loop processes bars in
    // stream-arrival order; using different timestamps guarantees BTC is
    // processed first (BTC's stream closes at 60s, ETH's stream closes at
    // 120s) so BTC opens the position before ETH's veto check runs.
    let btc_stream = live_stream_for("BTC/USD", vec![market_bar_at(60, 50_000.0)]);
    let eth_stream = live_stream_for("ETH/USD", vec![market_bar_at(120, 3_000.0)]);
    let multi = MultiLiveStream::new(vec![
        (AssetSymbol::Btc, btc_stream),
        (AssetSymbol::Eth, eth_stream),
    ]);

    let executor = Executor::live(
        &cfg,
        broker.clone(),
        multi,
        WallClock::with_now_fn(|| ts(120)),
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
        .expect("live run completes on stream end");

    // 1. Exactly one broker order: BTC's opening Buy.  ETH's long_open was
    //    vetoed and rewritten to hold, so it never reached the broker.
    let submitted = broker.submitted();
    assert_eq!(
        submitted.len(),
        1,
        "max_concurrent_positions=1 must allow only one opening order; \
         ETH's long_open must be vetoed (got {submitted:?})"
    );
    assert_eq!(submitted[0].asset, "BTC/USD", "the allowed order is BTC's open");

    // 2. A supervisor note with reason "max_concurrent_positions" was
    //    recorded for the vetoed ETH open.
    let notes = store.read_supervisor_notes(&run.id).await.unwrap();
    let veto_note = notes.iter().find(|(role, severity, content)| {
        role == "risk" && severity == "warn" && content.contains("max_concurrent_positions")
    });
    assert!(
        veto_note.is_some(),
        "a supervisor note with role=risk, severity=warn, reason=max_concurrent_positions \
         must be recorded for the vetoed ETH open; got notes={notes:?}"
    );

    // 3. The ETH decision row was persisted but with no fill price (the
    //    position was never opened by the broker).
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let eth_decision = decisions
        .iter()
        .find(|d| d.asset == "ETH/USD")
        .expect("an ETH decision row must be persisted");
    assert_eq!(
        eth_decision.fill_price, None,
        "vetoed ETH open must not produce a fill (fill_price must be None)"
    );

    // Suppress unused-variable warning for the dir guard.
    drop(dir);
}

/// With `daily_loss_kill_pct = 0.05` on 100k starting capital (kill
/// threshold: 5_000 USD realized loss per UTC day), a scripted live run:
///
///   1. bar t=60  — `long_open`, fills at 50_000 (0.2 units, 10k notional);
///   2. bar t=120 — `flat`, fills at 20_000 → realized PnL = -6_000,
///      breaching the 5_000 daily kill threshold;
///   3. bar t=180 — `long_open` again (same UTC day) → VETOED by the
///      daily-loss kill, rewritten to `hold`.
///
/// Evidence:
///   - the broker receives exactly TWO orders (the open + the close);
///   - a `supervisor_notes` row records reason `daily_loss_kill`;
///   - the third decision row records the trader's action (`long_open`)
///     but has no fill price (no order was placed).
#[tokio::test]
async fn live_daily_loss_kill_veto_blocks_open_after_breach() {
    let (store, dir) = fresh_store().await;
    let strategy = one_asset_strategy_daily_kill();
    let scenario = live_scenario(100_000.0);
    let cfg = one_asset_live_config();

    let mut run = Run::new_queued(strategy.manifest.id.clone(), String::new(), RunMode::Live);
    run.live_config = Some(cfg.clone());
    store.create(&run).await.unwrap();
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();

    // First order (the open) fills at 50_000; second (the close) at
    // 20_000 → realized = 0.2 * (20_000 - 50_000) = -6_000.
    let broker = RecordingBroker::with_prices(vec![50_000.0, 20_000.0]);

    // All three bars fall on the same UTC day (1970-01-01), so the
    // daily-loss accumulator never resets between decisions.
    let btc_stream = live_stream_for(
        "BTC/USD",
        vec![
            market_bar_at(60, 50_000.0),
            market_bar_at(120, 20_000.0),
            market_bar_at(180, 20_000.0),
        ],
    );
    let multi = MultiLiveStream::new(vec![(AssetSymbol::Btc, btc_stream)]);

    let executor = Executor::live(
        &cfg,
        broker.clone(),
        multi,
        WallClock::with_now_fn(|| ts(180)),
        None,
    )
    .expect("live executor builds");

    let _metrics = executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            action_sequence_dispatch(&["long_open", "flat", "long_open"]),
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await
        .expect("live run completes on stream end");

    // 1. Exactly two broker orders: the open and the loss-realizing close.
    //    The third decision's long_open was vetoed and never reached the
    //    broker.
    let submitted = broker.submitted();
    assert_eq!(
        submitted.len(),
        2,
        "after the daily-loss kill breach, the third long_open must be vetoed \
         and never reach the broker (got {submitted:?})"
    );

    // 2. A supervisor note with reason "daily_loss_kill" was recorded for
    //    the vetoed open.
    let notes = store.read_supervisor_notes(&run.id).await.unwrap();
    let veto_note = notes.iter().find(|(role, severity, content)| {
        role == "risk" && severity == "warn" && content.contains("daily_loss_kill")
    });
    assert!(
        veto_note.is_some(),
        "a supervisor note with role=risk, severity=warn, reason=daily_loss_kill \
         must be recorded for the vetoed open; got notes={notes:?}"
    );

    // 3. The third decision row (index 2) was persisted with the trader's
    //    original action but no fill (the order was rewritten to hold).
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let vetoed = decisions
        .iter()
        .find(|d| d.decision_index == 2)
        .expect("the third decision row must be persisted");
    assert_eq!(
        vetoed.action, "long_open",
        "the decision row records the trader's ORIGINAL action"
    );
    assert_eq!(
        vetoed.fill_price, None,
        "vetoed open must not produce a fill (fill_price must be None)"
    );

    drop(dir);
}
