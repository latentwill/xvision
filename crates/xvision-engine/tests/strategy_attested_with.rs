use serde_json::json;

use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::validate::validate_strategy;
use xvision_engine::strategies::{PipelineDef, Strategy};

fn manifest(attested: Vec<String>) -> PublicManifest {
    PublicManifest {
        id: "01HZSTRAT".into(),
        display_name: "Attested-with regression".into(),
        plain_summary: "verifies attested_with is informational".into(),
        creator: "@test".into(),
        template: "custom".into(),
        regime_fit: vec![],
        asset_universe: vec!["ETH/USD".into()],
        decision_cadence_minutes: 60,
        attested_with: attested,
        required_tools: vec![],
        risk_preset_or_config: "balanced".into(),
        published_at: None,
        min_warmup_bars: None,
        color: None,
        execution_mode: Default::default(),
        capital_mode: Default::default(),
        timeframe_requirements: Default::default(),
    }
}

fn trader_slot(attested: &str, bound_provider: Option<&str>, bound_model: Option<&str>) -> LLMSlot {
    LLMSlot {
        role: "trader".into(),
        attested_with: attested.into(),
        allowed_tools: vec![],
        provider: bound_provider.map(|s| s.into()),
        model: bound_model.map(|s| s.into()),
    }
}

fn strategy_with(
    manifest_attested: Vec<String>,
    slot_attested: &str,
    bound: Option<(&str, &str)>,
) -> Strategy {
    let (provider, model) = match bound {
        Some((p, m)) => (Some(p), Some(m)),
        None => (None, None),
    };
    Strategy {
        manifest: manifest(manifest_attested),
        hypothesis: None,
        agents: Vec::new(),
        pipeline: PipelineDef::default(),
        regime_slot: None,
        trader_slot: Some(trader_slot(slot_attested, provider, model)),
        risk: RiskPreset::Balanced.expand(),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

#[test]
fn bind_mismatch_passes_validation_because_attested_with_is_informational() {
    let strategy = strategy_with(
        vec!["claude-sonnet-4.6".into()],
        "anthropic.claude-sonnet-4.6",
        Some(("openrouter", "qwen-72b-instruct")),
    );
    let result = validate_strategy(&strategy);
    assert!(
        result.is_ok(),
        "attested_with must not gate validation when the binding differs; got {result:?}",
    );
    let slot = strategy.trader_slot.as_ref().expect("trader slot");
    assert_eq!(slot.model.as_deref(), Some("qwen-72b-instruct"));
    assert_eq!(slot.attested_with, "anthropic.claude-sonnet-4.6");
}

#[test]
fn attested_with_round_trips_serialize_deserialize() {
    let original = strategy_with(
        vec!["claude-sonnet-4.6".into(), "deepseek-v4-flash".into()],
        "anthropic.claude-sonnet-4.6",
        Some(("anthropic", "claude-sonnet-4.6")),
    );

    let wire = serde_json::to_string(&original).expect("serialize");
    let parsed: Strategy = serde_json::from_str(&wire).expect("deserialize");

    assert_eq!(
        parsed.manifest.attested_with,
        vec!["claude-sonnet-4.6".to_string(), "deepseek-v4-flash".to_string()],
    );
    let slot = parsed.trader_slot.as_ref().expect("trader slot survives");
    assert_eq!(slot.attested_with, "anthropic.claude-sonnet-4.6");
}

#[test]
fn legacy_required_models_field_name_fails_to_deserialize() {
    // Pre-launch breaking change: there is no backwards-compat alias for
    // the old manifest field name `required_models`. Strategies authored
    // before the rename must be re-published.
    let raw = json!({
        "id": "01HZSTRAT",
        "display_name": "legacy",
        "plain_summary": "",
        "creator": "@t",
        "template": "custom",
        "regime_fit": [],
        "asset_universe": ["ETH/USD"],
        "decision_cadence_minutes": 60,
        "required_models": ["claude-sonnet-4.6"],
        "required_tools": [],
        "risk_preset_or_config": "balanced",
        "published_at": null
    });
    let parsed: Result<PublicManifest, _> = serde_json::from_value(raw);
    assert!(
        parsed.is_err() || parsed.as_ref().unwrap().attested_with.is_empty(),
        "legacy `required_models` must not populate `attested_with` (no alias shim); got {parsed:?}",
    );
}

#[test]
fn legacy_model_requirement_field_name_fails_on_deny_unknown_fields() {
    // `LLMSlot` declares `#[serde(deny_unknown_fields)]`, so the pre-rename
    // field name must be rejected outright.
    let raw = json!({
        "role": "trader",
        "attested_with": "anthropic.claude-sonnet-4.6",
        "model_requirement": "anthropic.claude-sonnet-4.6",
        "allowed_tools": []
    });
    let parsed: Result<LLMSlot, _> = serde_json::from_value(raw);
    let err = parsed.expect_err("legacy `model_requirement` field must be rejected");
    assert!(
        err.to_string().contains("model_requirement"),
        "error should name `model_requirement`, got: {err}",
    );
}
