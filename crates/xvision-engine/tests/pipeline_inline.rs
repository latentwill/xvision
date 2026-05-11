use std::sync::Arc;
use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, PipelineOutputs};
use xvision_engine::bundle::manifest::{PublicManifest, RegimeFit};
use xvision_engine::bundle::risk::RiskPreset;
use xvision_engine::bundle::slot::LLMSlot;
use xvision_engine::bundle::StrategyBundle;
use xvision_engine::tools::ToolRegistry;

fn fixture_bundle() -> StrategyBundle {
    StrategyBundle {
        manifest: PublicManifest {
            id: "01H8N7ZPIPE".into(),
            display_name: "Pipe Test".into(),
            plain_summary: "x".into(),
            creator: "@t".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 15,
            required_models: vec!["mock".into()],
            required_tools: vec!["ohlcv".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
        },
        regime_slot: Some(LLMSlot {
            role: "regime".into(),
            prompt: "classify regime".into(),
            model_requirement: "mock".into(),
            allowed_tools: vec!["ohlcv".into()],
        }),
        intern_slot: Some(LLMSlot {
            role: "intern".into(),
            prompt: "build briefing".into(),
            model_requirement: "mock".into(),
            allowed_tools: vec!["ohlcv".into()],
        }),
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "decide".into(),
            model_requirement: "mock".into(),
            allowed_tools: vec!["ohlcv".into()],
        }),
        risk: RiskPreset::Balanced.expand(),
        capital: xvision_core::Capital::default(),
        risk_caps: xvision_core::RiskCaps::default(),
        mechanical_params: serde_json::json!({}),
    }
}

#[tokio::test]
async fn three_slot_pipeline_chains_outputs() {
    let bundle = fixture_bundle();
    let dispatch = Arc::new(MockDispatch::echo(r#"{"ok":true}"#));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let outs: PipelineOutputs = run_pipeline(PipelineInputs {
        bundle: &bundle,
        seed_inputs: serde_json::json!({"ohlcv_history": [], "indicator_panel": {}}),
        dispatch,
        tools,
    })
    .await
    .unwrap();
    assert!(outs.regime.is_some());
    assert!(outs.intern.is_some());
    assert!(outs.trader.is_some());
    assert!(outs.total_input_tokens > 0);
    assert!(outs.total_output_tokens > 0);
}

#[tokio::test]
async fn skips_missing_optional_slots() {
    let mut bundle = fixture_bundle();
    bundle.regime_slot = None; // skip
    let dispatch = Arc::new(MockDispatch::echo(r#"{"ok":true}"#));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let outs = run_pipeline(PipelineInputs {
        bundle: &bundle,
        seed_inputs: serde_json::json!({}),
        dispatch,
        tools,
    })
    .await
    .unwrap();
    assert!(outs.regime.is_none());
    assert!(outs.intern.is_some());
    assert!(outs.trader.is_some());
}
