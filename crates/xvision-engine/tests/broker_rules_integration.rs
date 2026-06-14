//! Integration tests for the broker-rule order-emission hook in the backtest
//! executor. Exercises the full pipeline from trader action → broker check →
//! (no) fill → finding emission.
//!
//! Contract: eval-broker-rule-findings (V2E item 23).
//!
//! Test matrix:
//! 1. Violating order rejected → no fill row in decisions, finding in findings.
//! 2. Non-violating order passes → fill row in decisions, no finding.
//! 3. Broker-rejected aggregate finding appears when violations occur.
//! 4. Rule set selection by asset_class (Crypto / Equity).
//!
//! Note: the backtest executor uses Market/GTC orders in v1. The broker-rule
//! check fires on `long_open` and `short_open` actions. To trigger a rule
//! violation we need to construct a scenario where the order fails a check —
//! the only v1 check that fires for Market/GTC orders is `min_order_size`.
//!
//! To trigger a `min_order_size_violation`, we set:
//!   risk_pct = very small (e.g. 0.001%)
//!   equity = $10,000
//!   next_bar_open = high price (e.g. $100,000)
//!   → estimated_qty = 10_000 × 0.00001 / 100_000 = $0.001 — well below $1.00
//!
//! The tiny `risk_pct` is achieved by using a custom `RiskConfig` with
//! `risk_pct_per_trade = 0.00001` (0.001%).

#![allow(deprecated)] // canonical_scenarios()

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_core::market::Ohlcv;
use xvision_engine::agent::llm::{ContentBlock, LlmResponse, MockDispatch, StopReason};
use xvision_engine::eval::executor::{Executor, RunExecutor};
use xvision_engine::eval::findings::Severity;
use xvision_engine::eval::run::{Run, RunMode};
use xvision_engine::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, BarGranularity, CalendarRef, Capital, DataSource, Fees,
    FillModel, LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode,
    Scenario, ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings,
};
use xvision_engine::eval::store::RunStore;
use xvision_engine::safety::VenueLabel;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;
use xvision_engine::tools::ToolRegistry;

// ── Infrastructure ────────────────────────────────────────────────────────────

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
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/013_cli_jobs.sql"))
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
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/017_eval_findings_review_columns.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/018_agent_run_observability.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // V2E trace-surface foundation (determinism_receipts +
    // eval_findings.evidence_cycle_ids_json + .produced_by_check).
    sqlx::query(include_str!("../migrations/026_trace_surface_foundation.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // V2E candle integrity + manifest (bars_content_hash,
    // manifest_canonical, bars_manifest on eval_runs).
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
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
    RunStore::new(pool)
}

/// Build a minimal crypto scenario. `risk_pct` allows forcing below-minimum
/// orders when set very small.
fn crypto_scenario(asset_class: AssetClass, symbol: &str, _venue_symbol: &str) -> Scenario {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let end = start + Duration::days(3);
    Scenario {
        id: format!("test-{}-{}", symbol.to_lowercase(), ulid::Ulid::new()),
        parent_scenario_id: None,
        source: ScenarioSource::User,
        display_name: format!("test {symbol}"),
        description: "broker rules integration test".into(),
        tags: vec![],
        notes: None,
        asset_class,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow { start, end },
        granularity: BarGranularity::Day1,
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        data_source: DataSource::AlpacaHistorical {
            feed: None,
            adjustment: AdjustmentMode::Raw,
        },
        venue: VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees {
                maker_bps: 10,
                taker_bps: 25,
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
            borrow_bps_per_day: 5.0,
        },
        replay_mode: ReplayMode::Continuous,
        capital: Capital {
            initial: 10_000.0,
            currency: "USD".into(),
        },
        bar_cache_policy: BarCachePolicy {
            cache_key: format!("test-{}", symbol.to_lowercase()),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        warmup_bars: 0,
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: Utc::now(),
        created_by: "test".into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    }
}

/// Build a strategy with a custom `risk_pct_per_trade` value.
fn strategy_with_risk_pct(agent_id: &str, risk_pct: f64) -> Strategy {
    let mut risk = RiskPreset::Balanced.expand();
    risk.risk_pct_per_trade = risk_pct;
    Strategy {
        manifest: PublicManifest {
            id: agent_id.into(),
            display_name: "broker-rule test strategy".into(),
            plain_summary: "V2E broker-rule-findings coverage".into(),
            creator: "@tester".into(),
            template: "trend_follow".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 1_440, // daily
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
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4.6+".into(),
            allowed_tools: vec![],
            provider: None,
            model: None,
        }),
        risk,
        hypothesis: None,
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

fn trader_resp(action: &str) -> LlmResponse {
    let body = format!(r#"{{"action":"{action}","conviction":0.7,"justification":"test {action}"}}"#);
    LlmResponse {
        content: vec![ContentBlock::Text { text: body }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    }
}

fn sequenced_dispatch(actions: &[&str]) -> Arc<xvision_engine::agent::llm::MockDispatch> {
    let resps: Vec<LlmResponse> = actions.iter().map(|a| trader_resp(a)).collect();
    Arc::new(MockDispatch::sequence(resps))
}

/// Build 3 daily bars at a given reference price.
fn daily_bars_at(price: f64, count: usize) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    (0..count)
        .map(|i| Ohlcv {
            timestamp: start + Duration::days(i as i64),
            open: price,
            high: price + 100.0,
            low: price - 100.0,
            close: price + 10.0,
            volume: 500.0,
        })
        .collect()
}

// ── Test: broker-rejected order leaves no fill ────────────────────────────────

/// Contract acceptance: "order rejection: a violating order does not appear in
/// trades.jsonl; the cycle's intended action is recorded in decisions.jsonl
/// but outcomes.jsonl shows no fill."
///
/// We trigger min_order_size_violation:
///   equity = $10,000
///   risk_pct = 0.000001 (0.0001%)
///   next_bar_open ≈ $100,000
///   → estimated_qty ≈ 0.0000001 BTC → notional ≈ $0.01 (below $1.00)
#[tokio::test]
async fn broker_rejected_order_records_decision_but_no_fill() {
    let store = fresh_store().await;
    let scenario = crypto_scenario(AssetClass::Crypto, "BTC", "BTC/USD");

    // Tiny risk_pct → tiny qty → below min notional.
    let agent_id = "01BROKERRULETEST0000000001";
    let strategy = strategy_with_risk_pct(agent_id, 0.000001); // 0.0001% of $10k → $0.01 per trade

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    // 3 bars at $100,000 open. estimated_qty = $10,000 × 0.000001 / $100,000 = $0.0001 BTC → $0.01 notional.
    let bars = daily_bars_at(100_000.0, 3);
    let dispatch = sequenced_dispatch(&["long_open", "long_open", "long_open"]);
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(bars);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run must complete even with broker rejections");

    // 1. Decisions are recorded (the strategy's intent is preserved).
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 3, "3 bars → 3 decision rows in decisions.jsonl");
    for (i, d) in decisions.iter().enumerate() {
        assert_eq!(
            d.action, "long_open",
            "decision {i}: trader intent must be recorded as long_open"
        );
    }

    // 2. No fills occurred (all were rejected by min_order_size_violation).
    for (i, d) in decisions.iter().enumerate() {
        assert!(
            d.fill_price.is_none(),
            "decision {i}: fill_price must be None for a broker-rejected order"
        );
        assert!(
            d.fill_size.is_none() || d.fill_size == Some(0.0),
            "decision {i}: fill_size must be None/0 for a broker-rejected order; got {:?}",
            d.fill_size,
        );
    }

    // 3. broker_rule_violation findings were emitted.
    let findings = store.read_findings(&run.id).await.unwrap();
    let violation_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.kind == "broker_rule_violation")
        .collect();
    assert!(
        !violation_findings.is_empty(),
        "at least one broker_rule_violation finding must be emitted"
    );

    // At least one per-decision finding + one aggregate summary finding.
    let per_decision: Vec<_> = violation_findings
        .iter()
        .filter(|f| {
            f.evidence
                .get("specific_rule")
                .and_then(|v| v.as_str())
                .map(|r| r == "min_order_size_violation")
                .unwrap_or(false)
        })
        .collect();
    assert!(
        !per_decision.is_empty(),
        "per-decision min_order_size_violation findings must be present"
    );

    // Findings must carry the right severity.
    for f in &per_decision {
        assert_eq!(
            f.severity,
            Severity::Critical,
            "min_order_size_violation must be Critical"
        );
    }

    // Aggregate summary finding must be present.
    // produced_by_check moved from evidence blob → typed Finding field with
    // the V2E trace-surface foundation.
    let aggregate: Vec<_> = violation_findings
        .iter()
        .filter(|f| f.produced_by_check.as_deref() == Some("broker:run_aggregate"))
        .collect();
    assert!(
        !aggregate.is_empty(),
        "a run-level aggregate finding must be emitted when broker_rejected_orders > 0"
    );

    // The aggregate finding's evidence must carry broker_rejected_orders count.
    let agg_count = aggregate[0]
        .evidence
        .get("broker_rejected_orders")
        .and_then(|v| v.as_u64())
        .expect("aggregate finding must carry broker_rejected_orders count");
    assert!(
        agg_count > 0,
        "broker_rejected_orders in aggregate finding must be > 0"
    );
}

// ── Test: non-violating order fills normally ──────────────────────────────────

/// A normal order (adequate notional, correct type/TIF) should fill and
/// produce no broker_rule_violation finding.
#[tokio::test]
async fn non_violating_order_fills_and_no_violation_finding() {
    let store = fresh_store().await;
    let scenario = crypto_scenario(AssetClass::Crypto, "BTC", "BTC/USD");

    // 5% risk per trade on $10k at $50k → $500 notional → well above $1.00.
    let agent_id = "01BROKERRULETEST0000000002";
    let strategy = strategy_with_risk_pct(agent_id, 0.05);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bars = daily_bars_at(50_000.0, 2);
    let dispatch = sequenced_dispatch(&["long_open", "flat"]);
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(bars);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("backtest run must complete");

    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert_eq!(decisions.len(), 2, "2 bars → 2 decisions");

    // First decision (long_open) must have filled.
    assert!(
        decisions[0].fill_price.is_some(),
        "long_open with adequate notional must fill"
    );
    assert!(
        decisions[0].fill_size.unwrap_or(0.0) > 0.0,
        "fill_size must be positive for a valid order"
    );

    // No broker_rule_violation findings for a valid strategy.
    let findings = store.read_findings(&run.id).await.unwrap();
    let violations: Vec<_> = findings
        .iter()
        .filter(|f| f.kind == "broker_rule_violation")
        .collect();
    assert!(
        violations.is_empty(),
        "no broker_rule_violation findings for a well-formed order; got {violations:?}"
    );
}

// ── Test: equity scenario uses no-op rules ────────────────────────────────────

/// Contract acceptance: "an Equity scenario uses equity rules" (no-op stub).
/// Orders always pass; no broker_rule_violation finding is emitted.
///
/// We use a Crypto asset class but override the scenario's asset_class to
/// Equity to exercise the rule-set selection path. The underlying order
/// parameters are the same tiny-notional ones that would fail crypto rules.
#[tokio::test]
async fn equity_scenario_no_op_rules_order_always_accepted() {
    let store = fresh_store().await;
    // Build an Equity scenario directly (no whitelist validation needed here
    // since we're injecting bars directly).
    let mut scenario = crypto_scenario(AssetClass::Equity, "AAPL", "AAPL");
    // Keep asset_class = Equity to select AlpacaEquityRules (no-op).
    scenario.asset_class = AssetClass::Equity;

    // Tiny risk_pct that would trigger min_order_size on crypto.
    let agent_id = "01BROKERRULETEST0000000003";
    let strategy = strategy_with_risk_pct(agent_id, 0.000001);

    let mut run = Run::new_queued(
        strategy.manifest.id.clone(),
        scenario.id.clone(),
        RunMode::Backtest,
    );
    store.create(&run).await.unwrap();

    let bars = daily_bars_at(100_000.0, 2);
    let dispatch = sequenced_dispatch(&["long_open", "flat"]);
    let tools = Arc::new(ToolRegistry::empty());
    let executor = Executor::with_bars(bars);

    executor
        .run(&mut run, &strategy, &scenario, &[], dispatch, tools, &store)
        .await
        .expect("equity backtest must complete");

    // With equity rules (no-op), the tiny-notional order should fill.
    let decisions = store.read_decisions(&run.id).await.unwrap();
    assert!(!decisions.is_empty(), "should have at least one decision");

    let first = &decisions[0];
    assert_eq!(first.action, "long_open");
    // The equity no-op lets the order through → it fills.
    assert!(
        first.fill_price.is_some(),
        "equity no-op rules must not block the order; expected a fill"
    );

    // No broker_rule_violation findings.
    let findings = store.read_findings(&run.id).await.unwrap();
    let violations: Vec<_> = findings
        .iter()
        .filter(|f| f.kind == "broker_rule_violation")
        .collect();
    assert!(
        violations.is_empty(),
        "equity no-op must not emit violation findings; got {violations:?}"
    );
}
