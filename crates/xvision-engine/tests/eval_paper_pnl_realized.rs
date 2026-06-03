//! Integration test for `DecisionRow.pnl_realized` in paper mode.
//!
//! Verifies that `paper-mode-executor-deleted` populates `pnl_realized` correctly using the
//! same formula as `backtest::simulate_fill`:
//!   realized = pre_fill_position × (fill_price − entry_price) − fee
//!
//! Scenario: long_open (bar 0) → short_open (bar 1) → hold × 2
//!   - Bar 0 close at 50_000: open long → pre_fill_pos = 0, pnl = Some(-fee)
//!     (pure open; fee = None from mock so realized = Some(0.0))
//!   - Bar 1 close at 50_100: short_open while long → close-long sell
//!     → pnl = Some(size × (50_100 − 50_000) − fee)
//!   - Bars 2-3: hold → no fill → pnl = None

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::{canonical_scenarios, Run, RunMode, RunStore, Scenario};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;
use xvision_execution::broker_surface::{BrokerSurface, MockBrokerSurface};

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
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/013_cli_jobs.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/018_agent_run_observability.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/037_review_annotations_and_autofire.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/038_eval_runs_live_config.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

fn minimal_strategy() -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01TESTSTRATEGY0000000000000B".into(),
            display_name: "PnL realized test strategy".into(),
            plain_summary: "for pnl_realized paper tests".into(),
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
        hypothesis: None,
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
    }
}

/// 4-bar scenario at 60-min cadence. Bars have rising closes so fills are
/// at known prices (MockBrokerSurface fills at `reference_price_usd` =
/// `bar.close`).
fn short_scenario() -> Scenario {
    let mut s = canonical_scenarios()[0].clone();
    s.id = "test-pnl-realized".into();
    s.time_window.start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    s.time_window.end = Utc.with_ymd_and_hms(2025, 1, 1, 4, 0, 0).unwrap();
    s
}

/// Returns bars matching the scenario window. Closes are 50_000, 50_100,
/// 50_200, 50_300 so the PnL arithmetic is deterministic.
fn pnl_bars(scenario: &Scenario) -> Vec<Ohlcv> {
    let mut bars = Vec::new();
    let mut ts = scenario.time_window.start;
    let closes = [50_000.0_f64, 50_100.0, 50_200.0, 50_300.0];
    for &close in &closes {
        if ts >= scenario.time_window.end {
            break;
        }
        bars.push(Ohlcv {
            timestamp: ts,
            open: close - 25.0,
            high: close + 50.0,
            low: close - 75.0,
            close,
            volume: 100.0,
        });
        ts += chrono::Duration::hours(1);
    }
    bars
}

/// long_open → short_open → hold → hold
///
/// Asserts:
/// - Decision 0 (open): `pnl_realized = Some(0.0)` — pure open, fee = None from mock
/// - Decision 1 (close via short_open): `pnl_realized = Some(3.0)` —
///   size=0.03 × (50_100 − 50_000) = 3.0, fee=0.0
/// - Decisions 2-3 (hold): `pnl_realized = None`
///
/// The expected close PnL mirrors the backtest formula exactly:
///   pre_fill_position × (fill_price − entry_price) − fee
///   = 0.03 × (50_100 − 50_000) − 0.0 = 3.0
#[tokio::test]
#[ignore = "executor-collapse-paper-mode (2026-05-22): PnL assertions were pinned to paper-mode broker close-price fills; migrate alongside the RealBrokerFills FillSink in live-bar-source-alpaca / live-eval-launch-and-freeze. SimulatedFills uses next-bar-open prices, not close prices, so the expected_pnl arithmetic differs. Keep the test body intact for the Live-track migration."]
async fn paper_pnl_realized_long_then_close() {
    let long_resp = || LlmResponse {
        content: vec![ContentBlock::Text {
            text: r#"{"action":"long_open","conviction":0.7,"justification":"go long"}"#.into(),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    };
    let short_resp = || LlmResponse {
        content: vec![ContentBlock::Text {
            text: r#"{"action":"short_open","conviction":0.6,"justification":"close long"}"#.into(),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    };
    let hold_resp = || LlmResponse {
        content: vec![ContentBlock::Text {
            text: r#"{"action":"hold","conviction":0.0,"justification":"wait"}"#.into(),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    };

    let responses = vec![long_resp(), short_resp(), hold_resp(), hold_resp()];

    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let initial_balance = 100_000.0_f64;
    let mock = Arc::new(MockBrokerSurface::new(initial_balance));
    let _broker: Arc<dyn BrokerSurface> = mock.clone();
    let strategy = minimal_strategy();
    let scenario = short_scenario();
    let bars = pnl_bars(&scenario);
    let executor = Executor::backtest(bars.clone());
    let mut run = Run::new_queued("pnl-test-hash".into(), scenario.id.clone(), RunMode::Backtest);
    store.create(&run).await.unwrap();
    store.ensure_agent_run_baseline(&run.id, "hash_only").await.unwrap();
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(MockDispatch::sequence(responses));
    let tools = Arc::new(ToolRegistry::empty());

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("long → close run must complete");

    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 4, "expected 4 decision rows");

    // Bar 0: long_open — pure open, pre_fill_position=0, fee=None → realized = Some(0.0)
    let d0 = &decisions[0];
    assert_eq!(d0.action, "long_open");
    assert!(
        d0.fill_price.is_some(),
        "decision 0 must have a fill_price (long was opened)"
    );
    assert_eq!(
        d0.pnl_realized,
        Some(0.0),
        "open row: pnl_realized must be Some(0.0) (pure open, no prior position, fee=0)"
    );

    // Bar 1: short_open while long → close-long sell
    // Expected PnL: size × (fill_price_1 − fill_price_0) − fee
    // size = buying_power × risk_pct / reference_price_0
    //      = 100_000 × 0.015 / 50_000 = 0.03
    // pnl  = 0.03 × (50_100 − 50_000) − 0.0 = 3.0
    let d1 = &decisions[1];
    assert_eq!(d1.action, "short_open");
    assert!(
        d1.fill_price.is_some(),
        "decision 1 must have a fill_price (close-long sell was submitted)"
    );
    let expected_size = (initial_balance * 0.015) / 50_000.0; // 0.03
    let expected_pnl = expected_size * (50_100.0 - 50_000.0); // 3.0
    let actual_pnl = d1.pnl_realized.expect("closing row must have Some(pnl_realized)");
    assert!(
        (actual_pnl - expected_pnl).abs() < 1e-9,
        "close row pnl_realized mismatch: expected {expected_pnl}, got {actual_pnl}"
    );

    // Bars 2-3: hold — no fill → pnl_realized must be None
    for (i, d) in decisions[2..].iter().enumerate() {
        assert_eq!(d.action, "hold");
        assert_eq!(
            d.pnl_realized,
            None,
            "hold row {} must have pnl_realized=None (no fill)",
            i + 2
        );
    }
}
