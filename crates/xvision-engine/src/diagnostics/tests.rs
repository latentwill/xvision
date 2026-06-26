use crate::agents::{Agent, AgentSlot, InputsPolicy};
use crate::diagnostics::{assert_launchable, diagnose, DiagnosticsError};
use crate::strategies::agent_ref::AgentRef;
use crate::strategies::manifest::PublicManifest;
use crate::strategies::risk::RiskPreset;
use crate::strategies::Strategy;
use chrono::Utc;

fn slot(name: &str, tools: Vec<&str>) -> AgentSlot {
    AgentSlot {
        name: name.to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        system_prompt: "You are a trading assistant. Submit decisions through tools.".repeat(8),
        skill_ids: Vec::new(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        prompt_version: String::new(),
        inputs_policy: InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        noop_skip: None,
        allowed_tools: tools.into_iter().map(str::to_string).collect(),
        delta_briefing: None,
    }
}

fn agent(id: &str, tools: Vec<&str>) -> Agent {
    let now = Utc::now();
    Agent {
        agent_id: id.to_string(),
        name: format!("agent-{id}"),
        description: String::new(),
        tags: Vec::new(),
        slots: vec![slot("trader", tools)],
        archived: false,
        created_at: now,
        updated_at: now,
        scope_strategy_id: None,
    }
}

fn strategy(required_tools: Vec<&str>) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "strat".to_string(),
            display_name: "Strategy".to_string(),
            plain_summary: String::new(),
            creator: "@test".to_string(),
            template: "custom".to_string(),
            regime_fit: Vec::new(),
            asset_universe: vec!["BTC/USD".to_string()],
            required_tools: required_tools.into_iter().map(str::to_string).collect(),
            decision_cadence_minutes: 15,
            timeframe_requirements: Default::default(),
            attested_with: Vec::new(),
            risk_preset_or_config: "balanced".to_string(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        agents: vec![AgentRef {
            agent_id: "agent-1".to_string(),
            role: "trader".to_string(),
            activates: None,
            prompt: String::new(),
            model_override: None,
            checkpoint: None,
            veto: None,
        }],
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        hypothesis: None,
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: true,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

#[test]
fn registered_tools_and_decision_path_are_launchable() {
    let diag = diagnose(
        &strategy(vec![]),
        &[agent("agent-1", vec!["ohlcv", "submit_decision"])],
    );

    assert!(diag.launchable);
    assert!(diag.unregistered_tools.is_empty());
    assert!(diag.has_decision_path);
    assert_eq!(diag.per_agent[0].tools.len(), 2);
    assert!(diag.per_agent[0].tools.iter().all(|tool| tool.registered));
    assert_launchable(&diag).unwrap();
}

#[test]
fn unregistered_tool_blocks_launch() {
    let diag = diagnose(
        &strategy(vec![]),
        &[agent("agent-1", vec!["not_a_tool", "submit_decision"])],
    );

    assert!(!diag.launchable);
    assert_eq!(diag.unregistered_tools.len(), 1);
    assert_eq!(diag.unregistered_tools[0].tool, "not_a_tool");
}

#[test]
fn missing_submit_decision_blocks_launch() {
    let diag = diagnose(&strategy(vec![]), &[agent("agent-1", vec!["ohlcv"])]);

    assert!(!diag.launchable);
    assert!(!diag.has_decision_path);
    assert!(matches!(
        assert_launchable(&diag).unwrap_err(),
        DiagnosticsError::NotLaunchable {
            has_decision_path: false,
            ..
        }
    ));
}

#[test]
fn empty_slot_tools_fall_back_to_strategy_required_tools() {
    let diag = diagnose(
        &strategy(vec!["ohlcv", "submit_decision"]),
        &[agent("agent-1", vec![])],
    );

    assert!(diag.launchable);
    let names: Vec<_> = diag.per_agent[0]
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();
    assert_eq!(names, vec!["ohlcv", "submit_decision"]);
}
