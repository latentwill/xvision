use std::sync::Arc;
use xvision_engine::agent::llm::{ContentBlock, LlmResponse, MockDispatch, StopReason};
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, PipelineOutputs, ResolvedAgentSlot};
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::{PipelineDef, Strategy};
use xvision_engine::tools::ToolRegistry;

fn fixture_strategy() -> Strategy {
    Strategy {
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

            min_warmup_bars: None,
        },
        agents: Vec::new(),
        pipeline: PipelineDef::default(),
        regime_slot: Some(LLMSlot {
            role: "regime".into(),
            prompt: "classify regime".into(),
            model_requirement: "mock".into(),
            allowed_tools: vec!["ohlcv".into()],
            provider: None,
            model: None,
        }),
        intern_slot: Some(LLMSlot {
            role: "intern".into(),
            prompt: "build briefing".into(),
            model_requirement: "mock".into(),
            allowed_tools: vec!["ohlcv".into()],
            provider: None,
            model: None,
        }),
        trader_slot: Some(LLMSlot {
            role: "trader".into(),
            prompt: "decide".into(),
            model_requirement: "mock".into(),
            allowed_tools: vec!["ohlcv".into()],
            provider: None,
            model: None,
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    }
}

fn text_response(text: &str) -> LlmResponse {
    LlmResponse {
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 1,
        output_tokens: 1,
    }
}

#[tokio::test]
async fn three_slot_pipeline_chains_outputs() {
    let strategy = fixture_strategy();
    let dispatch = Arc::new(MockDispatch::echo(r#"{"ok":true}"#));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let outs: PipelineOutputs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &[],
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
    let mut strategy = fixture_strategy();
    strategy.regime_slot = None; // skip
    let dispatch = Arc::new(MockDispatch::echo(r#"{"ok":true}"#));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &[],
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

#[tokio::test]
async fn resolved_agent_pipeline_uses_trader_role_as_decision_output() {
    let mut strategy = fixture_strategy();
    strategy.regime_slot = None;
    strategy.intern_slot = None;
    strategy.trader_slot = None;
    strategy.pipeline = PipelineDef::sequential();
    let agent_slots = vec![
        ResolvedAgentSlot {
            role: "scout".into(),
            slot: LLMSlot {
                role: "scout".into(),
                prompt: "scan".into(),
                model_requirement: "mock".into(),
                allowed_tools: vec!["ohlcv".into()],
                provider: None,
                model: None,
            },
        },
        ResolvedAgentSlot {
            role: "trader".into(),
            slot: LLMSlot {
                role: "trader".into(),
                prompt: "decide".into(),
                model_requirement: "mock".into(),
                allowed_tools: vec!["ohlcv".into()],
                provider: None,
                model: None,
            },
        },
    ];

    let dispatch = Arc::new(MockDispatch::echo(r#"{"action":"hold"}"#));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &agent_slots,
        seed_inputs: serde_json::json!({}),
        dispatch,
        tools,
    })
    .await
    .unwrap();
    assert!(outs.regime.is_none());
    assert!(outs.intern.is_none());
    assert!(outs.trader.is_some());
    assert!(outs.total_input_tokens > 0);
}

#[tokio::test]
async fn resolved_agent_pipeline_does_not_treat_non_trader_as_decision_output() {
    let mut strategy = fixture_strategy();
    strategy.regime_slot = None;
    strategy.intern_slot = None;
    strategy.trader_slot = None;
    strategy.pipeline = PipelineDef::sequential();
    let agent_slots = vec![
        ResolvedAgentSlot {
            role: "scout".into(),
            slot: LLMSlot {
                role: "scout".into(),
                prompt: "scan".into(),
                model_requirement: "mock".into(),
                allowed_tools: vec!["ohlcv".into()],
                provider: None,
                model: None,
            },
        },
        ResolvedAgentSlot {
            role: "final_decider".into(),
            slot: LLMSlot {
                role: "final_decider".into(),
                prompt: "decide".into(),
                model_requirement: "mock".into(),
                allowed_tools: vec!["ohlcv".into()],
                provider: None,
                model: None,
            },
        },
    ];

    let dispatch = Arc::new(MockDispatch::sequence(vec![
        text_response(r#"{"stage":"first"}"#),
        text_response(r#"{"stage":"second"}"#),
    ]));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let outs = run_pipeline(PipelineInputs {
        strategy: &strategy,
        agent_slots: &agent_slots,
        seed_inputs: serde_json::json!({}),
        dispatch,
        tools,
    })
    .await
    .unwrap();

    assert!(outs.trader.is_none());
    assert_eq!(outs.total_input_tokens, 2);
    assert_eq!(outs.total_output_tokens, 2);
}
