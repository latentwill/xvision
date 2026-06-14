//! Regression test for `risk-gate-min-notional`.
//!
//! Operator-reported failure 2026-05-19 (round-4 finding B):
//! paper-venue ETH/USD orders sized ~$6 (0.00274 ETH × ~$2,200) were
//! submitted every decision cycle and rejected by Alpaca with
//! `cost basis must be >= minimal amount of order 10`. PR #314 + #286
//! make the run survive (recoverable broker error + agent feedback);
//! this contract is the proactive layer that keeps the broker call
//! from firing in the first place.
//!
//! Acceptance: with `min_notional_usd = 10.0` set on the `paper-mode-executor-deleted`,
//! a `long_open` decision that sizes below $10 produces:
//!   - zero broker submissions (`MockBrokerSurface.submitted().len() == 0`)
//!   - one persisted decision row per tick, tagged
//!     `[below_venue_min_notional]` in the justification
//!   - run completes successfully (status = Completed)
//!
//! Without the gate (`min_notional_usd = None`) the same configuration
//! submits the orders — confirms the gate is doing the work, not some
//! other guard.

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use uuid::Uuid;
use xvision_core::market::Ohlcv;
use xvision_core::{Action, AssetSymbol, Direction, PortfolioState, TraderDecision};
use xvision_engine::agent::llm::{LlmDispatch, MockDispatch};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::{canonical_scenarios, Run, RunMode, RunStatus, RunStore, Scenario};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};
use xvision_risk::rules::MinNotional;
use xvision_risk::{RiskEvalContext, RiskRule, RuleVerdict};

async fn pool_with_migration() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
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
    pool
}

/// Strategy with a tiny `risk_pct_per_trade` so the executor sizes a
/// long_open at the operator's reported notional (~$6 on $60 buying
/// power × 0.1 risk = $6). The `Balanced` preset's defaults are too
/// generous to reproduce the failure on the ETH regression fixture, so
/// we tune the risk preset directly.
fn tiny_risk_strategy() -> Strategy {
    let mut risk = RiskPreset::Balanced.expand();
    risk.risk_pct_per_trade = 0.1;
    Strategy {
        manifest: PublicManifest {
            id: "01TESTSTRATEGYMINNOTIONAL01".into(),
            display_name: "Tiny-notional regression".into(),
            plain_summary: "Repros operator's ETH ~$6 order".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: vec!["ETH/USD".into()],
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

/// 4-hour scenario at 60-min cadence → 4 ticks; ETH-like ~$2,200 close.
fn short_scenario() -> Scenario {
    let mut s = canonical_scenarios()[0].clone();
    s.id = "test-min-notional".into();
    s.time_window.start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    s.time_window.end = Utc.with_ymd_and_hms(2025, 1, 1, 4, 0, 0).unwrap();
    s
}

fn eth_like_bars(scenario: &Scenario) -> Vec<Ohlcv> {
    // Use an ETH-like reference price so notional math mirrors the
    // operator's failure. ~$2,200 × 0.00274 ETH ≈ $6.03.
    let mut bars = Vec::new();
    let mut ts = scenario.time_window.start;
    let mut i = 0.0;
    while ts < scenario.time_window.end {
        let close = 2_200.0 + i;
        bars.push(Ohlcv {
            timestamp: ts,
            open: close - 0.25,
            high: close + 0.5,
            low: close - 0.75,
            close,
            volume: 100.0 + i,
        });
        ts += chrono::Duration::hours(1);
        i += 1.0;
    }
    bars
}

#[test]
fn exact_min_notional_boundary_passes_risk_rule() {
    let rule = MinNotional {
        min_notional_usd: 10.0,
        venue_id: "paper".into(),
    };
    let decision = TraderDecision {
        cycle_id: Uuid::new_v4(),
        action: Action::Buy,
        size_bps: 100,
        direction: Direction::Long,
        stop_loss_pct: 2.0,
        take_profit_pct: 5.0,
        trader_summary: "exact boundary order".into(),
        asset: AssetSymbol::Eth,
        trailing_stop_pct: None,
        breakeven_trigger_pct: None,
        breakeven_offset_pct: None,
        fade_sl_bars: None,
        fade_sl_start_pct: None,
        fade_sl_end_pct: None,
        max_bars_held: None,
        sl_atr_mult: None,
        tp_atr_mult: None,
        tp1_pct: None,
        tp1_close_fraction: None,
        tp2_pct: None,
    };
    let portfolio = PortfolioState {
        equity_usd: 1000.0,
        realized_pnl_today_usd: 0.0,
        day_index: 0,
        open_positions: BTreeMap::new(),
        as_of: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
    };

    let verdict = rule.evaluate(&RiskEvalContext {
        decision: &decision,
        portfolio: &portfolio,
        asset: AssetSymbol::Eth,
        conviction: 1.0,
        funding_rate_8h: None,
    });
    assert!(
        matches!(verdict, RuleVerdict::Pass),
        "$1000 equity x 100 bps equals the $10 venue minimum and must pass; got {verdict:?}",
    );
}

/// Confirms the gate works end-to-end on the operator's exact failure
/// shape: tiny buying power × small risk_pct → ~$6 notional → veto.
/// The broker never sees a submit.
#[tokio::test]
#[ignore = "executor-collapse-paper-mode (2026-05-22): asserts pre-submit min-notional gate that lived in the paper executor. The gate logic belongs in the Live wiring track once RealBrokerFills + LiveConfig.safety_limits land. Re-enable then."]
async fn min_notional_gate_skips_broker_for_below_min_orders() {
    // Buying power 60 USD × risk_pct 0.1 = $6 USD notional, below the
    // paper venue's $10 minimum.
    let initial_balance = 60.0;
    let canned = r#"{"action":"long_open","conviction":0.7,"justification":"go long ETH"}"#;

    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(initial_balance));
    let _broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = tiny_risk_strategy();
    let scenario = short_scenario();
    let executor = Executor::with_bars(eth_like_bars(&scenario)); // paper venue minimum
    let mut run = Run::new_queued("test-min-notional".into(), scenario.id.clone(), RunMode::Backtest);
    store.create(&run).await.unwrap();
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned));
    let tools = Arc::new(ToolRegistry::empty());

    let metrics = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("run must complete cleanly when MinNotional gate fires");

    // Acceptance #1: broker NEVER called.
    let submitted = mock.submitted();
    assert_eq!(
        submitted.len(),
        0,
        "broker must not be called for below-min-notional orders; submitted={submitted:?}"
    );

    // Acceptance #2: run completed (not errored).
    let after = store.get(&run.id).await.unwrap();
    assert_eq!(after.status, RunStatus::Completed);

    // Acceptance #3: every tick recorded a decision row tagged with
    // the veto reason so the operator sees what happened.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(
        decisions.len(),
        4,
        "expected one decision row per tick (4); got {}",
        decisions.len()
    );
    for d in &decisions {
        assert_eq!(d.asset, "ETH/USD");
        assert_eq!(d.action, "long_open");
        assert!(
            d.order_size.is_none(),
            "order_size must be None for vetoed orders"
        );
        assert!(d.fill_price.is_none());
        assert!(
            d.justification
                .as_deref()
                .unwrap_or("")
                .contains("[below_venue_min_notional]"),
            "decision row must carry the [below_venue_min_notional] tag; got {:?}",
            d.justification
        );
    }

    // Acceptance #4: zero trades recorded.
    assert_eq!(metrics.n_trades, 0);
    assert_eq!(metrics.n_decisions, 4);
}

/// Control test: the same configuration WITHOUT the gate submits the
/// orders. Demonstrates the gate is the thing doing the work, not some
/// unrelated guard.
#[tokio::test]
#[ignore = "executor-collapse-paper-mode (2026-05-22): asserts pre-submit min-notional gate that lived in the paper executor. The gate logic belongs in the Live wiring track once RealBrokerFills + LiveConfig.safety_limits land. Re-enable then."]
async fn without_gate_below_min_orders_reach_the_broker() {
    let initial_balance = 60.0;
    let canned = r#"{"action":"long_open","conviction":0.7,"justification":"go long ETH"}"#;

    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(initial_balance));
    let _broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = tiny_risk_strategy();
    let scenario = short_scenario();
    // No `with_min_notional_usd` call — gate is disabled.
    let executor = Executor::with_bars(eth_like_bars(&scenario));
    let mut run = Run::new_queued(
        "test-min-notional-control".into(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned));
    let tools = Arc::new(ToolRegistry::empty());

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("run must complete");

    let submitted = mock.submitted();
    assert!(
        !submitted.is_empty(),
        "WITHOUT gate, the tiny-notional order should reach the broker; that's the failure mode we're fixing"
    );
}

/// A `min_notional_usd = 0.0` explicit disable behaves the same as
/// no gate at all — required by the contract's "0.0 = no-op" semantics.
#[tokio::test]
#[ignore = "executor-collapse-paper-mode (2026-05-22): asserts pre-submit min-notional gate that lived in the paper executor. The gate logic belongs in the Live wiring track once RealBrokerFills + LiveConfig.safety_limits land. Re-enable then."]
async fn zero_min_notional_is_noop() {
    let initial_balance = 60.0;
    let canned = r#"{"action":"long_open","conviction":0.7,"justification":"go long ETH"}"#;

    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(initial_balance));
    let _broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = tiny_risk_strategy();
    let scenario = short_scenario();
    let executor = Executor::with_bars(eth_like_bars(&scenario));
    let mut run = Run::new_queued(
        "test-min-notional-zero".into(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned));
    let tools = Arc::new(ToolRegistry::empty());

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("run must complete");

    let submitted = mock.submitted();
    assert!(
        !submitted.is_empty(),
        "min_notional_usd = 0.0 must be a no-op; orders should reach the broker"
    );
}

/// Normally-sized orders (well above the gate) flow through to the
/// broker exactly as before — the gate is precise.
#[tokio::test]
#[ignore = "executor-collapse-paper-mode (2026-05-22): asserts pre-submit min-notional gate that lived in the paper executor. The gate logic belongs in the Live wiring track once RealBrokerFills + LiveConfig.safety_limits land. Re-enable then."]
async fn above_min_notional_orders_pass_through() {
    // Large buying power × default-ish risk_pct → notional well above $10.
    let initial_balance = 100_000.0;
    let canned = r#"{"action":"long_open","conviction":0.7,"justification":"go long ETH"}"#;

    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let mock = Arc::new(MockBrokerSurface::new(initial_balance));
    let _broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = {
        let mut s = tiny_risk_strategy();
        // 1% risk × $100k buying power = $1000 notional — well over $10.
        s.risk.risk_pct_per_trade = 0.01;
        s
    };
    let scenario = short_scenario();
    let executor = Executor::with_bars(eth_like_bars(&scenario));
    let mut run = Run::new_queued("test-above-min".into(), scenario.id.clone(), RunMode::Backtest);
    store.create(&run).await.unwrap();
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::echo(canned));
    let tools = Arc::new(ToolRegistry::empty());

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("run must complete");

    let submitted = mock.submitted();
    assert_eq!(
        submitted.len(),
        1,
        "above-min orders must pass through; expected 1 submit (subsequent ticks are duplicate long_opens), got {}",
        submitted.len()
    );
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert!(
        decisions.iter().all(|d| d.asset == "ETH/USD"),
        "ETH regression fixture must persist ETH decisions; got {:?}",
        decisions.iter().map(|d| &d.asset).collect::<Vec<_>>()
    );
}
