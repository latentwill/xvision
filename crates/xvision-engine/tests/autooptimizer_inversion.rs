use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::json;

use xvision_engine::autooptimizer::eval_adapter::PaperTestRunner;
use xvision_engine::autooptimizer::inversion::{invert_mutation, run_inversion_pair};
use xvision_engine::autooptimizer::mutator::{MutationDiff, MutationKind, ParamChange, ProseEdit, ToolDiff};
use xvision_engine::eval::scenario_seed::canonical_seed_rows;
use xvision_engine::eval::{MetricsSummary, Scenario};
use xvision_engine::strategies::Strategy;

// ── fixtures ──────────────────────────────────────────────────────────────────

fn fixture_strategy() -> Strategy {
    serde_json::from_value(json!({
        "manifest": {
            "id": "01HINV000001",
            "display_name": "Inversion Test",
            "plain_summary": "inversion fixture",
            "creator": "@test",
            "template": "custom",
            "regime_fit": [],
            "asset_universe": ["BTC/USD"],
            "decision_cadence_minutes": 60,
            "required_tools": ["price_feed"],
            "risk_preset_or_config": "balanced"
        },
        "agents": [{"agent_id": "01HINVAGENT1", "role": "trader"}],
        "risk": {
            "risk_pct_per_trade": 0.015,
            "max_concurrent_positions": 2,
            "max_leverage": 3.0,
            "stop_loss_atr_multiple": 2.0,
            "daily_loss_kill_pct": 0.05
        }
    }))
    .expect("fixture strategy deserializes")
}

fn fixture_diff() -> MutationDiff {
    MutationDiff {
        kind: MutationKind::Param,
        prose: vec![ProseEdit {
            agent_role: "trader".into(),
            before: "analyze carefully".into(),
            after: "analyze aggressively".into(),
        }],
        params: vec![ParamChange {
            key: "rsi_period".into(),
            before: json!(14),
            after: json!(21),
        }],
        tools: ToolDiff {
            added: vec!["volume_profile".into()],
            removed: vec!["atr".into()],
        },
        filter: Vec::new(),
        create_filter: None,
        rationale: "increase aggression".into(),
    }
}

fn metrics(sharpe: f64) -> MetricsSummary {
    MetricsSummary {
        sharpe,
        ..MetricsSummary::default()
    }
}

fn fixture_scenarios() -> (Scenario, Scenario) {
    let rows = canonical_seed_rows();
    assert!(rows.len() >= 2, "need at least 2 canonical scenarios");
    (rows[0].clone(), rows[1].clone())
}

// ── stub PaperTestRunner ──────────────────────────────────────────────────────

struct OrderedStub(Mutex<VecDeque<MetricsSummary>>);

impl OrderedStub {
    fn new(items: Vec<MetricsSummary>) -> Self {
        Self(Mutex::new(items.into_iter().collect()))
    }
}

#[async_trait]
impl PaperTestRunner for OrderedStub {
    async fn run(&self, _strategy: &Strategy, _scenario: &Scenario) -> anyhow::Result<MetricsSummary> {
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("OrderedStub exhausted"))
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn invert_mutation_round_trips() {
    let d = fixture_diff();
    let double = invert_mutation(&invert_mutation(&d));

    assert_eq!(double.prose[0].before, d.prose[0].before);
    assert_eq!(double.prose[0].after, d.prose[0].after);
    assert_eq!(double.prose[0].agent_role, d.prose[0].agent_role);

    assert_eq!(double.params[0].key, d.params[0].key);
    assert_eq!(double.params[0].before, d.params[0].before);
    assert_eq!(double.params[0].after, d.params[0].after);

    assert_eq!(double.tools.added, d.tools.added);
    assert_eq!(double.tools.removed, d.tools.removed);

    assert_eq!(double.rationale, d.rationale);
}

#[tokio::test]
async fn run_inversion_pair_symmetric_noise_true() {
    let (day, baseline) = fixture_scenarios();
    let stub = OrderedStub::new(vec![
        metrics(0.90), // forward_day
        metrics(0.88), // forward_untouched
        metrics(0.91), // reverse_day  — delta = 0.01 < 0.05
        metrics(0.89), // reverse_untouched
    ]);
    let result = run_inversion_pair(&fixture_strategy(), &fixture_diff(), &stub, &day, &baseline)
        .await
        .expect("run_inversion_pair must not fail");

    assert!(
        result.symmetric_noise,
        "near-equal Sharpe must flag symmetric_noise"
    );
}

#[tokio::test]
async fn run_inversion_pair_asymmetric_signal_false() {
    let (day, baseline) = fixture_scenarios();
    let stub = OrderedStub::new(vec![
        metrics(1.50), // forward_day
        metrics(1.45), // forward_untouched
        metrics(0.30), // reverse_day  — delta = 1.20 >> 0.05
        metrics(0.35), // reverse_untouched
    ]);
    let result = run_inversion_pair(&fixture_strategy(), &fixture_diff(), &stub, &day, &baseline)
        .await
        .expect("run_inversion_pair must not fail");

    assert!(
        !result.symmetric_noise,
        "large Sharpe gap must not flag symmetric_noise"
    );
}
