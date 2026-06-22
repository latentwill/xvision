use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::{RiskConfig, RiskPreset};
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::Strategy;

fn sample_strategy() -> Strategy {
    use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
    use xvision_engine::strategies::slot::LLMSlot;
    Strategy {
        manifest: PublicManifest {
            id: "01H8N7Z000".to_string(),
            display_name: "Test".to_string(),
            plain_summary: "test strategy".to_string(),
            creator: "@test".to_string(),
            template: "mean_reversion".to_string(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".to_string()],
            decision_cadence_minutes: 15,
            timeframe_requirements: Default::default(),
            attested_with: vec!["anthropic.claude-sonnet-4.6".to_string()],
            required_tools: vec!["ohlcv".to_string()],
            risk_preset_or_config: "balanced".to_string(),
            published_at: None,

            min_warmup_bars: None,

            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: Default::default(),
        regime_slot: Some(LLMSlot {
            role: "regime".into(),
            attested_with: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
            provider: None,
            model: None,
        }),
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            attested_with: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into()],
            provider: None,
            model: None,
        }),
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
fn slot_serializes_to_json_and_back() {
    let slot = LLMSlot {
        role: "trader".to_string(),
        attested_with: "anthropic.claude-sonnet-4.6+".to_string(),
        allowed_tools: vec!["ohlcv".to_string(), "indicator_panel".to_string()],
        provider: None,
        model: None,
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
        timeframe_requirements: Default::default(),
        attested_with: vec!["anthropic.claude-sonnet-4.6+".to_string()],
        required_tools: vec!["ohlcv".to_string(), "indicator_panel".to_string()],
        risk_preset_or_config: "balanced".to_string(),
        published_at: None,
        min_warmup_bars: None,
        color: None,
        execution_mode: Default::default(),
        capital_mode: Default::default(),
    };
    let json = serde_json::to_string(&m).unwrap();
    let parsed: PublicManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.template, "mean_reversion");
}

#[test]
fn strategy_manifest_with_auxiliary_timeframes_roundtrips() {
    let mut strategy = sample_strategy();
    strategy.manifest.timeframe_requirements.auxiliary = vec![
        xvision_engine::strategies::manifest::TimeframeSpec("4h".into()),
        xvision_engine::strategies::manifest::TimeframeSpec("1d".into()),
    ];
    let json = serde_json::to_string(&strategy).unwrap();
    let parsed: Strategy = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed.manifest.timeframe_requirements.auxiliary,
        strategy.manifest.timeframe_requirements.auxiliary
    );
}

#[test]
fn strategy_roundtrip() {
    let strategy = sample_strategy();
    let json = serde_json::to_string(&strategy).unwrap();
    let parsed: Strategy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.manifest.template, "mean_reversion");
    assert!(parsed.regime_slot.is_some());
    assert!(parsed.trader_slot.is_some());
}

use xvision_engine::strategies::validate::{validate_strategy, ValidationError};

#[test]
fn valid_strategy_passes() {
    let strategy = sample_strategy();
    assert!(validate_strategy(&strategy).is_ok());
}

#[test]
fn strategy_without_any_llm_slot_fails() {
    let mut strategy = sample_strategy();
    strategy.regime_slot = None;
    strategy.trader_slot = None;
    let err = validate_strategy(&strategy).unwrap_err();
    assert!(matches!(err, ValidationError::NoAgents));
}

#[test]
fn strategy_with_empty_asset_universe_fails() {
    let mut strategy = sample_strategy();
    strategy.manifest.asset_universe.clear();
    let err = validate_strategy(&strategy).unwrap_err();
    assert!(matches!(err, ValidationError::EmptyAssetUniverse));
}

#[test]
fn strategy_with_zero_capital_risk_fails() {
    let mut strategy = sample_strategy();
    strategy.risk.risk_pct_per_trade = 0.0;
    let err = validate_strategy(&strategy).unwrap_err();
    assert!(matches!(err, ValidationError::InvalidRisk(_)));
}

#[test]
fn strategy_without_trader_slot_fails() {
    let mut strategy = sample_strategy();
    strategy.trader_slot = None; // regime_slot still Some, so NoAgents wouldn't fire
    let err = validate_strategy(&strategy).unwrap_err();
    assert!(matches!(err, ValidationError::MissingTraderSlot));
}

#[test]
fn strategy_does_not_carry_capital_or_risk_caps() {
    // Capital moved back onto Scenario (not Strategy/strategy). The strategy
    // only carries per-trade RiskConfig. Verify the struct round-trips
    // cleanly without capital/risk_caps fields.
    let strategy = sample_strategy();
    let json = serde_json::to_string(&strategy).unwrap();
    assert!(
        !json.contains("\"capital\""),
        "capital must not appear in Strategy JSON"
    );
    assert!(
        !json.contains("\"risk_caps\""),
        "risk_caps must not appear in Strategy JSON"
    );
}

#[test]
fn strategy_with_extra_capital_field_in_json_still_deserializes() {
    // Old serialized strategies (pre-merge) may have capital/risk_caps in JSON.
    // Strategy ignores unknown fields by default — they silently drop.
    let pre_merge_json = serde_json::json!({
        "manifest": {
            "id": "01H8OLDB",
            "display_name": "Legacy",
            "plain_summary": "x",
            "creator": "@t",
            "template": "mean_reversion",
            "regime_fit": ["range_bound"],
            "asset_universe": ["BTC/USD"],
            "decision_cadence_minutes": 15,
            "attested_with": ["mock"],
            "required_tools": ["ohlcv"],
            "risk_preset_or_config": "balanced",
            "published_at": null
        },
        "trader_slot": {
            "role": "trader",
            "attested_with": "mock",
            "allowed_tools": ["ohlcv"]
        },
        "risk": {
            "risk_pct_per_trade": 0.015,
            "max_concurrent_positions": 2,
            "max_leverage": 3.0,
            "stop_loss_atr_multiple": 2.0,
            "daily_loss_kill_pct": 0.05
        },
        "capital": { "initial": 100000.0, "currency": "USD" },
        "risk_caps": { "max_concurrent_positions": 1, "max_leverage": 1.0, "daily_loss_kill_switch_pct": 0.05 }
    });
    // Should parse without error; extra fields are ignored.
    let parsed: Strategy = serde_json::from_value(pre_merge_json).unwrap();
    assert_eq!(parsed.manifest.id, "01H8OLDB");
    assert!(parsed.trader_slot.is_some());
}

#[test]
fn strategy_with_undeclared_required_tool_fails() {
    let mut b = sample_strategy();
    // Manifest declares a tool no slot has in its allowed_tools.
    b.manifest.required_tools.push("nansen_smartmoney".into());
    let err = validate_strategy(&b).unwrap_err();
    match err {
        ValidationError::UndeclaredTool(name) => assert_eq!(name, "nansen_smartmoney"),
        other => panic!("expected UndeclaredTool, got {other:?}"),
    }
}
