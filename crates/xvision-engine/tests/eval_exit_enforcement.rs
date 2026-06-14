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
//! Shared eval harness scaffolding lives in `tests/support/eval_harness.rs`.

#![allow(deprecated)] // canonical_scenarios() — same pattern as eval_guardrails.rs

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmResponse, MockDispatch, StopReason};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::canonical_scenarios;
use xvision_engine::strategies::risk::{RiskConfig, RiskPreset};
use xvision_engine::tools::ToolRegistry;
use xvision_filters::{
    ActivationMode, AgentContextTemplateId, Condition, ConditionItem, ConditionTree, Filter, FilterId,
    FilterStatus, IndicatorRef, Operand, Operator, ScanCadence, StrategyId, Symbol, Timeframe,
    WakeInPosition, DEFAULT_AGENT_CONTEXT_TEMPLATE,
};

mod support;

use support::eval_harness::{
    count_notes_with_prefix, fresh_store, sequenced_dispatch, strategy_with, strategy_with_risk, trader_resp,
};

/// A `long_open` carrying an explicit `take_profit_pct` bracket, followed by
/// `hold`s. Used to drive a deterministic winning round-trip (TP exit).
fn long_open_with_tp_then_holds(tp_pct: f64, holds: usize) -> Arc<dyn LlmDispatch> {
    let open = LlmResponse {
        content: vec![ContentBlock::Text {
            text: format!(
                r#"{{"action":"long_open","conviction":0.8,"justification":"breakout","take_profit_pct":{tp_pct}}}"#
            ),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    };
    let mut resps = vec![open];
    for _ in 0..holds {
        resps.push(trader_resp("hold"));
    }
    Arc::new(MockDispatch::sequence(resps))
}

fn always_true_filter(strategy_id: &str, wake_when_in_position: WakeInPosition) -> Filter {
    Filter {
        id: FilterId::new("01TESTFILTERALWAYSTRUE000000"),
        strategy_id: StrategyId::new(strategy_id),
        display_name: "always true test filter".into(),
        description: None,
        status: FilterStatus::Active,
        asset_scope: vec![Symbol::new("BTC/USD")],
        timeframe: Timeframe::new("1d"),
        scan_cadence: ScanCadence::BarClose,
        conditions: ConditionTree::All(vec![ConditionItem::Leaf(Condition {
            lhs: Operand::Indicator(IndicatorRef::close()),
            op: Operator::Gt,
            rhs: Operand::Numeric(0.0),
        })]),
        fire: None,
        cooldown_bars: 0,
        max_wakeups_per_day: None,
        wake_when_in_position,
        agent_context_template: AgentContextTemplateId::new(DEFAULT_AGENT_CONTEXT_TEMPLATE),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// R1 — configured ATR stop force-closes a held position with no model bracket
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn configured_atr_stop_force_closes_held_long_without_model_bracket() {
    // Flat ~100 history (small ATR ≈ 2.0), open long, then a deep crash bar.
    // The first 15 bars are pre-entry decisions so the configured ATR stop has
    // enough history when the long opens. Entry fills at the bar AFTER the
    // long_open. With ATR ≈ 2 and
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

    // Bars: 15 flat pre-entry bars to seed ATR, open on bar 15, hold on bar
    // 16, crash on bar 17, and fill the stop at bar 18's crashed open. The
    // ATR(14) over the flat prefix is ~2; we keep the crash well past any
    // plausible stop.
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let mut bars: Vec<Ohlcv> = Vec::new();
    for i in 0..20 {
        let (o, h, l, c) = if i == 17 {
            // Deep crash bar: low collapses far below any ATR stop.
            (100.0, 100.0, 60.0, 62.0)
        } else if i == 18 {
            // The SLTP path fills at the next bar's open.
            (62.0, 63.0, 60.0, 62.0)
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

    // Decisions: hold through ATR warmup, long_open once, then hold for the
    // rest. The SL/TP check runs pre-LLM, so on the crash bar the position
    // closes before a hold is even dispatched.
    let mut actions = vec!["hold"; 15];
    actions.extend(["long_open", "hold", "hold", "hold", "hold"]);
    let dispatch = sequenced_dispatch(&actions);
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
        stop_rows,
        1,
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

#[tokio::test]
async fn configured_atr_stop_runs_before_filter_gate_when_position_is_open() {
    // Reproduces PF-17: a filter-gated strategy opens once, then suppresses
    // in-position wakeups. The deterministic ATR stop must still run on the
    // crash bar before the filter skip can bypass the agent pipeline.
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTFILTERSLTP0000000000A";
    let mut strategy = strategy_with(agent_id, &["BTC/USD"], RiskPreset::Balanced, 1_440);
    strategy.activation_mode = ActivationMode::FilterGated;
    strategy.filter = Some(always_true_filter(agent_id, WakeInPosition::Never));

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let mut bars: Vec<Ohlcv> = Vec::new();
    for i in 0..20 {
        let (o, h, l, c) = if i == 17 {
            (100.0, 100.0, 60.0, 62.0)
        } else if i == 18 {
            (62.0, 63.0, 60.0, 62.0)
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

    let mut actions = vec!["hold"; 15];
    actions.extend(["long_open", "hold", "hold", "hold", "hold"]);
    let dispatch = sequenced_dispatch(&actions);
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(bars);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    let decisions = store.read_decisions(&run.id).await.unwrap();
    let actions_seen = decisions.iter().map(|d| d.action.as_str()).collect::<Vec<_>>();
    let open_idx = actions_seen
        .iter()
        .position(|action| *action == "long_open")
        .expect("test must open a long before SL/TP enforcement is meaningful");
    let stop_idx = actions_seen
        .iter()
        .position(|action| *action == "stop_loss")
        .expect("test must force one stop_loss row");
    assert_eq!(
        &actions_seen[(open_idx + 1)..stop_idx],
        &[] as &[&str],
        "filter-suppressed in-position bars must not emit extra trader decision rows between open and stop; actions = {actions_seen:?}"
    );

    let stop_rows = decisions.iter().filter(|d| d.action == "stop_loss").count();
    assert_eq!(
        stop_rows,
        1,
        "filter-gated in-position bars must still run SL/TP before skipping; decisions = {:?}",
        decisions.iter().map(|d| d.action.clone()).collect::<Vec<_>>()
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
    for (sym_idx, sym) in [
        xvision_core::trading::AssetSymbol::Btc,
        xvision_core::trading::AssetSymbol::Eth,
        xvision_core::trading::AssetSymbol::Sol,
    ]
    .into_iter()
    .enumerate()
    {
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
        asset_bars.insert(sym, series);
    }

    // Two gated timestamps (day0, day1) × 3 assets = 6 decisions, consumed in
    // (timestamp, asset)-sorted order: day0 BTC/ETH/SOL then day1 BTC/ETH/SOL.
    // Day0: all three attempt long_open (SOL vetoed at the cap). Day1: hold.
    let dispatch = sequenced_dispatch(&["long_open", "long_open", "long_open", "hold", "hold", "hold"]);
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::new().with_asset_bars(asset_bars);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    // Exactly one `max_concurrent_positions` veto note (the third open).
    let veto_count = count_notes_with_prefix(&store, &run.id, "risk veto `max_concurrent_positions`").await;
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
    // Custom risk: a deliberately tight daily-loss budget (0.1% of initial)
    // so a single small stop-out loss breaches it, plus a configured ATR stop
    // so the open long actually stops out. 1-minute cadence so every
    // per-minute bar on the SAME UTC day fires a decision (the daily-loss
    // window must not roll mid-test).
    let risk = RiskConfig {
        risk_pct_per_trade: 0.010,
        max_concurrent_positions: 5, // not the constraint under test
        max_leverage: 2.0,
        stop_loss_atr_multiple: 2.0,
        daily_loss_kill_pct: 0.001, // 0.1% of initial — tight on purpose
        max_position_pct_nav: 20.0,
        max_funding_pay_8h: 0.0,
        min_liq_distance_pct: 0.0,
        max_total_exposure_pct: 0.0,
    };
    let strategy = strategy_with_risk(agent_id, &["BTC/USD"], risk, 1);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // All bars share the SAME UTC day so the daily-loss window does not roll.
    // Use intraday-spaced bars on 2026-01-01. ATR stays small (flat ~100)
    // across a 15-bar pre-entry history, then a crash bar realizes a loss
    // large enough to breach the daily budget. A later open on the same day is
    // vetoed.
    let day = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let mut bars: Vec<Ohlcv> = Vec::new();
    for i in 0..20 {
        // The stop is TRIGGERED by the crash bar's low (i == 17) but the exit
        // FILLS at the next bar's open, so bar 18 must also be crashed for the
        // close to realize a loss. Bars 6+ recover to ~100 (where re-opens are
        // attempted and must be vetoed once the loss budget is spent).
        let (o, h, l, c) = if i == 17 {
            (100.0, 100.0, 50.0, 52.0) // crash: triggers the ATR stop
        } else if i == 18 {
            (52.0, 53.0, 50.0, 52.0) // stop fills here at open=52 → realized loss
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

    // Hold through ATR warmup, open, hold into the crash, then attempt to open
    // again after the stop-out on the same day. The stop bar itself skips the
    // LLM, so the post-stop `long_open` is the next consumed response.
    let mut actions = vec!["hold"; 15];
    actions.extend(["long_open", "hold", "long_open", "long_open", "long_open"]);
    let dispatch = sequenced_dispatch(&actions);
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

// ─────────────────────────────────────────────────────────────────────────
// U4 — round-trip accounting: win_rate reflects realized round-trip PnL,
// n_trades is the leg count.
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn winning_round_trip_yields_win_rate_one_and_two_legs() {
    // Open a long at ~100, hit a +6% take-profit on a later bar. That is ONE
    // closed round-trip with positive realized PnL → win_rate == 1.0. The
    // fill-leg count `n_trades` is 2 (the open leg + the TP close leg).
    let store = fresh_store().await;
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash-2024-08 scenario must exist");

    let agent_id = "01TESTEXITWINRATE0000000000A";
    let strategy = strategy_with(agent_id, &["BTC/USD"], RiskPreset::Balanced, 1_440);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // Flat ~100, then a bar that rallies through +6% (high >= 106) to trigger
    // the take-profit. Entry fills at the bar after long_open (~100).
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let mut bars: Vec<Ohlcv> = Vec::new();
    for i in 0..6 {
        let (o, h, l, c) = if i == 4 {
            // Rally bar: high well above the +6% TP (106).
            (100.0, 112.0, 100.0, 110.0)
        } else if i == 5 {
            // The SLTP path fills at the next bar's open, so keep that open
            // above the entry to make this a winning round-trip.
            (110.0, 112.0, 109.0, 110.0)
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

    let dispatch = long_open_with_tp_then_holds(6.0, 6);
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(bars);

    let metrics = executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run should complete");

    // The take-profit close must have recorded a winning round-trip.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    let tp_close = decisions
        .iter()
        .find(|d| d.action == "take_profit")
        .expect("a take_profit close row must exist");
    assert!(
        tp_close.pnl_realized.unwrap_or(0.0) > 0.0,
        "take-profit close must realize a gain; got {:?}",
        tp_close.pnl_realized
    );

    assert!(
        (metrics.win_rate - 1.0).abs() < 1e-9,
        "one winning round-trip → win_rate must be 1.0, got {}",
        metrics.win_rate
    );
    // Leg-count semantics: open leg + take-profit close leg = 2.
    assert_eq!(
        metrics.n_trades, 2,
        "n_trades counts fill legs (open + TP close) for one round-trip, got {}",
        metrics.n_trades
    );
}
