//! Regression tests for the `qa-role-normalization` track
//! (`team/contracts/qa-role-normalization.md`).
//!
//! These bundle QA findings #5, #6, #7 from
//! `qa/2026-05-17-comprehensive-codebase-review.md`:
//!
//! - #5: Attached `Trader` role passes eval validation but is dropped
//!   from pipeline outputs because the schema-selection comparison was
//!   case-insensitive while the output-assignment match was
//!   case-sensitive.
//! - #6: Whitespace-padded role values can persist; the canonical
//!   comparison key now trims so this doesn't drift across sites.
//! - #7: Reasoning-class truncation hint missed whitespace-padded
//!   trader roles. Covered by unit tests inside the executor modules.
//!
//! After this track, every comparison site reads roles through
//! `canonical_role(&str)` (trim + ASCII lowercase) so the bugs above
//! cannot recur.

use std::sync::Arc;
use xvision_engine::agent::llm::MockDispatch;
use xvision_engine::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use xvision_engine::strategies::agent_ref::canonical_role;
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::validate::{validate_strategy, ValidationError};
use xvision_engine::strategies::{AgentRef, PipelineDef, PipelineEdge, PipelineKind, Strategy};
use xvision_engine::tools::ToolRegistry;

fn fixture_strategy_with_agents(agents: Vec<AgentRef>, pipeline: PipelineDef) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: "01H8N7ZROLE".into(),
            display_name: "RoleNormTest".into(),
            plain_summary: "x".into(),
            creator: "@t".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 15,
            attested_with: vec!["mock".into()],
            required_tools: vec!["ohlcv".into()],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents,
        pipeline,
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

fn resolved_agent_slot(role: &str) -> ResolvedAgentSlot {
    ResolvedAgentSlot {
        role: role.into(),
        slot: LLMSlot {
            role: role.into(),
            attested_with: "mock".into(),
            allowed_tools: Vec::new(),
            provider: None,
            model: Some("mock".into()),
        },
        system_prompt: String::new(),
        max_tokens: None,
        max_wall_ms: None,
        temperature: None,
        inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
        bar_history_limit: None,
        memory_mode: xvision_memory::types::MemoryMode::Off,
        agent_id: String::new(),
        noop_skip: true,
    }
}

/// QA #5 — attached `Trader` / `TRADER` / ` trader ` must populate
/// `PipelineOutputs.trader`, not silently drop the result.
#[tokio::test]
async fn pipeline_output_assigned_for_role_variants() {
    for variant in [" trader ", "Trader", "TRADER", "trader"] {
        let strategy = fixture_strategy_with_agents(
            vec![AgentRef {
                agent_id: "01HZAGENT".into(),
                role: variant.into(),
                activates: None,
                prompt_override: None,
                model_override: None,
            }],
            PipelineDef {
                kind: PipelineKind::Single,
                edges: Vec::new(),
            },
        );
        let slots = vec![resolved_agent_slot(variant)];
        let dispatch = Arc::new(MockDispatch::echo(r#"{"action":"long_open","reasoning":"r"}"#));
        let tools = Arc::new(ToolRegistry::default_with_builtins());
        let outs = run_pipeline(PipelineInputs {
            strategy: &strategy,
            agent_slots: &slots,
            seed_inputs: serde_json::json!({}),
            dispatch,
            tools,
            obs: None,
            memory_recorder: None,

            scenario_start: None,

            source_window_start: None,

            source_window_end: None,

            run_id: String::new(),

            scenario_id: String::new(),

            cycle_idx: 0,
            provider_catalogs: std::collections::HashMap::new(),
            filter_ctx: None,
            trace_attrs: None,
            recorder: None,
            runtime: Default::default(),
            cline: None,
            model_call_span_id: None,
        })
        .await
        .expect("pipeline runs");
        assert!(
            outs.trader.is_some(),
            "role variant `{variant}` should populate PipelineOutputs.trader",
        );
    }
}

/// QA #6 — graph-edge role lookups use the canonical comparison key, so
/// validation accepts edges that reference role names in different
/// cases / whitespace as long as one of the attached agents canonicalizes
/// to the same value.
#[test]
fn graph_edge_validation_uses_canonical_form() {
    // Agents persisted with case/whitespace variants; serde already
    // normalizes on deserialize, but constructing programmatically
    // here exercises the validate.rs canonicalization path directly.
    let strategy = fixture_strategy_with_agents(
        vec![
            AgentRef {
                agent_id: "01HZSCOUT".into(),
                role: "Scout".into(),
                activates: None,
                prompt_override: None,
                model_override: None,
            },
            AgentRef {
                agent_id: "01HZTRADER".into(),
                role: " trader ".into(),
                activates: None,
                prompt_override: None,
                model_override: None,
            },
        ],
        PipelineDef {
            kind: PipelineKind::Graph,
            edges: vec![PipelineEdge {
                from_role: " SCOUT ".into(),
                to_role: " trader ".into(),
                condition: None,
            }],
        },
    );

    // validate_strategy must accept: canonical form of each agent.role
    // and each edge endpoint resolves to the same key.
    validate_strategy(&strategy).expect("canonical-form lookups should accept variants");
}

/// QA #6 follow-on — whitespace-only role is still rejected after
/// canonicalization, because `canonical_role` of an all-whitespace
/// string is empty.
#[test]
fn whitespace_only_role_is_rejected() {
    let strategy = fixture_strategy_with_agents(
        vec![AgentRef {
            agent_id: "01HZBLANK".into(),
            role: "   ".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
        }],
        PipelineDef {
            kind: PipelineKind::Single,
            edges: Vec::new(),
        },
    );
    match validate_strategy(&strategy) {
        Err(ValidationError::EmptyAgentRole) => {}
        other => panic!("expected EmptyAgentRole, got {other:?}"),
    }
}

/// QA #6 — duplicate-role detection runs on canonical form, so
/// `Trader` + ` trader ` count as the same role.
#[test]
fn duplicate_role_detected_across_variants() {
    let strategy = fixture_strategy_with_agents(
        vec![
            AgentRef {
                agent_id: "01HZA".into(),
                role: "Trader".into(),
                activates: None,
                prompt_override: None,
                model_override: None,
            },
            AgentRef {
                agent_id: "01HZB".into(),
                role: " TRADER ".into(),
                activates: None,
                prompt_override: None,
                model_override: None,
            },
        ],
        PipelineDef {
            kind: PipelineKind::Sequential,
            edges: Vec::new(),
        },
    );
    match validate_strategy(&strategy) {
        Err(ValidationError::DuplicateAgentRole(role)) => {
            assert_eq!(role, "trader", "duplicate detection must report canonical key");
        }
        other => panic!("expected DuplicateAgentRole, got {other:?}"),
    }
}

/// Serde round-trip — a whitespace-padded role on disk normalizes on
/// load, so the engine never sees a non-canonical persisted form.
#[test]
fn agent_ref_round_trip_normalizes_role() {
    let json = serde_json::json!({
        "agent_id": "01HZAGENT",
        "role": " Trader ",
    });
    let r: AgentRef = serde_json::from_value(json).unwrap();
    assert_eq!(r.role, "trader");
    let back = serde_json::to_string(&r).unwrap();
    assert!(back.contains("\"trader\""));
    assert!(!back.contains(" Trader"));
}

/// Public helper smoke-test — `canonical_role` is the single source of
/// truth; any new comparison site should import it rather than rolling
/// its own trim/case-fold.
#[test]
fn canonical_role_is_trim_lowercase() {
    assert_eq!(canonical_role(" Trader "), "trader");
    assert_eq!(canonical_role("trader"), "trader");
    assert_eq!(canonical_role(""), "");
    assert_eq!(canonical_role("   "), "");
}
