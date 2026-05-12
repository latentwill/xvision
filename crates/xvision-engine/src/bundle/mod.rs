pub mod agent_ref;
pub mod manifest;
pub mod risk;
pub mod slot;
pub mod store;
pub mod validate;

use serde::{Deserialize, Serialize};

pub use crate::bundle::agent_ref::{AgentRef, PipelineDef, PipelineEdge, PipelineKind};
use crate::bundle::manifest::PublicManifest;
use crate::bundle::risk::RiskConfig;
use crate::bundle::slot::LLMSlot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrategyBundle {
    pub manifest: PublicManifest,

    // ── New: agent composition (refactor T1) ──────────────────────────
    /// Agent references composing this strategy's pipeline. Empty for
    /// bundles authored before the agent-composition refactor — those
    /// still carry the legacy slot fields below. New bundles populate
    /// `agents` and leave the slot fields `None`. The migration step
    /// (a separate task) lifts slots into Agent records and populates
    /// `agents` accordingly.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<AgentRef>,

    /// Wiring spec for the agents above. Defaults to `Single` for
    /// pre-refactor bundles (which had at most three slots executed in
    /// a fixed order — equivalent to Sequential, but the migration is
    /// what populates `agents`; pre-migration bundles just have an
    /// empty `agents` Vec, so Single is the safe parse default).
    #[serde(default, skip_serializing_if = "is_default_pipeline")]
    pub pipeline: PipelineDef,

    // ── Legacy: fixed slot fields (deprecated, kept for back-compat) ──
    /// DEPRECATED post-refactor: use `agents` + an Agent record. Read
    /// path keeps this populated for bundles authored before the
    /// migration; the engine prefers `agents` when both are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regime_slot: Option<LLMSlot>,

    /// DEPRECATED — see `regime_slot`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intern_slot: Option<LLMSlot>,

    /// DEPRECATED — see `regime_slot`. Pre-refactor: at least one slot
    /// must be filled; trader was required. Post-refactor: presence in
    /// `agents` replaces this constraint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trader_slot: Option<LLMSlot>,

    pub risk: RiskConfig,

    /// Template-specific mechanical params (e.g., rsi thresholds, EMA periods).
    pub mechanical_params: serde_json::Value,
}

fn is_default_pipeline(p: &PipelineDef) -> bool {
    p.kind == PipelineKind::Single && p.edges.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::manifest::PublicManifest;
    use crate::bundle::risk::{RiskConfig, RiskPreset};
    use serde_json::json;

    fn make_manifest() -> PublicManifest {
        PublicManifest {
            id: "01HZBUNDLE".into(),
            display_name: "Test".into(),
            plain_summary: "test".into(),
            creator: "@test".into(),
            template: "ma_crossover".into(),
            regime_fit: vec![],
            asset_universe: vec![],
            decision_cadence_minutes: 60,
            required_models: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
        }
    }

    #[test]
    fn legacy_bundle_json_parses_with_empty_agents() {
        // Bundle authored before the refactor: has regime/intern/trader_slot
        // fields and no `agents`/`pipeline`. Must still parse — serde(default)
        // gives empty agents and Single pipeline.
        let raw = json!({
            "manifest": make_manifest(),
            "trader_slot": {
                "role": "trader",
                "prompt": "you are a trader",
                "model_requirement": "anthropic.claude-sonnet-4.6+",
                "allowed_tools": []
            },
            "risk": RiskPreset::Balanced.expand(),
            "mechanical_params": {}
        });
        let bundle: StrategyBundle = serde_json::from_value(raw).unwrap();
        assert!(bundle.agents.is_empty(), "agents defaults to empty");
        assert_eq!(
            bundle.pipeline.kind,
            PipelineKind::Single,
            "pipeline defaults to Single",
        );
        assert!(bundle.trader_slot.is_some(), "legacy slot survives the parse");
    }

    #[test]
    fn new_bundle_json_parses_with_agents() {
        // Bundle authored post-refactor: has `agents`/`pipeline` and no
        // legacy slot fields.
        let raw = json!({
            "manifest": make_manifest(),
            "agents": [
                { "agent_id": "01HZAGENT1", "role": "trader" }
            ],
            "pipeline": { "kind": "single" },
            "risk": RiskPreset::Balanced.expand(),
            "mechanical_params": {}
        });
        let bundle: StrategyBundle = serde_json::from_value(raw).unwrap();
        assert_eq!(bundle.agents.len(), 1);
        assert_eq!(bundle.agents[0].agent_id, "01HZAGENT1");
        assert_eq!(bundle.agents[0].role, "trader");
        assert_eq!(bundle.pipeline.kind, PipelineKind::Single);
        assert!(bundle.regime_slot.is_none());
        assert!(bundle.trader_slot.is_none());
    }

    #[test]
    fn mixed_bundle_json_keeps_both() {
        // During the migration window a bundle may have BOTH `agents`
        // and legacy slots (the new agents derived from the slots).
        // The serde shape must round-trip without dropping either.
        let raw = json!({
            "manifest": make_manifest(),
            "agents": [
                { "agent_id": "01HZAGENT1", "role": "trader" }
            ],
            "pipeline": { "kind": "single" },
            "trader_slot": {
                "role": "trader",
                "prompt": "you are a trader",
                "model_requirement": "anthropic.claude-sonnet-4.6+",
                "allowed_tools": []
            },
            "risk": RiskPreset::Balanced.expand(),
            "mechanical_params": {}
        });
        let bundle: StrategyBundle = serde_json::from_value(raw).unwrap();
        assert_eq!(bundle.agents.len(), 1);
        assert!(bundle.trader_slot.is_some());
    }

    #[test]
    fn empty_agents_and_default_pipeline_round_trip_compactly() {
        // For pre-migration bundles, the new fields stay out of the
        // wire shape so existing JSON stays diff-clean.
        let bundle = StrategyBundle {
            manifest: make_manifest(),
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: json!({}),
        };
        let s = serde_json::to_string(&bundle).unwrap();
        assert!(!s.contains("\"agents\""), "empty agents omitted: {s}");
        assert!(!s.contains("\"pipeline\""), "default pipeline omitted: {s}");
        // But populated agents/pipeline DO surface.
        let bundle = StrategyBundle {
            agents: vec![AgentRef {
                agent_id: "x".into(),
                role: "main".into(),
            }],
            pipeline: PipelineDef::sequential(),
            ..bundle
        };
        let s = serde_json::to_string(&bundle).unwrap();
        assert!(s.contains("\"agents\""), "populated agents serialized");
        assert!(s.contains("\"pipeline\""), "non-default pipeline serialized");
    }
}
