//! I3: mark-to-market at scenario end (run_inner-level integration).
//!
//! The unit tests in `backtest::mark_to_market_tests` and
//! `book::tests::close_all_at_mark_*` exercise `PortfolioBook::close_all_at_mark`
//! in isolation. They do NOT prove the *wiring*: that `run_inner` actually
//! invokes the mark-to-market close at NORMAL COMPLETION (all bars consumed)
//! and folds the resulting realized PnL into the run's final metrics.
//!
//! This test drives a full backtest through `Executor::run` (→ `run_inner`):
//!   - bar 0: trader opens a LONG (`long_open`)
//!   - bars 1..N: trader HOLDs — the position is NEVER closed by the strategy
//!
//! Prices rise monotonically, so the still-open long is a clear winner at the
//! final bar. Because the strategy never closes it, the ONLY way the run can
//! book a closed, winning round-trip is the mark-to-market block at scenario
//! end. We therefore assert on the *realized* round-trip view:
//!   - `win_rate == 1.0`  (the held long closed at mark as a winner)
//!   - `n_trades >= 2`    (open leg + the MTM close leg)
//!
//! Without the MTM block, the open-and-held position never closes:
//! `realized_count == 0` → `win_rate == 0.0`. With it, the position is closed
//! at its last mark and counts as a winning round-trip. The mirror control
//! (`held_long_loser_*`) drives the same setup on a FALLING price series and
//! asserts `win_rate == 0.0`, proving the win is the MTM mark and not a
//! harness artifact.

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_core::{market::Ohlcv, Capital};
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, CalendarRef, DataSource, Fees, FillModel, LatencyModel,
    LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario, ScenarioSource,
    SlippageModel, TimeWindow, Venue, VenueSettings,
};
use xvision_engine::eval::store::RunStore;
use xvision_engine::safety::VenueLabel;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

async fn fresh_store() -> RunStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/037_review_annotations_and_autofire.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(include_str!("../migrations/038_eval_runs_live_config.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/065_eval_run_source_and_unrealized_pnl.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    RunStore::new(pool)
}

fn build_strategy(agent_id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "mtm-at-scenario-end strategy".into(),
            plain_summary: "I3 mark-to-market wiring coverage".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
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

/// Zero-fee, zero-slippage, full-at-close hourly BTC/USD scenario so the
/// mark-to-market arithmetic is exact and a rising/falling series is an
/// unambiguous winner/loser with no fee drag flipping the sign.
fn mtm_scenario(bars: usize) -> Scenario {
    let start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    Scenario {
        id: "test-mtm-scenario-end".into(),
        parent_scenario_id: None,
        source: ScenarioSource::Generated,
        display_name: "MTM at scenario end".into(),
        description: "Deterministic hourly BTC/USD bars for I3 mark-to-market".into(),
        tags: vec!["test".into()],
        notes: None,
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start,
            end: start + Duration::hours(bars as i64),
        },
        granularity: xvision_engine::eval::BarGranularity::Hour1,
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        data_source: DataSource::AlpacaHistorical {
            feed: Some("crypto".into()),
            adjustment: AdjustmentMode::Raw,
        },
        venue: VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees {
                maker_bps: 0,
                taker_bps: 0,
            },
            slippage: SlippageModel::None,
            latency: LatencyModel {
                decision_to_fill_ms: 0,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::FullAtClose,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
            overrides: Vec::new(),
            borrow_bps_per_day: 0.0,
        },
        replay_mode: ReplayMode::Continuous,
        capital: Capital {
            initial: 100_000.0,
            currency: "USD".into(),
        },
        bar_cache_policy: BarCachePolicy {
            cache_key: "test-mtm-scenario-end".into(),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        warmup_bars: 0,
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: start,
        created_by: "test".into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    }
}

/// `count` hourly bars whose close moves by `step` per bar from `base`.
/// `step > 0` → rising (a held long is a winner); `step < 0` → falling.
fn ramp_bars(scenario: &Scenario, count: usize, base: f64, step: f64) -> Vec<Ohlcv> {
    let mut bars = Vec::with_capacity(count);
    let mut ts = scenario.time_window.start;
    for i in 0..count {
        let close = base + i as f64 * step;
        bars.push(Ohlcv {
            timestamp: ts,
            open: close,
            high: close + step.abs() + 10.0,
            low: close - step.abs() - 10.0,
            close,
            volume: 1_000.0,
        });
        ts += Duration::hours(1);
    }
    bars
}

/// long_open on bar 0, then hold forever (MockDispatch::sequence holds the
/// last response steady-state).
fn long_then_hold_dispatch() -> Arc<dyn LlmDispatch> {
    let mk = |text: &str| LlmResponse {
        content: vec![ContentBlock::Text { text: text.into() }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    };
    Arc::new(MockDispatch::sequence(vec![
        mk(r#"{"action":"long_open","conviction":0.8,"justification":"i3-open"}"#),
        mk(r#"{"action":"hold","conviction":0.0,"justification":"i3-hold"}"#),
    ]))
}

/// I3: a long opened on bar 0 and HELD through a rising series to the final
/// bar must be closed at mark at scenario end. The held winner shows up as a
/// closed, winning round-trip in the final metrics — `win_rate == 1.0` — which
/// is only possible if `run_inner` runs the mark-to-market block. Without it
/// the position never closes and `win_rate` would be `0.0`.
#[tokio::test]
async fn held_long_winner_is_marked_to_market_at_scenario_end() {
    let store = fresh_store().await;
    let bars_n = 8usize;
    let scenario = mtm_scenario(bars_n);
    let strategy = build_strategy("01TESTI3MTMWINNER0000000000A");
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // Rising prices: 50_000, 50_200, ... → the long opened on bar 0 is well in
    // profit at the final bar.
    let bars = ramp_bars(&scenario, bars_n, 50_000.0, 200.0);
    let dispatch = long_then_hold_dispatch();
    let executor = Executor::with_bars(bars);

    let metrics = executor
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
        .expect("backtest run must complete");

    // Sanity: the run actually iterated every bar and opened the long.
    assert_eq!(
        metrics.n_decisions, bars_n as u32,
        "one decision per bar expected"
    );
    assert!(
        metrics.n_trades >= 2,
        "expected the open leg plus the mark-to-market close leg (got n_trades={})",
        metrics.n_trades
    );

    // The discriminating assertion: the held long was closed at mark as a
    // WINNING round-trip. Without the scenario-end MTM block this is 0.0
    // because the strategy never closed the position.
    assert_eq!(
        metrics.win_rate, 1.0,
        "held long winner must close at mark as a winning round-trip (win_rate={})",
        metrics.win_rate
    );

    // Final equity must reflect the unrealized gain folded in as realized PnL.
    assert!(
        metrics.total_return_pct > 0.0,
        "rising-series held long must end with positive total return (got {}%)",
        metrics.total_return_pct
    );
}

/// Mirror control: the IDENTICAL open-and-hold setup on a FALLING series must
/// mark to market as a LOSING round-trip — `win_rate == 0.0`. This proves the
/// `win_rate == 1.0` above is the MTM mark of a real winner, not a harness
/// artifact that always reports a win.
#[tokio::test]
async fn held_long_loser_marks_to_market_as_loss_at_scenario_end() {
    let store = fresh_store().await;
    let bars_n = 8usize;
    let scenario = mtm_scenario(bars_n);
    let strategy = build_strategy("01TESTI3MTMLOSER00000000000A");
    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // Falling prices: 50_000, 49_800, ... → the held long is underwater at the
    // final bar.
    let bars = ramp_bars(&scenario, bars_n, 50_000.0, -200.0);
    let dispatch = long_then_hold_dispatch();
    let executor = Executor::with_bars(bars);

    let metrics = executor
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
        .expect("backtest run must complete");

    assert!(
        metrics.n_trades >= 2,
        "expected the open leg plus the mark-to-market close leg (got n_trades={})",
        metrics.n_trades
    );
    assert_eq!(
        metrics.win_rate, 0.0,
        "held long loser must close at mark as a losing round-trip (win_rate={})",
        metrics.win_rate
    );
    assert!(
        metrics.total_return_pct < 0.0,
        "falling-series held long must end with negative total return (got {}%)",
        metrics.total_return_pct
    );
}
