//! Acceptance tests for the `strategy-slot-prompt-resolution` contract.
//!
//! The legacy `LLMSlot.prompt` field was removed on 2026-05-22 — the
//! agent-side `AgentSlot.system_prompt` is now the single source of
//! truth for the trader's system prompt. These tests pin the four
//! invariants the contract demands:
//!
//! 1. `LLMSlot` deserializes WITHOUT a `prompt` field.
//! 2. Old JSON carrying `"prompt": "..."` fails to deserialize (the
//!    struct uses `deny_unknown_fields` — no `serde(alias)` shim).
//! 3. Strategy validation succeeds end-to-end without a slot prompt.
//! 4. `xvn_validate_draft` smoke: a strategy whose bound agent has a
//!    populated `system_prompt` passes validation.

use serde_json::json;
use xvision_engine::authoring;
use xvision_engine::strategies::manifest::{PublicManifest, RegimeFit};
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::slot::LLMSlot;
use xvision_engine::strategies::validate::validate_strategy;
use xvision_engine::strategies::{
    store::{FilesystemStore, StrategyStore},
    AgentRef, PipelineDef, Strategy,
};

#[test]
fn llmslot_deserializes_without_prompt_field() {
    let raw = json!({
        "role": "trader",
        "attested_with": "anthropic.claude-sonnet-4.6",
        "allowed_tools": []
    });
    let slot: LLMSlot = serde_json::from_value(raw).expect("slot without prompt parses");
    assert_eq!(slot.role, "trader");
    assert_eq!(slot.attested_with, "anthropic.claude-sonnet-4.6");
    assert!(slot.allowed_tools.is_empty());
}

#[test]
fn llmslot_rejects_legacy_prompt_field() {
    // Pre-launch breaking change: strategies persisted with the legacy
    // `"prompt"` field fail to deserialize because `LLMSlot` uses
    // `deny_unknown_fields`. No `#[serde(alias = "prompt")]` shim was
    // added per the repo's no-shims rule — operators must re-save the
    // strategy without the field.
    let raw = json!({
        "role": "trader",
        "prompt": "you are a trader",
        "attested_with": "anthropic.claude-sonnet-4.6",
        "allowed_tools": []
    });
    let err = serde_json::from_value::<LLMSlot>(raw).expect_err("legacy prompt must be rejected");
    assert!(
        err.to_string().contains("unknown field") && err.to_string().contains("prompt"),
        "expected `unknown field \"prompt\"` error, got: {err}",
    );
}

#[test]
fn strategy_validates_with_agent_ref_and_no_slot_prompt() {
    // Post-2026-05-22 strategies carry an `AgentRef`; the agent-side
    // `system_prompt` is the real source of truth. The Strategy itself
    // has no slot prompt to inspect — validation must succeed without
    // one.
    let strategy = Strategy {
        manifest: PublicManifest {
            id: "01HZNOPROMPT".into(),
            display_name: "No-slot-prompt".into(),
            plain_summary: "test".into(),
            creator: "@t".into(),
            template: "custom".into(),
            regime_fit: vec![RegimeFit::TrendingBull],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec!["anthropic.claude-sonnet-4.6".into()],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        },
        hypothesis: None,
        agents: vec![AgentRef {
            agent_id: "01HZAGENT".into(),
            role: "trader".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
        }],
        pipeline: PipelineDef::default(),
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
    };
    validate_strategy(&strategy).expect("strategy with AgentRef and no slot prompt validates");
}

#[tokio::test]
async fn validate_draft_passes_with_bound_agent_ref() {
    // xvn_validate_draft smoke: build a draft via the authoring layer,
    // attach an AgentRef (the bound agent's `system_prompt` is the
    // single source of truth for the trader's prompt), and confirm the
    // resulting draft validates. The Strategy itself carries no slot
    // prompt — the prompt-from-slot codepath is gone.
    let strategy_dir = tempfile::tempdir().expect("tempdir");
    let store = FilesystemStore::new(strategy_dir.path().to_path_buf());

    let created = authoring::create_strategy(
        &store,
        authoring::CreateStrategyReq {
            name: "slot-prompt-smoke".into(),
            creator: Some("@test".into()),
        },
    )
    .await
    .expect("create_strategy");

    let mut strategy = store.load(&created.id).await.expect("load draft");
    strategy.agents.push(AgentRef {
        agent_id: "01HZAGENTSMOKE".into(),
        role: "trader".into(),
        activates: None,
        prompt_override: None,
        model_override: None,
    });
    store.save(&strategy).await.expect("save strategy");

    let strategy = store.load(&created.id).await.expect("reload");
    validate_strategy(&strategy).expect("draft with bound agent ref validates");
}
