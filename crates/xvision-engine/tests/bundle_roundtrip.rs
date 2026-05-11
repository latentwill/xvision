use xvision_engine::bundle::manifest::{PublicManifest, RegimeFit};
use xvision_engine::bundle::risk::{RiskConfig, RiskPreset};
use xvision_engine::bundle::slot::LLMSlot;
use xvision_engine::bundle::StrategyBundle;

fn sample_bundle() -> StrategyBundle {
    use xvision_engine::bundle::manifest::{PublicManifest, RegimeFit};
    use xvision_engine::bundle::slot::LLMSlot;
    StrategyBundle {
        manifest: PublicManifest {
            id: "01H8N7Z000".to_string(),
            display_name: "Test".to_string(),
            plain_summary: "test bundle".to_string(),
            creator: "@test".to_string(),
            template: "mean_reversion".to_string(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".to_string()],
            decision_cadence_minutes: 15,
            required_models: vec!["anthropic.claude-sonnet-4.6".to_string()],
            required_tools: vec!["ohlcv".to_string()],
            risk_preset_or_config: "balanced".to_string(),
            published_at: None,
        },
        regime_slot: Some(LLMSlot {
            role: "regime".into(),
            prompt: "...".into(),
            model_requirement: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
        }),
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "...".into(),
            model_requirement: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into()],
        }),
        risk: RiskPreset::Balanced.expand(),
        capital: xvision_core::Capital::default(),
        risk_caps: xvision_core::RiskCaps::default(),
        mechanical_params: serde_json::json!({"rsi_oversold": 30, "rsi_overbought": 70}),
    }
}

#[test]
fn slot_serializes_to_json_and_back() {
    let slot = LLMSlot {
        role: "trader".to_string(),
        prompt: "decide: enter long, enter short, or no-op".to_string(),
        model_requirement: "anthropic.claude-sonnet-4.6+".to_string(),
        allowed_tools: vec!["ohlcv".to_string(), "indicator_panel".to_string()],
    };
    let json = serde_json::to_string(&slot).unwrap();
    let parsed: LLMSlot = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.role, "trader");
    assert_eq!(parsed.allowed_tools.len(), 2);
}

#[test]
fn preset_expands_to_explicit_config() {
    let cons = RiskPreset::Conservative.expand();
    assert!(cons.risk_pct_per_trade <= 0.015);
    assert!(cons.max_leverage <= 3.0);
    let bal = RiskPreset::Balanced.expand();
    assert!(bal.risk_pct_per_trade > cons.risk_pct_per_trade);
    let agg = RiskPreset::Aggressive.expand();
    assert!(agg.risk_pct_per_trade > bal.risk_pct_per_trade);
}

#[test]
fn risk_config_roundtrips() {
    let cfg = RiskPreset::Balanced.expand();
    let json = serde_json::to_string(&cfg).unwrap();
    let parsed: RiskConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg, parsed);
}

#[test]
fn manifest_roundtrip_with_required_fields() {
    let m = PublicManifest {
        id: "01H8N7Z123".to_string(),
        display_name: "Buys dips".to_string(),
        plain_summary: "Buys ETH when oversold, sells when recovered.".to_string(),
        creator: "@xvision_official".to_string(),
        template: "mean_reversion".to_string(),
        regime_fit: vec![RegimeFit::RangeBound, RegimeFit::LowVol],
        asset_universe: vec!["ETH/USD".to_string()],
        decision_cadence_minutes: 15,
        required_models: vec!["anthropic.claude-sonnet-4.6+".to_string()],
        required_tools: vec!["ohlcv".to_string(), "indicator_panel".to_string()],
        risk_preset_or_config: "balanced".to_string(),
        published_at: None,
    };
    let json = serde_json::to_string(&m).unwrap();
    let parsed: PublicManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.template, "mean_reversion");
}

#[test]
fn bundle_roundtrip() {
    let b = sample_bundle();
    let json = serde_json::to_string(&b).unwrap();
    let parsed: StrategyBundle = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.manifest.template, "mean_reversion");
    assert!(parsed.regime_slot.is_some());
    assert!(parsed.intern_slot.is_none());
    assert!(parsed.trader_slot.is_some());
}

use xvision_engine::bundle::validate::{validate_bundle, ValidationError};

#[test]
fn valid_bundle_passes() {
    let b = sample_bundle();
    assert!(validate_bundle(&b).is_ok());
}

#[test]
fn bundle_without_any_llm_slot_fails() {
    let mut b = sample_bundle();
    b.regime_slot = None;
    b.intern_slot = None;
    b.trader_slot = None;
    let err = validate_bundle(&b).unwrap_err();
    assert!(matches!(err, ValidationError::NoLlmSlots));
}

#[test]
fn bundle_with_empty_asset_universe_fails() {
    let mut b = sample_bundle();
    b.manifest.asset_universe.clear();
    let err = validate_bundle(&b).unwrap_err();
    assert!(matches!(err, ValidationError::EmptyAssetUniverse));
}

#[test]
fn bundle_with_zero_capital_risk_fails() {
    let mut b = sample_bundle();
    b.risk.risk_pct_per_trade = 0.0;
    let err = validate_bundle(&b).unwrap_err();
    assert!(matches!(err, ValidationError::InvalidRisk(_)));
}

#[test]
fn bundle_without_trader_slot_fails() {
    let mut b = sample_bundle();
    b.trader_slot = None; // regime_slot still Some, so NoLlmSlots wouldn't fire
    let err = validate_bundle(&b).unwrap_err();
    assert!(matches!(err, ValidationError::MissingTraderSlot));
}

#[test]
fn bundle_carries_capital_and_risk_caps() {
    // CS-M2 Task 5: capital + risk caps moved off Scenario, onto bundle.
    let b = sample_bundle();
    assert_eq!(b.capital.initial, 100_000.0);
    assert_eq!(b.capital.currency, "USD");
    assert_eq!(b.risk_caps.max_concurrent_positions, 1);
    assert_eq!(b.risk_caps.max_leverage, 1.0);
}

#[test]
fn bundle_with_missing_capital_still_deserializes() {
    // Old serialized bundles (pre-Task-5) didn't have capital/risk_caps. The
    // #[serde(default)] guard means they still round-trip with the default
    // values populated.
    let pre_task5_json = serde_json::json!({
        "manifest": {
            "id": "01H8OLDB",
            "display_name": "Legacy",
            "plain_summary": "x",
            "creator": "@t",
            "template": "mean_reversion",
            "regime_fit": ["range_bound"],
            "asset_universe": ["BTC/USD"],
            "decision_cadence_minutes": 15,
            "required_models": ["mock"],
            "required_tools": ["ohlcv"],
            "risk_preset_or_config": "balanced",
            "published_at": null
        },
        "trader_slot": {
            "role": "trader",
            "prompt": "decide",
            "model_requirement": "mock",
            "allowed_tools": ["ohlcv"]
        },
        "risk": {
            "risk_pct_per_trade": 0.015,
            "max_concurrent_positions": 2,
            "max_leverage": 3.0,
            "stop_loss_atr_multiple": 2.0,
            "daily_loss_kill_pct": 0.05
        },
        "mechanical_params": {}
    });
    let parsed: StrategyBundle = serde_json::from_value(pre_task5_json).unwrap();
    assert_eq!(parsed.capital.initial, 100_000.0);
    assert_eq!(parsed.risk_caps.max_concurrent_positions, 1);
}

#[test]
fn bundle_with_undeclared_required_tool_fails() {
    let mut b = sample_bundle();
    // Manifest declares a tool no slot has in its allowed_tools.
    b.manifest.required_tools.push("nansen_smartmoney".into());
    let err = validate_bundle(&b).unwrap_err();
    match err {
        ValidationError::UndeclaredTool(name) => assert_eq!(name, "nansen_smartmoney"),
        other => panic!("expected UndeclaredTool, got {other:?}"),
    }
}
