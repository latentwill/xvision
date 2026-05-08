use xianvec_engine::bundle::slot::LLMSlot;
use xianvec_engine::bundle::risk::{RiskConfig, RiskPreset};
use xianvec_engine::bundle::manifest::{PublicManifest, RegimeFit};
use xianvec_engine::bundle::StrategyBundle;

fn sample_bundle() -> StrategyBundle {
    use xianvec_engine::bundle::manifest::{PublicManifest, RegimeFit};
    use xianvec_engine::bundle::slot::LLMSlot;
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
            role: "regime".into(), prompt: "...".into(),
            model_requirement: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
        }),
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(), prompt: "...".into(),
            model_requirement: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into()],
        }),
        risk: RiskPreset::Balanced.expand(),
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
        creator: "@xianvec_official".to_string(),
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
