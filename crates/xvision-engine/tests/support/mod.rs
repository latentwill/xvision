#![allow(dead_code, deprecated)]

pub mod eval_harness;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use futures::stream;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

use xvision_core::trading::AssetSymbol;
use xvision_core::Capital;
use xvision_data::alpaca::{BarGranularity, MarketBar};
use xvision_data::alpaca_live::{AlpacaLiveClient, AlpacaLiveCredentials, LiveBarItem};
use xvision_data::alpaca_live_poll::{AlpacaLivePoll, AlpacaPollError, LivePollFetcher};
use xvision_execution::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest};

use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::executor::{Executor, LiveStream, MultiLiveStream, RunExecutor, WallClock};
use xvision_engine::eval::live_config::{LiveConfig, StopPolicy};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::{AssetClass, AssetRef, Scenario};
use xvision_engine::eval::store::RunStore;
use xvision_engine::eval::{canonical_scenarios, scenario_store};
use xvision_engine::safety::VenueLabel;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

// ---------------------------------------------------------------------------
// Original helpers (preserved)
// ---------------------------------------------------------------------------

pub async fn api_eval_run_context() -> (ApiContext, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("strategies")).unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "operator".into(),
        },
    )
    .await
    .unwrap();
    seed_flash_scenario(&ctx).await;
    (ctx, dir)
}

pub async fn api_eval_run_context_with_agents() -> (ApiContext, tempfile::TempDir) {
    api_eval_run_context().await
}

async fn seed_flash_scenario(ctx: &ApiContext) {
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash canonical scenario must exist");
    scenario_store::insert_scenario(ctx, &scenario).await.unwrap();
}

/// Apply every migration that touches eval-review state. Mirrors the
/// prefix `ApiContext::open` walks at startup so eval-review integration
/// tests do not each curate their own schema list.
#[allow(dead_code)]
pub async fn eval_review_pool_with_migrations() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    for sql in [
        include_str!("../../migrations/002_eval.sql"),
        include_str!("../../migrations/014_eval_agent_id.sql"),
        include_str!("../../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../../migrations/016_eval_reviews.sql"),
        include_str!("../../migrations/017_eval_findings_review_columns.sql"),
        include_str!("../../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../../migrations/026_trace_surface_foundation.sql"),
        include_str!("../../migrations/027_run_bars_manifest.sql"),
        include_str!("../../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../../migrations/038_eval_runs_live_config.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}

/// Apply the safety-state schema for safety integration tests.
#[allow(dead_code)]
pub async fn safety_pool_with_migrations() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../../migrations/030_safety_state_and_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

// ---------------------------------------------------------------------------
// CT5 live-deployments helpers (Task 0)
// ---------------------------------------------------------------------------

/// A fully-migrated `ApiContext` (includes all registered migrations, e.g.
/// migration 065 once Task 1 registers it in `api/mod.rs`).
///
/// Mirrors `tests/common/mod.rs::open_api_context()` which calls
/// `ApiContext::open(dir.path(), actor)`. CRITICAL: the file-backed SQLite DB
/// lives in a `TempDir` — if the `TempDir` drops the DB file is deleted and
/// every query fails with "unable to open database file". We `Box::leak` it so
/// the signature can stay `-> ApiContext` without burdening callers with a
/// (Context, TempDir) tuple. Test processes are short-lived; the leak is
/// bounded.
pub async fn api_context_fresh() -> ApiContext {
    let dir: &'static tempfile::TempDir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    std::fs::create_dir_all(dir.path().join("strategies")).unwrap();
    ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "operator".into(),
        },
    )
    .await
    .unwrap()
}

/// Build a `Queued` live `Run` with the given `VenueLabel` in its `LiveConfig`.
///
/// The live config uses BTC/USD (Alpaca crypto whitelist), 10_000 USD initial
/// capital, `broker_creds_ref = "alpaca"`, and a `bar_limit = 10` stop policy.
/// Useful for testing `store.create()`, venue-label persistence, and the live
/// deployment list/filter path.
pub fn live_run_with_venue(label: VenueLabel) -> Run {
    let cfg = LiveConfig {
        strategy_id: "01TESTSUPPORT_LIVE".into(),
        assets: vec![AssetRef {
            class: AssetClass::Crypto,
            symbol: "BTC/USD".into(),
            venue_symbol: "BTC/USD".into(),
        }],
        capital: Capital {
            initial: 10_000.0,
            currency: "USD".into(),
        },
        broker_creds_ref: "alpaca".into(),
        stop_policy: StopPolicy {
            bar_limit: Some(10),
            ..Default::default()
        },
        venue_label: label,
        warmup_bars: None,
        safety_limits: None,
        display_name: "support live run".into(),
        description: None,
        tags: vec![],
        notes: None,
    };
    Run::new_queued("01TESTSUPPORT_LIVE".into(), String::new(), RunMode::Live).with_live_config(cfg)
}

/// Build a `Queued` backtest `Run` (mode=Backtest, a scenario_id, no
/// `live_config`). Useful for list-filter tests that need a non-live run
/// alongside a live run.
///
/// Uses `"flash-crash-aug-2024"` — the ID seeded by `ApiContext::open` via
/// `scenario_seed::run_seed_if_needed`. This differs from the legacy
/// `canonical_scenarios()` ID (`"flash-crash-2024-08"`) which is NOT in the
/// canonical seed rows and would fail the trigger FK check.
pub fn backtest_run() -> Run {
    Run::new_queued(
        "01TESTSUPPORT_BT".into(),
        "flash-crash-aug-2024".into(),
        RunMode::Backtest,
    )
}

/// Handle returned by [`run_short_live`]. Gives the caller the pool and
/// run_id so they can query the state tables the executor wrote.
pub struct LiveTestHandle {
    pub pool: sqlx::SqlitePool,
    pub run_id: String,
}

// ---------------------------------------------------------------------------
// Internal helpers for run_short_live — replicating live_fixtures() from
// tests/eval_executor_live_loop.rs. We can't call across test binaries so
// we inline the fixture construction here.
// ---------------------------------------------------------------------------

fn _support_live_client() -> AlpacaLiveClient {
    AlpacaLiveClient::new(AlpacaLiveCredentials {
        key_id: "test".into(),
        secret_key: "test".into(),
    })
}

struct _SupportEmptyFetcher;

#[async_trait]
impl LivePollFetcher for _SupportEmptyFetcher {
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

/// Build a single-asset BTC/USD `MultiLiveStream` from `n` scripted bars.
fn _support_stream(n: usize, initial: f64) -> MultiLiveStream {
    let bars: Vec<MarketBar> = (0..n)
        .map(|i| {
            let ts = Utc.timestamp_opt((i as i64 + 1) * 60, 0).single().unwrap();
            MarketBar {
                timestamp: ts,
                open: initial - 1.0,
                high: initial + 1.0,
                low: initial - 2.0,
                close: initial,
                volume: 1_000.0,
            }
        })
        .collect();
    let ws_items: Vec<LiveBarItem> = bars.iter().cloned().map(LiveBarItem::Bar).collect();
    let ws = _support_live_client().subscription_from_stream(BarGranularity::Minute1, stream::iter(ws_items));
    let poll = AlpacaLivePoll::new(
        Arc::new(_SupportEmptyFetcher),
        "BTC/USD".into(),
        BarGranularity::Minute1,
    )
    .with_poll_interval(Duration::ZERO);
    let live_stream = LiveStream::new_for_test(Vec::new(), ws, poll);
    MultiLiveStream::new(vec![(AssetSymbol::Btc, live_stream)])
}

/// Minimal no-op broker mock: every order is filled at `initial` price.
struct _SupportBroker {
    price: f64,
}

#[async_trait]
impl BrokerSurface for _SupportBroker {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        Ok(OrderConfirmation {
            broker_order_id: format!("support-{}", req.idempotency_key),
            fill_price: Some(self.price),
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

fn _support_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01TESTSUPPORT_LIVE".into(),
            display_name: "support live strategy".into(),
            plain_summary: "CT5 support helper".into(),
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

fn _support_live_scenario(initial: f64) -> Scenario {
    #[allow(deprecated)]
    let mut scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("canonical flash-crash scenario must exist");
    scenario.capital = Capital {
        initial,
        currency: "USD".into(),
    };
    scenario.warmup_bars = 0;
    scenario
}

fn _support_live_config(initial: f64, bars: usize) -> LiveConfig {
    LiveConfig {
        strategy_id: "01TESTSUPPORT_LIVE".into(),
        assets: vec![AssetRef {
            class: AssetClass::Crypto,
            symbol: "BTC/USD".into(),
            venue_symbol: "BTC/USD".into(),
        }],
        capital: Capital {
            initial,
            currency: "USD".into(),
        },
        broker_creds_ref: "alpaca".into(),
        stop_policy: StopPolicy {
            bar_limit: Some(bars as u32),
            ..Default::default()
        },
        venue_label: VenueLabel::Paper,
        warmup_bars: Some(0),
        safety_limits: None,
        display_name: "support run_short_live".into(),
        description: None,
        tags: vec![],
        notes: None,
    }
}

/// Drive the live executor for `bars` synthetic BTC/USD bars at `initial`
/// capital. Opens a fully-migrated ApiContext, inserts a Queued live run,
/// drives the executor to completion, then returns the pool and run_id so
/// the caller can query `live_run_state` and related tables.
///
/// This replicates the `live_fixtures()` + executor pattern from
/// `tests/eval_executor_live_loop.rs` in a reusable form (cross-binary
/// reuse is not possible; the fixture construction is inlined here).
pub async fn run_short_live(bars: usize, initial: f64) -> LiveTestHandle {
    let ctx = api_context_fresh().await;
    let pool = ctx.db.clone();
    let store = RunStore::new(pool.clone());

    let live_cfg = _support_live_config(initial, bars);
    let mut run = Run::new_queued("01TESTSUPPORT_LIVE".into(), String::new(), RunMode::Live)
        .with_live_config(live_cfg.clone());
    store.create(&run).await.unwrap();
    store
        .ensure_agent_run_baseline(&run.id, "hash_only")
        .await
        .unwrap();

    let strategy = _support_strategy();
    let scenario = _support_live_scenario(initial);
    let broker: Arc<dyn BrokerSurface> = Arc::new(_SupportBroker { price: initial });
    let stream = _support_stream(bars, initial);
    let now_ts = Utc.timestamp_opt(60, 0).single().unwrap();

    let executor = Executor::live(
        &live_cfg,
        broker,
        stream,
        WallClock::with_now_fn(move || now_ts),
        None,
    )
    .expect("live executor builds in support helper");

    let dispatch = Arc::new(MockDispatch::echo(
        r#"{"action":"hold","conviction":0.0,"justification":"support"}"#,
    ));

    executor
        .run(
            &mut run,
            &strategy,
            &scenario,
            &[],
            dispatch,
            Arc::new(ToolRegistry::empty()),
            &store,
        )
        .await
        .expect("run_short_live executor completes");

    let run_id = run.id.clone();
    LiveTestHandle { pool, run_id }
}
