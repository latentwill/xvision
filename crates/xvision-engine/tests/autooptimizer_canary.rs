use std::sync::Arc;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use serde_json::json;
use xvision_engine::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use xvision_engine::autooptimizer::canary::{build_sabotaged_strategy, run_honesty_check};
use xvision_engine::autooptimizer::config::AutoOptimizerConfig;
use xvision_engine::autooptimizer::eval_adapter::PaperTestRunner;
use xvision_engine::autooptimizer::gate::{GateInput, GateVerdict};
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
        },
        "mechanical_params": {"ema_fast": 12}
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
    }
}

#[test]
fn build_sabotaged_strategy_is_deterministic() {
    let base = make_strategy();
    let a = build_sabotaged_strategy(&base, 42);
    let b = build_sabotaged_strategy(&base, 42);
    assert_eq!(a, b, "same seed must produce identical sabotaged strategy");
}

#[test]
fn sabotaged_strategy_differs_from_base() {
    let base = make_strategy();
    for seed in 0_u64..=2 {
        let sabotaged = build_sabotaged_strategy(&base, seed);
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
