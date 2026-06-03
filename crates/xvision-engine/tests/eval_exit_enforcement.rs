//! Integration coverage for eval-harness protective-exit enforcement
//! (plan `2026-06-03-001-fix-eval-harness-exit-enforcement`, units U2/U3).
//!
//! Three scenarios drive the backtest executor through the apply seam and
//! assert the deterministic risk controls fire WITHOUT requiring the trader
//! LLM to emit its own bracket:
//!
//!   R1 — a held long rides an adverse move past
//!        `risk.stop_loss_atr_multiple × ATR` from entry and is force-closed
//!        on the breaching bar, even though the trader emitted NO stop_loss
//!        bracket. (Fails on pre-fix code: the configured ATR stop was inert
//!        unless the model emitted `sl_atr_mult`.)
//!   R3 — once cumulative realized loss for the day exceeds
//!        `daily_loss_kill_pct × initial`, further opens are vetoed (rewritten
//!        to `hold`, recorded as a `risk` supervisor note).
//!   R3 — with `max_concurrent_positions = 2` and three eligible assets, only
//!        two opens are admitted; the third is vetoed.
//!
//! These mirror `eval_guardrails.rs` (same fresh_store / minimal_strategy /
//! MockDispatch scaffolding) and `eval_broker_circuit_breaker.rs`.

#![allow(deprecated)] // canonical_scenarios() — same pattern as eval_guardrails.rs

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

const MIGRATION_018: &str = include_str!("../migrations/018_agent_run_observability.sql");

async fn fresh_store() -> RunStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    for sql in [
        include_str!("../migrations/001_api_audit.sql"),
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/013_cli_jobs.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../migrations/027_run_bars_manifest.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../migrations/038_eval_runs_live_config.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    sqlx::query(MIGRATION_018).execute(&pool).await.unwrap();
    RunStore::new(pool)
}

fn store_pool(store: &RunStore) -> SqlitePool {
    store.pool_for_test()
}

/// Strategy over `assets`, 1-day cadence (every daily bar fires), `Balanced`
/// risk preset (stop_loss_atr_multiple = 2.0, max_concurrent_positions = 2,
/// daily_loss_kill_pct = 0.05) unless overridden by the caller.
fn strategy_with(agent_id: &str, assets: &[&str], preset: RiskPreset, cadence_minutes: u32) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "exit-enforcement test strategy".into(),
            plain_summary: "U2 protective-exit coverage".into(),
            creator: "@tester".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![],
            asset_universe: assets.iter().map(|s| (*s).into()).collect(),
            decision_cadence_minutes: cadence_minutes,
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
        risk: preset.expand(),
        mechanical_params: serde_json::json!({}),
        hypothesis: None,
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
    }
}

/// Trader response with NO protective bracket — the model leaves SL/TP unset,
/// exactly the failure mode where the configured ATR stop must take over.
fn trader_resp(action: &str) -> LlmResponse {
    let body = format!(r#"{{"action":"{action}","conviction":0.7,"justification":"test {action}"}}"#);
    LlmResponse {
        content: vec![ContentBlock::Text { text: body }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    }
}

fn sequenced_dispatch(actions: &[&str]) -> Arc<dyn LlmDispatch> {
    let resps: Vec<LlmResponse> = actions.iter().map(|a| trader_resp(a)).collect();
    Arc::new(MockDispatch::sequence(resps))
}

async fn count_notes_with_prefix(store: &RunStore, run_id: &str, prefix: &str) -> i64 {
    let pool = store_pool(store);
    let pattern = format!("{prefix}%");
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM supervisor_notes WHERE run_id = ? AND content LIKE ?",
    )
    .bind(run_id)
    .bind(pattern)
    .fetch_one(&pool)
    .await
    .unwrap()
}

// ─────────────────────────────────────────────────────────────────────────
// R1 — configured ATR stop force-closes a held position with no model bracket
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn configured_atr_stop_force_closes_held_long_without_model_bracket() {
    // Flat ~100 history (small ATR ≈ 2.0), open long, then a deep crash bar.
    // Entry fills at the bar AFTER the long_open. With ATR ≈ 2 and
    // stop_loss_atr_multiple = 2.0, the stop sits ≈ 4 below entry (~96). A
    // crash bar with low far below that must force a `stop_loss` close on the
    // breaching bar, BEFORE the trader is consulted that bar.
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTEXITATRSTOP0000000000A";
    // Daily cadence: each midnight-UTC daily bar fires a decision.
    let strategy = strategy_with(agent_id, &["BTC/USD"], RiskPreset::Balanced, 1_440);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // Bars: 5 flat warmup-ish bars (to seed ATR), open on bar 5, crash on
    // bar 7. The ATR(14) over the flat prefix is ~2 (high-low = 4 → TR≈4,
    // but flat closes keep it ~ (h-l)=4; multiple = 2 → stop ~ entry-? );
    // we keep the crash well past any plausible stop.
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let mut bars: Vec<Ohlcv> = Vec::new();
    for i in 0..8 {
        let (o, h, l, c) = if i == 7 {
            // Deep crash bar: low collapses far below any ATR stop.
            (100.0, 100.0, 60.0, 62.0)
        } else {
            (100.0, 101.0, 99.0, 100.0)
        };
        bars.push(Ohlcv {
            timestamp: start + Duration::days(i as i64),
            open: o,
            high: h,
            low: l,
            close: c,
            volume: 1_000.0,
        });
    }

    // Decisions: long_open once, then hold for the rest. The SL/TP check runs
    // pre-LLM, so on the crash bar the position closes before a hold is even
    // dispatched (we still supply enough hold responses to be safe).
    let dispatch =
        sequenced_dispatch(&["long_open", "hold", "hold", "hold", "hold", "hold", "hold", "hold"]);
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(bars);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    let decisions = store.read_decisions(&run.id).await.unwrap();
    // A `stop_loss` decision row must exist (the sltp force-close path records
    // `action = "stop_loss"`).
    let stop_rows = decisions.iter().filter(|d| d.action == "stop_loss").count();
    assert_eq!(
        stop_rows, 1,
        "configured ATR stop must force exactly one stop_loss close; decisions = {:?}",
        decisions.iter().map(|d| d.action.clone()).collect::<Vec<_>>()
    );
    // The stop-loss close must book a realized loss (entry ~100, exit ~60).
    let stop = decisions.iter().find(|d| d.action == "stop_loss").unwrap();
    assert!(
        stop.pnl_realized.unwrap_or(0.0) < 0.0,
        "stop-loss close must realize a loss; got {:?}",
        stop.pnl_realized
    );
}

// ─────────────────────────────────────────────────────────────────────────
// R3 — max_concurrent_positions vetoes the over-cap open
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn max_concurrent_positions_vetoes_third_simultaneous_open() {
    // Balanced preset caps concurrent positions at 2. Three assets each get a
    // long_open at the same timestamp; only the first two open, the third is
    // rewritten to `hold` and recorded as a `risk veto max_concurrent_positions`
    // note.
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTEXITMAXPOS00000000000A";
    let strategy = strategy_with(
        agent_id,
        &["BTC/USD", "ETH/USD", "SOL/USD"],
        RiskPreset::Balanced,
        1_440,
    );

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // Per-asset daily bar series sharing the same midnight-UTC timestamps so
    // all three assets are evaluated within each timestamp slot; the
    // open-position count grows as legs open within day0.
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let mut asset_bars: std::collections::BTreeMap<xvision_core::trading::AssetSymbol, Vec<Ohlcv>> =
        std::collections::BTreeMap::new();
    for (sym_idx, sym) in ["BTC/USD", "ETH/USD", "SOL/USD"].iter().enumerate() {
        let base = 100.0 + sym_idx as f64 * 10.0;
        let series: Vec<Ohlcv> = (0..2)
            .map(|i| Ohlcv {
                timestamp: start + Duration::days(i as i64),
                open: base,
                high: base + 1.0,
                low: base - 1.0,
                close: base,
                volume: 1_000.0,
            })
            .collect();
        asset_bars.insert(xvision_core::trading::AssetSymbol::from(*sym), series);
    }

    // Two gated timestamps (day0, day1) × 3 assets = 6 decisions, consumed in
    // (timestamp, asset)-sorted order: day0 BTC/ETH/SOL then day1 BTC/ETH/SOL.
    // Day0: all three attempt long_open (SOL vetoed at the cap). Day1: hold.
    let dispatch = sequenced_dispatch(&[
        "long_open", "long_open", "long_open", "hold", "hold", "hold",
    ]);
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::new().with_asset_bars(asset_bars);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    // Exactly one `max_concurrent_positions` veto note (the third open).
    let veto_count =
        count_notes_with_prefix(&store, &run.id, "risk veto `max_concurrent_positions`").await;
    assert_eq!(
        veto_count, 1,
        "third simultaneous open must be vetoed exactly once by max_concurrent_positions"
    );

    // At most two assets actually filled an open.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let filled_opens = decisions
        .iter()
        .filter(|d| d.fill_price.is_some() && d.fill_size.unwrap_or(0.0) > 0.0)
        .count();
    assert!(
        filled_opens <= 2,
        "no more than 2 concurrent opens may fill under max_concurrent_positions=2; got {filled_opens}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// R3 — daily_loss_kill_pct vetoes further opens after the day's loss budget
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn daily_loss_kill_vetoes_further_opens_after_loss_budget_breached() {
    // A custom risk config with a very tight daily-loss budget and ATR stop so
    // an opened long stops out at a loss, and a subsequent same-day open is
    // vetoed. We use the Conservative preset (max_concurrent_positions = 1,
    // daily_loss_kill_pct = 0.03) and drive: open → stop-out (loss) → attempt
    // re-open same day → veto.
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTEXITDAILYLOSS00000000A";
    // Conservative: max_concurrent_positions = 1, daily_loss_kill_pct = 0.03,
    // stop_loss_atr_multiple = 2.0.
    // 1-minute cadence so every per-minute bar on the SAME UTC day fires a
    // decision (the daily-loss window must not roll mid-test).
    let strategy = strategy_with(agent_id, &["BTC/USD"], RiskPreset::Conservative, 1);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // All bars share the SAME UTC day so the daily-loss window does not roll.
    // Use intraday-spaced bars on 2026-01-01. ATR stays small (flat ~100)
    // until a crash bar realizes a loss large enough to breach 3% of the
    // initial capital, then a later open on the same day is vetoed.
    let day = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let mut bars: Vec<Ohlcv> = Vec::new();
    for i in 0..10 {
        let (o, h, l, c) = if i == 4 {
            // Crash bar: deep loss on the open long.
            (100.0, 100.0, 50.0, 52.0)
        } else {
            (100.0, 101.0, 99.0, 100.0)
        };
        bars.push(Ohlcv {
            // Same calendar day; spaced by minutes so timestamps are distinct.
            timestamp: day + Duration::minutes(i as i64),
            open: o,
            high: h,
            low: l,
            close: c,
            volume: 1_000.0,
        });
    }

    // long_open early, hold through the crash/stop-out, then attempt to open
    // again later the same day (must be vetoed once the loss budget is gone).
    let dispatch = sequenced_dispatch(&[
        "long_open", "hold", "hold", "hold", "hold", "long_open", "long_open", "long_open",
        "long_open", "long_open",
    ]);
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(bars);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    // At least one daily-loss veto note must appear after the stop-out.
    let veto_count = count_notes_with_prefix(&store, &run.id, "risk veto `daily_loss_kill`").await;
    assert!(
        veto_count >= 1,
        "a same-day open after the daily-loss budget is breached must be vetoed at least once; got {veto_count}"
    );
}
