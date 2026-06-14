use std::sync::Arc;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use serde_json::json;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::autooptimizer::canary::{build_sabotaged_strategy, run_honesty_check};
use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
use xvision_engine::autooptimizer::eval_adapter::PaperTestRunner;
use xvision_engine::autooptimizer::gate::{GateInput, GateVerdict, Objective};
use xvision_engine::autooptimizer::mutator::Mutator;
use xvision_engine::eval::{
    AdjustmentMode, AssetClass, BarCachePolicy, BarGranularity, CalendarRef, DataSource, Fees, FillModel,
    LatencyModel, LimitOrderFill, MarketOrderFill, MetricsSummary, QuoteCurrency, RefreshPolicy, ReplayMode,
    Scenario, ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings,
};
use xvision_engine::safety::VenueLabel;
use xvision_engine::strategies::Strategy;
use xvision_engine::Capital;

fn make_strategy() -> Strategy {
    let v = json!({
        "manifest": {
            "id": "01HZCANARY",
            "display_name": "Canary Test",
            "plain_summary": "",
            "creator": "@test",
            "template": "custom",
            "regime_fit": [],
            "asset_universe": ["BTC/USD"],
            "decision_cadence_minutes": 60,
            "required_tools": [],
            "risk_preset_or_config": "balanced"
        },
        "agents": [{"agent_id": "01HZAGENT1", "role": "trader"}],
        "risk": {
            "risk_pct_per_trade": 0.015,
            "max_concurrent_positions": 2,
            "max_leverage": 3.0,
            "stop_loss_atr_multiple": 2.0,
            "daily_loss_kill_pct": 0.05
        }
    });
    serde_json::from_value(v).expect("fixture strategy deserializes")
}

fn make_scenario() -> Scenario {
    Scenario {
        id: "sc_canary".into(),
        parent_scenario_id: None,
        source: ScenarioSource::User,
        display_name: "canary".into(),
        description: "".into(),
        tags: vec![],
        notes: None,
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2025, 1, 2, 0, 0, 0).unwrap(),
        },
        granularity: BarGranularity::Hour1,
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
        capital: Capital::default(),
        bar_cache_policy: BarCachePolicy {
            cache_key: "canary".into(),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        warmup_bars: 0,
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
        created_by: "test".into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    }
}

struct StubLlmDispatch;

#[async_trait]
impl LlmDispatch for StubLlmDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        Ok(LlmResponse {
            content: vec![ContentBlock::Text { text: "{}".into() }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 0,
            output_tokens: 0,
        })
    }
}

fn make_mutator() -> Mutator {
    Mutator {
        provider: "stub".into(),
        model: "stub-model".into(),
        dispatch: Arc::new(StubLlmDispatch) as Arc<dyn LlmDispatch + Send + Sync>,
        max_retries: 0,
    }
}

struct ConstMetricsTester {
    sharpe: f64,
}

#[async_trait]
impl PaperTestRunner for ConstMetricsTester {
    async fn run(&self, _strategy: &Strategy, _scenario: &Scenario) -> anyhow::Result<MetricsSummary> {
        Ok(MetricsSummary {
            sharpe: self.sharpe,
            // A run with a non-zero sharpe must have actually traded; keep
            // n_trades > 0 so the B28 zero-trade neutralization does not apply
            // and these metrics reach the gate verbatim.
            n_trades: 5,
            ..MetricsSummary::default()
        })
    }
}

fn gate_builder(
    parent_day: &MetricsSummary,
    child_day: &MetricsSummary,
    parent_untouched: &MetricsSummary,
    child_untouched: &MetricsSummary,
) -> GateInput {
    GateInput {
        parent_day_metrics: parent_day.clone(),
        child_day_metrics: child_day.clone(),
        parent_untouched_metrics: parent_untouched.clone(),
        child_untouched_metrics: child_untouched.clone(),
        min_improvement: 0.1,
        objective: Default::default(),
    }
}

/// B28 regression scaffolding: a tester whose legitimate (parent) runs succeed
/// with real metrics, and whose CANARY runs complete with ZERO trades — exactly
/// what a `kill-trades` sabotage produces in production. Every order is $0
/// notional → rejected by broker-rule validation → no fill, but the backtest
/// still COMPLETES (`RunStatus::Completed`) with `n_trades == 0` and all-zero
/// metrics. It does NOT error and does NOT leave `metrics_json` NULL.
struct ZeroTradeCanaryTester {
    parent_total_return: f64,
}

#[async_trait]
impl PaperTestRunner for ZeroTradeCanaryTester {
    async fn run(&self, _strategy: &Strategy, _scenario: &Scenario) -> anyhow::Result<MetricsSummary> {
        Ok(MetricsSummary {
            total_return_pct: self.parent_total_return,
            sharpe: self.parent_total_return,
            n_trades: 7,
            ..MetricsSummary::default()
        })
    }

    async fn run_canary(
        &self,
        _strategy: &Strategy,
        _scenario: &Scenario,
        _sabotage_variant: &str,
    ) -> anyhow::Result<MetricsSummary> {
        // Real zero-trade canary: the run completes successfully with no fills.
        Ok(MetricsSummary {
            n_trades: 0,
            ..MetricsSummary::default()
        })
    }
}

/// B28 (narrowed): a tester whose CANARY run hits a GENUINE backtest error —
/// NOT a zero-trade completion. The neutral fallback is zero-trade-only, so this
/// must PROPAGATE rather than be masked as a passed honesty check.
struct ErroringCanaryTester {
    parent_total_return: f64,
}

#[async_trait]
impl PaperTestRunner for ErroringCanaryTester {
    async fn run(&self, _strategy: &Strategy, _scenario: &Scenario) -> anyhow::Result<MetricsSummary> {
        Ok(MetricsSummary {
            total_return_pct: self.parent_total_return,
            sharpe: self.parent_total_return,
            n_trades: 7,
            ..MetricsSummary::default()
        })
    }

    async fn run_canary(
        &self,
        _strategy: &Strategy,
        _scenario: &Scenario,
        _sabotage_variant: &str,
    ) -> anyhow::Result<MetricsSummary> {
        anyhow::bail!("provider_outage: 503 from inference endpoint (genuine fault, not zero-trade)")
    }
}

fn gate_builder_total_return(
    parent_day: &MetricsSummary,
    child_day: &MetricsSummary,
    parent_untouched: &MetricsSummary,
    child_untouched: &MetricsSummary,
) -> GateInput {
    GateInput {
        parent_day_metrics: parent_day.clone(),
        child_day_metrics: child_day.clone(),
        parent_untouched_metrics: parent_untouched.clone(),
        child_untouched_metrics: child_untouched.clone(),
        min_improvement: 0.1,
        objective: Objective::TotalReturn,
    }
}

/// B28: a zero-trade sabotage canary under `--objective total_return` must NOT
/// take down the cycle. The canary completes with `n_trades == 0`; it is scored
/// as a neutral (no-improvement) sentinel so the gate correctly rejects it and
/// the honesty check PASSES and completes (the completion record is written and
/// the cycle lock is released normally).
#[tokio::test]
async fn run_honesty_check_total_return_zero_trade_canary_passes() {
    let base = make_strategy();
    let mutator = make_mutator();
    let scenario = make_scenario();
    let config = AutoOptimizerConfig::default();
    // Parent legitimately makes a positive total_return; canary trades zero.
    let tester = ZeroTradeCanaryTester {
        parent_total_return: 5.0,
    };

    let result = run_honesty_check(
        &base,
        &mutator,
        &tester,
        gate_builder_total_return,
        &scenario,
        &scenario,
        &config,
        0, // seed 0 → kill-trades (zeroed position sizing → zero trades)
    )
    .await
    .expect("zero-trade canary must NOT error out the honesty check (B28)");

    assert!(
        result.passed_check,
        "honesty check must PASS: a zero-trade sabotage must be rejected, not look good"
    );
    assert!(
        matches!(result.gate_verdict, GateVerdict::Fail { .. }),
        "gate verdict must reject the neutral-scored sabotage canary"
    );
    assert_eq!(result.sabotage_variant, "kill-trades");
}

/// B28 (narrowed): the neutral fallback is zero-trade-ONLY. A GENUINE canary
/// backtest error (provider outage, panic, malformed scenario) must PROPAGATE
/// out of `run_honesty_check` rather than be silently scored as a passed honesty
/// check. Masking real faults as "honesty check passed" would hide broken
/// infrastructure; the cycle lock is still released because `run_cycle_cmd`
/// releases it unconditionally before propagating the error.
#[tokio::test]
async fn run_honesty_check_propagates_genuine_canary_error() {
    let base = make_strategy();
    let mutator = make_mutator();
    let scenario = make_scenario();
    let config = AutoOptimizerConfig::default();
    let tester = ErroringCanaryTester {
        parent_total_return: 5.0,
    };

    let result = run_honesty_check(
        &base,
        &mutator,
        &tester,
        gate_builder_total_return,
        &scenario,
        &scenario,
        &config,
        0,
    )
    .await;

    assert!(
        result.is_err(),
        "a genuine canary error must propagate, not be masked as a passed honesty check"
    );
    let msg = format!("{:#}", result.unwrap_err());
    assert!(
        msg.contains("provider_outage"),
        "propagated error should be the genuine fault; got: {msg}"
    );
}

#[test]
fn build_sabotaged_strategy_is_deterministic() {
    let base = make_strategy();
    let (a, av) = build_sabotaged_strategy(&base, 42);
    let (b, bv) = build_sabotaged_strategy(&base, 42);
    assert_eq!(a, b, "same seed must produce identical sabotaged strategy");
    assert_eq!(av, bv, "same seed must produce the same sabotage variant");
    // 42 % 3 == 0 → kill-trades (zeroed position sizing).
    assert_eq!(av.as_str(), "kill-trades");
}

#[test]
fn sabotaged_strategy_differs_from_base() {
    let base = make_strategy();
    for seed in 0_u64..=2 {
        let (sabotaged, _variant) = build_sabotaged_strategy(&base, seed);
        assert_ne!(
            sabotaged, base,
            "seed {seed} sabotaged strategy must differ from base"
        );
    }
}

#[tokio::test]
async fn run_honesty_check_with_bad_child_metrics_passes() {
    let base = make_strategy();
    let mutator = make_mutator();
    let scenario = make_scenario();
    let config = AutoOptimizerConfig::default();
    let tester = ConstMetricsTester { sharpe: -5.0 };

    let result = run_honesty_check(
        &base,
        &mutator,
        &tester,
        gate_builder,
        &scenario,
        &scenario,
        &config,
        0,
    )
    .await
    .expect("honesty check must not error");

    assert!(
        result.passed_check,
        "honesty check must pass when sabotaged mutation is rejected"
    );
    assert!(
        matches!(result.gate_verdict, GateVerdict::Fail { .. }),
        "gate verdict must be Rejected for a sabotaged strategy"
    );
}
