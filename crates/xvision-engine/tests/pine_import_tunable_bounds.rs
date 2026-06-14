//! WU-A integration tests: TunableBound persistence on Strategy.
//!
//! TDD: these tests are written FIRST and will fail until WU-A is implemented.

use xvision_engine::strategies::pine_import::{import_pine, InputKind};
use xvision_engine::strategies::{ActivationMode, Strategy, TunableBound};

// ── Fixture helpers ────────────────────────────────────────────────────────────

fn wu3_fixture() -> &'static str {
    include_str!("fixtures/pine/wu3_inputs_bound.pine")
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[test]
fn import_pine_wu3_fixture_yields_three_tunable_bounds() {
    let outcome = import_pine(wu3_fixture()).expect("wu3 fixture must import cleanly");
    let bounds = &outcome.strategy.tunable_bounds;

    assert_eq!(
        bounds.len(),
        3,
        "expected 3 TunableBound entries (rsi_len, stop_pct, use_filter); got: {bounds:?}"
    );

    // rsi_len: Int, min=2, max=50, no step
    let rsi = bounds
        .iter()
        .find(|b| b.kind == InputKind::Int)
        .expect("must have an Int bound");
    assert!(
        !rsi.path.is_empty(),
        "rsi_len path must not be empty; got: {:?}",
        rsi.path
    );
    assert_eq!(rsi.min, Some(2.0), "rsi_len min must be 2.0");
    assert_eq!(rsi.max, Some(50.0), "rsi_len max must be 50.0");
    assert!(rsi.step.is_none(), "rsi_len step must be None (not declared)");

    // stop_pct: Float, min=0.5, max=10.0, step=0.1
    let stop = bounds
        .iter()
        .find(|b| b.kind == InputKind::Float)
        .expect("must have a Float bound");
    assert!(
        !stop.path.is_empty(),
        "stop_pct path must not be empty; got: {:?}",
        stop.path
    );
    assert_eq!(stop.min, Some(0.5), "stop_pct min must be 0.5");
    assert_eq!(stop.max, Some(10.0), "stop_pct max must be 10.0");
    assert_eq!(stop.step, Some(0.1), "stop_pct step must be 0.1");

    // use_filter: Bool, no bounds
    let bool_b = bounds
        .iter()
        .find(|b| b.kind == InputKind::Bool)
        .expect("must have a Bool bound");
    assert!(
        !bool_b.path.is_empty(),
        "use_filter path must not be empty; got: {:?}",
        bool_b.path
    );
    assert!(bool_b.min.is_none(), "use_filter min must be None");
    assert!(bool_b.max.is_none(), "use_filter max must be None");
    assert!(bool_b.step.is_none(), "use_filter step must be None");
}

#[test]
fn non_pine_strategy_omits_tunable_bounds_key() {
    // A Strategy with empty tunable_bounds must NOT serialise the key at all
    // (serde skip_serializing_if = Vec::is_empty).
    use serde_json::json;
    use xvision_engine::strategies::{
        agent_ref::PipelineDef, manifest::PublicManifest, mechanistic::DecisionMode, risk::RiskPreset,
    };

    let manifest_val = json!({
        "id": "01HZTEST000000000000BOUNDS",
        "display_name": "No Bounds",
        "plain_summary": "",
        "creator": "@test",
        "template": "custom",
        "regime_fit": [],
        "asset_universe": [],
        "decision_cadence_minutes": 60,
        "required_tools": [],
        "risk_preset_or_config": "balanced"
    });
    let manifest: PublicManifest = serde_json::from_value(manifest_val).unwrap();

    let strategy = Strategy {
        manifest,
        hypothesis: None,
        agents: Vec::new(),
        pipeline: PipelineDef::default(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: DecisionMode::Agentic,
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    };

    let serialized = serde_json::to_string(&strategy).unwrap();
    assert!(
        !serialized.contains("\"tunable_bounds\""),
        "empty tunable_bounds must be omitted from serialization; got: {serialized}"
    );
}

#[test]
fn strategy_round_trips_with_tunable_bounds() {
    use serde_json::json;
    use xvision_engine::strategies::{
        agent_ref::PipelineDef, manifest::PublicManifest, mechanistic::DecisionMode, risk::RiskPreset,
    };

    let manifest_val = json!({
        "id": "01HZTEST000000000000RTBND",
        "display_name": "Round-trip bounds",
        "plain_summary": "",
        "creator": "@test",
        "template": "custom",
        "regime_fit": [],
        "asset_universe": [],
        "decision_cadence_minutes": 60,
        "required_tools": [],
        "risk_preset_or_config": "balanced"
    });
    let manifest: PublicManifest = serde_json::from_value(manifest_val).unwrap();

    let bounds = vec![
        TunableBound {
            path: "mechanistic.close_policies.0.pct".to_string(),
            min: Some(0.5),
            max: Some(10.0),
            step: Some(0.1),
            kind: InputKind::Float,
        },
        TunableBound {
            path: "unbound.rsi_len".to_string(),
            min: Some(2.0),
            max: Some(50.0),
            step: None,
            kind: InputKind::Int,
        },
    ];

    let strategy = Strategy {
        manifest,
        hypothesis: None,
        agents: Vec::new(),
        pipeline: PipelineDef::default(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: DecisionMode::Agentic,
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: bounds.clone(),
    };

    let json_str = serde_json::to_string(&strategy).unwrap();
    assert!(
        json_str.contains("\"tunable_bounds\""),
        "non-empty tunable_bounds must be serialized; got: {json_str}"
    );

    let deserialized: Strategy = serde_json::from_str(&json_str).unwrap();
    assert_eq!(
        deserialized.tunable_bounds, bounds,
        "tunable_bounds must survive round-trip"
    );
}

#[test]
fn from_markdown_preserves_tunable_bounds() {
    use serde_json::json;
    use xvision_engine::autooptimizer::program_view::{from_markdown, to_markdown};
    use xvision_engine::strategies::{
        agent_ref::PipelineDef, manifest::PublicManifest, mechanistic::DecisionMode, risk::RiskPreset,
    };

    let manifest_val = json!({
        "id": "01HZTEST000000000000FMBND",
        "display_name": "from_markdown bounds",
        "plain_summary": "",
        "creator": "@test",
        "template": "custom",
        "regime_fit": [],
        "asset_universe": [],
        "decision_cadence_minutes": 60,
        "required_tools": [],
        "risk_preset_or_config": "balanced"
    });
    let manifest: PublicManifest = serde_json::from_value(manifest_val).unwrap();

    let bounds = vec![
        TunableBound {
            path: "mechanistic.close_policies.0.pct".to_string(),
            min: Some(1.0),
            max: Some(5.0),
            step: Some(0.5),
            kind: InputKind::Float,
        },
        TunableBound {
            path: "unbound.use_filter".to_string(),
            min: None,
            max: None,
            step: None,
            kind: InputKind::Bool,
        },
    ];

    let strategy = Strategy {
        manifest,
        hypothesis: None,
        agents: Vec::new(),
        pipeline: PipelineDef::default(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: DecisionMode::Agentic,
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: bounds.clone(),
    };

    let md = to_markdown(&strategy);
    let recovered = from_markdown(&md, &strategy).expect("from_markdown must succeed");

    assert_eq!(
        recovered.tunable_bounds, bounds,
        "from_markdown must preserve tunable_bounds from base strategy"
    );
}
