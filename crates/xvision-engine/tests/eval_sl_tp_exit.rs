//! DoD coverage for the eval-trader risk-parity exit engine
//! (`docs/superpowers/specs/2026-06-03-eval-trader-risk-parity-sl-tp-sizing.md`).
//!
//! The backtest gained an intrabar stop/target exit engine plus model-emitted
//! sizing. These tests open a long position and drive the price path so a
//! position exits at the model stop, the model target, or the config ATR stop,
//! and check that `size_bps` overrides the mechanical sizing.

#![allow(deprecated)] // canonical_scenarios()

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::store::DecisionRow;
use xvision_engine::eval::{canonical_scenarios, Run, RunMode, RunStore, Scenario};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

async fn pool_with_migration() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    for m in [
        include_str!("../migrations/001_api_audit.sql"),
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/013_cli_jobs.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/018_agent_run_observability.sql"),
        include_str!("../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../migrations/027_run_bars_manifest.sql"),
        include_str!("../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../migrations/038_eval_runs_live_config.sql"),
    ] {
        sqlx::query(m).execute(&pool).await.unwrap();
    }
    pool
}

/// Single-asset strategy with explicit config ATR multiples so each test can
/// isolate model-emitted vs config-driven levels.
fn strategy(stop_atr: f64, take_atr: f64) -> Strategy {
    let mut risk = RiskPreset::Balanced.expand();
    risk.stop_loss_atr_multiple = stop_atr;
    risk.take_profit_atr_multiple = take_atr;
    // period=1 keeps the ATR warm from the first bar so config-ATR levels are
    // available on these short (4-bar) fixtures.
    risk.atr_period = 1;
    Strategy {
        manifest: PublicManifest {
            id: "01TESTSLTPEXIT0000000000001".into(),
            display_name: "SL/TP exit regression".into(),
            plain_summary: "open long, exit at stop/target".into(),
            creator: "@tester".into(),
            template: "custom".into(),
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
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
        risk,
        mechanical_params: serde_json::json!({}),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
    }
}

fn scenario(n_bars: usize) -> Scenario {
    let mut s = canonical_scenarios()[0].clone();
    s.id = "test-sl-tp".into();
    s.time_window.start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    s.time_window.end = Utc
        .with_ymd_and_hms(2025, 1, 1, n_bars as u32, 0, 0)
        .unwrap();
    s
}

/// Bars from explicit (open, high, low, close) tuples on an hourly grid.
fn bars(rows: &[(f64, f64, f64, f64)], s: &Scenario) -> Vec<Ohlcv> {
    let mut ts = s.time_window.start;
    let mut out = Vec::new();
    for &(open, high, low, close) in rows {
        out.push(Ohlcv {
            timestamp: ts,
            open,
            high,
            low,
            close,
            volume: 1_000.0,
        });
        ts += chrono::Duration::hours(1);
    }
    out
}

struct RunOut {
    win_rate: f64,
    n_trades: u32,
    decisions: Vec<DecisionRow>,
}

fn resp(text: &str) -> LlmResponse {
    LlmResponse {
        content: vec![ContentBlock::Text { text: text.into() }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    }
}

async fn run(strategy: Strategy, bars: Vec<Ohlcv>, canned: &str) -> RunOut {
    let n = bars.len();
    let scen = scenario(n);
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let executor = Executor::with_bars(bars);
    let mut r = Run::new_queued("test-sl-tp".into(), scen.id.clone(), RunMode::Backtest);
    store.create(&r).await.unwrap();
    store
        .ensure_agent_run_baseline(&r.id, "hash_only")
        .await
        .unwrap();
    // Open on the first bar, then hold — so the engine's exit (not a re-open
    // from the every-bar echo) is what closes the position. `sequence` repeats
    // the last entry, so this is long_open once then hold forever.
    let hold = r#"{"action":"hold","conviction":0.5,"justification":"hold"}"#;
    let dispatch: Arc<dyn LlmDispatch> =
        Arc::new(MockDispatch::sequence(vec![resp(canned), resp(hold)]));
    let tools = Arc::new(ToolRegistry::empty());
    let metrics = executor
        .run(&mut r, &strategy, &scen, &[], dispatch, tools, &store)
        .await
        .expect("run completes");
    let decisions = store.read_decisions(&r.id).await.unwrap();
    RunOut {
        win_rate: metrics.win_rate,
        n_trades: metrics.n_trades,
        decisions,
    }
}

const LONG_SL2_TP10: &str =
    r#"{"action":"long_open","conviction":0.8,"justification":"enter long","stop_loss_pct":2.0,"take_profit_pct":10.0}"#;

#[tokio::test]
async fn model_stop_loss_exit_closes_at_stop() {
    // Entry fills at bar[1].open = 100 → model stop = 98 (2%). bar[2] dips to
    // low 97 (open 99.5, no gap) so the exit fills at the stop, 98 — a loss.
    let b = bars(
        &[
            (100.0, 101.0, 99.0, 100.0),
            (100.0, 101.0, 99.5, 100.0),
            (99.5, 100.0, 97.0, 98.0),
            (98.0, 98.5, 97.5, 98.0),
        ],
        &scenario(4),
    );
    let out = run(strategy(0.0, 0.0), b, LONG_SL2_TP10).await;
    assert_eq!(out.n_trades, 2, "one open fill + one stop exit");
    assert_eq!(out.win_rate, 0.0, "stop exit is a losing round-trip");
    // The stop is entry × 0.98 (2%); entry is the actual open fill (≈100 plus
    // slippage), so derive the expected stop from it rather than the nominal.
    let entry = out
        .decisions
        .iter()
        .find(|d| d.action == "long_open")
        .and_then(|d| d.fill_price)
        .expect("an open fill price");
    let exit = out
        .decisions
        .iter()
        .find(|d| d.justification.as_deref().is_some_and(|j| j.contains("stop_loss")))
        .expect("a stop_loss exit decision must be recorded");
    assert!(
        (exit.fill_price.unwrap() - entry * 0.98).abs() < 1e-6,
        "stop exit fills at entry×0.98 = {}, got {:?}",
        entry * 0.98,
        exit.fill_price
    );
}

#[tokio::test]
async fn model_take_profit_exit_closes_at_target() {
    // Entry 100 → model target = 110 (10%). bar[2] rallies to high 111 (open
    // 105, no gap) so the exit fills at the target, 110 — a win.
    let b = bars(
        &[
            (100.0, 101.0, 99.0, 100.0),
            (100.0, 102.0, 99.5, 101.0),
            (105.0, 111.0, 104.0, 110.0),
            (110.0, 110.5, 109.0, 110.0),
        ],
        &scenario(4),
    );
    let out = run(strategy(0.0, 0.0), b, LONG_SL2_TP10).await;
    assert_eq!(out.n_trades, 2, "one open fill + one take-profit exit");
    assert_eq!(out.win_rate, 1.0, "take-profit exit is a winning round-trip");
    let entry = out
        .decisions
        .iter()
        .find(|d| d.action == "long_open")
        .and_then(|d| d.fill_price)
        .expect("an open fill price");
    let exit = out
        .decisions
        .iter()
        .find(|d| d.justification.as_deref().is_some_and(|j| j.contains("take_profit")))
        .expect("a take_profit exit decision must be recorded");
    assert!(
        (exit.fill_price.unwrap() - entry * 1.10).abs() < 1e-6,
        "target exit fills at entry×1.10 = {}, got {:?}",
        entry * 1.10,
        exit.fill_price
    );
}

#[tokio::test]
async fn gap_through_stop_fills_at_open() {
    // Entry 100 → model stop = 98. bar[2] gaps down, opening at 95 (already
    // below the stop). The exit fills at the bar open (95), not the stop.
    let b = bars(
        &[
            (100.0, 101.0, 99.0, 100.0),
            (100.0, 101.0, 99.5, 100.0),
            (95.0, 96.0, 94.0, 95.0),
            (95.0, 95.5, 94.5, 95.0),
        ],
        &scenario(4),
    );
    let out = run(strategy(0.0, 0.0), b, LONG_SL2_TP10).await;
    let exit = out
        .decisions
        .iter()
        .find(|d| d.justification.as_deref().is_some_and(|j| j.contains("stop_loss")))
        .expect("a stop_loss exit decision must be recorded");
    assert!(
        (exit.fill_price.unwrap() - 95.0).abs() < 1e-6,
        "gap-through exit fills at the bar open 95, got {:?}",
        exit.fill_price
    );
}

#[tokio::test]
async fn config_atr_stop_exit_when_model_omits() {
    // No model brackets; config stop_loss_atr_multiple = 1.0. With ~1.0 ATR
    // (high-low = 1 each bar) and entry 100, the config stop ≈ 99. bar[2] dips
    // to 98 → exit near the ATR stop. Proves the config floor fires.
    let canned = r#"{"action":"long_open","conviction":0.7,"justification":"enter long, no brackets"}"#;
    let b = bars(
        &[
            (100.0, 100.5, 99.5, 100.0),
            (100.0, 100.5, 99.5, 100.0),
            (99.5, 100.0, 98.0, 99.0),
            (99.0, 99.5, 98.5, 99.0),
        ],
        &scenario(4),
    );
    let out = run(strategy(1.0, 0.0), b, canned).await;
    assert!(
        out.decisions
            .iter()
            .any(|d| d.justification.as_deref().is_some_and(|j| j.contains("stop_loss"))),
        "config ATR stop must produce a stop_loss exit; decisions: {:?}",
        out.decisions.iter().map(|d| &d.justification).collect::<Vec<_>>()
    );
    assert_eq!(out.win_rate, 0.0, "the config-stop exit is a loss");
}

#[tokio::test]
async fn size_bps_override_changes_open_quantity() {
    // Same flat price path (no exit). Compare opened quantity with vs without a
    // model size_bps. Balanced risk_pct_per_trade = 0.015 (150 bps); a 1000-bps
    // override should open a materially larger position.
    let flat = &[
        (100.0, 100.1, 99.9, 100.0),
        (100.0, 100.1, 99.9, 100.0),
    ];
    let default_open = run(
        strategy(0.0, 0.0),
        bars(flat, &scenario(2)),
        r#"{"action":"long_open","conviction":0.7,"justification":"default size"}"#,
    )
    .await;
    let sized_open = run(
        strategy(0.0, 0.0),
        bars(flat, &scenario(2)),
        r#"{"action":"long_open","conviction":0.7,"justification":"big size","size_bps":1000}"#,
    )
    .await;
    let qty = |o: &RunOut| {
        o.decisions
            .iter()
            .find(|d| d.action == "long_open")
            .and_then(|d| d.fill_size)
            .expect("an open fill_size")
    };
    let dq = qty(&default_open);
    let sq = qty(&sized_open);
    assert!(
        sq > dq * 3.0,
        "size_bps=1000 (10% NAV) must open a much larger qty than the 1.5% default; default={dq}, sized={sq}"
    );
}
