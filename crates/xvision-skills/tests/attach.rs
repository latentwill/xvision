use xvision_engine::bundle::manifest::{PublicManifest, RegimeFit};
use xvision_engine::bundle::risk::RiskPreset;
use xvision_engine::bundle::slot::LLMSlot;
use xvision_engine::bundle::StrategyBundle;
use xvision_skills::attach::attach_skill_to_agent;
use xvision_skills::Skill;

fn dummy_skill() -> Skill {
    Skill {
        name: "test".into(),
        display_name: "T".into(),
        description: "x".into(),
        version: "1.0.0".into(),
        allowed_tools: vec!["indicator_panel".into()],
        model_requirement: "anthropic.claude-sonnet-4.6".into(),
        body: "NEW PROMPT".into(),
        content_hash: "deadbeef".into(),
    }
}

fn dummy_bundle() -> StrategyBundle {
    StrategyBundle {
        manifest: PublicManifest {
            id: "01".into(),
            display_name: "T".into(),
            plain_summary: "x".into(),
            creator: "@t".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 15,
            required_models: vec!["anthropic.claude-sonnet-4.6".into()],
            required_tools: vec!["ohlcv".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
        },
        regime_slot: None,
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "OLD".into(),
            model_requirement: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into()],
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    }
}

#[test]
fn attaches_to_trader_replaces_prompt_unions_tools() {
    let mut bundle = dummy_bundle();
    attach_skill_to_agent(&mut bundle, "trader", &dummy_skill()).unwrap();
    let trader = bundle.trader_slot.unwrap();
    assert_eq!(trader.prompt, "NEW PROMPT");
    assert_eq!(trader.model_requirement, "anthropic.claude-sonnet-4.6");
    assert!(trader.allowed_tools.contains(&"ohlcv".into()));
    assert!(trader.allowed_tools.contains(&"indicator_panel".into()));
    // Tool union must not duplicate.
    assert_eq!(trader.allowed_tools.len(), 2);
}

#[test]
fn attaching_to_empty_slot_fails() {
    let mut bundle = dummy_bundle();
    let err = attach_skill_to_agent(&mut bundle, "regime", &dummy_skill()).unwrap_err();
    assert!(err.to_string().contains("regime"), "msg: {err}");
}

#[test]
fn unknown_slot_role_fails() {
    let mut bundle = dummy_bundle();
    let err = attach_skill_to_agent(&mut bundle, "bogus", &dummy_skill()).unwrap_err();
    assert!(err.to_string().contains("bogus"), "msg: {err}");
}
