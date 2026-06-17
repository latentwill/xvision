//! Regression test — every builtin AgentTemplate must, when
//! lifted into a Strategy with one `AgentRef` per slot, validate green
//! out of the box.

use xvision_engine::agents::capability::Capability;
use xvision_engine::agents::templates::{builtin_templates, AgentTemplate};
use xvision_engine::strategies::{
    manifest::PublicManifest, risk::RiskPreset, validate::validate_strategy, AgentRef, PipelineDef,
    PipelineKind, Strategy,
};

fn activates_for_slot_name(name: &str) -> Option<Capability> {
    let name = name.to_ascii_lowercase();
    if name.contains("regime") || name.contains("filter") {
        Some(Capability::Filter)
    } else {
        Some(Capability::Trader)
    }
}

fn template_to_strategy(t: &AgentTemplate) -> Strategy {
    let agents: Vec<AgentRef> = t
        .slots
        .iter()
        .enumerate()
        .map(|(idx, slot)| AgentRef {
            agent_id: format!("agent-{}-{}", t.id, idx),
            role: slot.name.clone(),
            activates: activates_for_slot_name(&slot.name),
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        })
        .collect();

    let kind = if agents.len() <= 1 {
        PipelineKind::Single
    } else {
        PipelineKind::Sequential
    };

    Strategy {
        manifest: PublicManifest {
            id: format!("01HZTEMPLATE-{}", t.id),
            display_name: t.name.clone(),
            plain_summary: t.description.clone(),
            creator: "@test".into(),
            template: t.id.clone(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents,
        pipeline: PipelineDef { kind, edges: vec![] },
        regime_slot: None,
        trader_slot: None,
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
fn validate_succeeds_for_every_builtin_template() {
    let templates = builtin_templates();
    assert!(
        !templates.is_empty(),
        "builtin_templates() must return at least one starter template"
    );

    for t in &templates {
        let strategy = template_to_strategy(t);

        for (i, agent) in strategy.agents.iter().enumerate() {
            assert!(
                agent.activates.is_some(),
                "template `{}` slot {} ({}): AgentRef.activates is None",
                t.id,
                i,
                agent.role,
            );
        }

        // Acceptance: the strategy validates green.
        match validate_strategy(&strategy) {
            Ok(()) => {}
            Err(e) => panic!(
                "template `{}` failed validation after Phase E retrofit: {:?}\n\
                 agents: {:?}",
                t.id, e, strategy.agents
            ),
        }
    }
}

#[test]
fn at_least_one_template_activates_trader_capability() {
    let templates = builtin_templates();
    let activates_trader_count: usize = templates
        .iter()
        .map(|t| {
            template_to_strategy(t)
                .agents
                .iter()
                .filter(|a| a.activates == Some(Capability::Trader))
                .count()
        })
        .sum();

    assert!(
        activates_trader_count > 0,
        "no AgentRef in any builtin template activates Capability::Trader — \
         the Phase E retrofit must keep at least the Trader-headed templates intact"
    );
}

#[test]
fn single_trader_template_is_trader_only() {
    let template = builtin_templates()
        .into_iter()
        .find(|t| t.id == "single-trader")
        .expect("builtin_templates must include `single-trader`");

    assert_eq!(
        template.slots.len(),
        1,
        "`single-trader` template should ship with exactly one slot (the 80% case); \
         got {} slots",
        template.slots.len(),
    );

    let only_slot = &template.slots[0];
    assert!(
        only_slot
            .allowed_tools
            .iter()
            .any(|tool| tool == "submit_decision"),
        "`single-trader` template's only slot must grant submit_decision; got allowed_tools = {:?}",
        only_slot.allowed_tools,
    );

    let strategy = template_to_strategy(&template);
    assert_eq!(strategy.agents.len(), 1);
    assert_eq!(
        strategy.agents[0].activates,
        Some(Capability::Trader),
        "single-trader's AgentRef must activate Capability::Trader",
    );

    validate_strategy(&strategy).expect("single-trader template must validate green");
}

#[test]
fn indicator_panel_tool_is_present_in_the_template_set() {
    let templates = builtin_templates();

    let has_indicator_panel = templates.iter().any(|t| {
        t.slots
            .iter()
            .any(|slot| slot.allowed_tools.iter().any(|tool| tool == "indicator_panel"))
    });

    assert!(has_indicator_panel, "no template slot grants indicator_panel");
}
