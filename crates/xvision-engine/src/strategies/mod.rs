pub mod agent_ref;
pub mod manifest;
pub mod risk;
pub mod slot;
pub mod store;
pub mod validate;

use serde::{Deserialize, Serialize};

pub use crate::strategies::agent_ref::{AgentRef, PipelineDef, PipelineEdge, PipelineKind};
use crate::strategies::manifest::PublicManifest;
use crate::strategies::risk::RiskConfig;
use crate::strategies::slot::LLMSlot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Strategy {
    pub manifest: PublicManifest,

    // ── New: agent composition (refactor T1) ──────────────────────────
    /// Agent references composing this strategy's pipeline. Empty for
    /// strategies authored before the agent-composition refactor — those
    /// still carry the legacy slot fields below. New strategies populate
    /// `agents` and leave the slot fields `None`. The migration step
    /// (a separate task) lifts slots into Agent records and populates
    /// `agents` accordingly.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<AgentRef>,

    /// Wiring spec for the agents above. Defaults to `Single` for
    /// pre-refactor strategies (which had at most three slots executed in
    /// a fixed order — equivalent to Sequential, but the migration is
    /// what populates `agents`; pre-migration strategies just have an
    /// empty `agents` Vec, so Single is the safe parse default).
    #[serde(default, skip_serializing_if = "is_default_pipeline")]
    pub pipeline: PipelineDef,

    // ── Legacy: fixed slot fields (deprecated, kept for back-compat) ──
    /// DEPRECATED post-refactor: use `agents` + an Agent record. Read
    /// path keeps this populated for strategies authored before the
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

/// Fallback warmup-bar count when neither `manifest.min_warmup_bars` nor
/// any indicator period can be derived from `mechanical_params`.
pub const FALLBACK_MIN_WARMUP_BARS: u32 = 0;

impl Strategy {
    /// Minimum prior-bar context this strategy needs at decision t=0.
    ///
    /// Resolution order:
    /// 1. `manifest.min_warmup_bars`, if set.
    /// 2. The largest positive integer in `mechanical_params` (scanned
    ///    recursively), times 2. Covers `ema_slow=50` → 100,
    ///    `donchian_period=20` → 40, etc.
    /// 3. [`FALLBACK_MIN_WARMUP_BARS`].
    pub fn min_warmup_bars(&self) -> u32 {
        if let Some(explicit) = self.manifest.min_warmup_bars {
            return explicit;
        }
        match max_indicator_period(&self.mechanical_params) {
            Some(p) => p.saturating_mul(2),
            None => FALLBACK_MIN_WARMUP_BARS,
        }
    }
}

/// Recursively walk a `serde_json::Value` and return the largest positive
/// integer found. Used as a heuristic to derive a strategy's
/// `min_warmup_bars` from the indicator periods baked into
/// `mechanical_params` (`ema_fast`, `ema_slow`, `donchian_period`, etc.).
fn max_indicator_period(value: &serde_json::Value) -> Option<u32> {
    use serde_json::Value;
    match value {
        Value::Number(n) => {
            let as_u64 = n.as_u64().or_else(|| n.as_i64().filter(|x| *x > 0).map(|x| x as u64));
            as_u64.and_then(|n| u32::try_from(n).ok()).filter(|n| *n > 0)
        }
        Value::Array(arr) => arr.iter().filter_map(max_indicator_period).max(),
        Value::Object(map) => map.values().filter_map(max_indicator_period).max(),
        Value::Null | Value::Bool(_) | Value::String(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategies::manifest::PublicManifest;
    #[allow(unused_imports)]
    use crate::strategies::risk::{RiskConfig, RiskPreset};
    use serde_json::json;

    // ── q15-scenario-warmup-bars — min_warmup_bars derivation ──────────

    fn strategy_with_params(min_explicit: Option<u32>, params: serde_json::Value) -> Strategy {
        Strategy {
            manifest: PublicManifest {
                min_warmup_bars: min_explicit,
                ..make_manifest()
            },
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: params,
        }
    }

    #[test]
    fn min_warmup_bars_prefers_explicit_manifest_value() {
        let s = strategy_with_params(Some(42), json!({"ema_slow": 50}));
        // Explicit wins over the derived max-period heuristic.
        assert_eq!(s.min_warmup_bars(), 42);
    }

    #[test]
    fn min_warmup_bars_derives_from_max_indicator_period_when_unset() {
        let s = strategy_with_params(
            None,
            json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50}),
        );
        // Max period is 50 -> doubled to 100.
        assert_eq!(s.min_warmup_bars(), 100);
    }

    #[test]
    fn min_warmup_bars_walks_nested_objects_and_arrays() {
        let s = strategy_with_params(
            None,
            json!({
                "outer": {"inner_period": 25},
                "list": [3, 5, 30, 7],
                "non_int": "ignored",
            }),
        );
        assert_eq!(s.min_warmup_bars(), 60);
    }

    #[test]
    fn min_warmup_bars_falls_back_when_no_periods_present() {
        let s = strategy_with_params(None, json!({}));
        assert_eq!(s.min_warmup_bars(), FALLBACK_MIN_WARMUP_BARS);
    }

    fn make_manifest() -> PublicManifest {
        PublicManifest {
            id: "01HZSTRATEGY".into(),
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
            min_warmup_bars: None,
        }
    }

    #[test]
    fn legacy_strategy_json_parses_with_empty_agents() {
        // Strategy authored before the refactor: has regime/intern/trader_slot
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
        let strategy: Strategy = serde_json::from_value(raw).unwrap();
        assert!(strategy.agents.is_empty(), "agents defaults to empty");
        assert_eq!(
            strategy.pipeline.kind,
            PipelineKind::Single,
            "pipeline defaults to Single",
        );
        assert!(strategy.trader_slot.is_some(), "legacy slot survives the parse");
    }

    #[test]
    fn new_strategy_json_parses_with_agents() {
        // Strategy authored post-refactor: has `agents`/`pipeline` and no
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
        let strategy: Strategy = serde_json::from_value(raw).unwrap();
        assert_eq!(strategy.agents.len(), 1);
        assert_eq!(strategy.agents[0].agent_id, "01HZAGENT1");
        assert_eq!(strategy.agents[0].role, "trader");
        assert_eq!(strategy.pipeline.kind, PipelineKind::Single);
        assert!(strategy.regime_slot.is_none());
        assert!(strategy.trader_slot.is_none());
    }

    #[test]
    fn mixed_strategy_json_keeps_both() {
        // During the migration window a strategy may have BOTH `agents`
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
        let strategy: Strategy = serde_json::from_value(raw).unwrap();
        assert_eq!(strategy.agents.len(), 1);
        assert!(strategy.trader_slot.is_some());
    }

    #[test]
    fn empty_agents_and_default_pipeline_round_trip_compactly() {
        // For pre-migration strategies, the new fields stay out of the
        // wire shape so existing JSON stays diff-clean.
        let strategy = Strategy {
            manifest: make_manifest(),
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: json!({}),
        };
        let s = serde_json::to_string(&strategy).unwrap();
        assert!(!s.contains("\"agents\""), "empty agents omitted: {s}");
        assert!(!s.contains("\"pipeline\""), "default pipeline omitted: {s}");
        // But populated agents/pipeline DO surface.
        let strategy = Strategy {
            agents: vec![AgentRef {
                agent_id: "x".into(),
                role: "main".into(),
            }],
            pipeline: PipelineDef::sequential(),
            ..strategy
        };
        let s = serde_json::to_string(&strategy).unwrap();
        assert!(s.contains("\"agents\""), "populated agents serialized");
        assert!(s.contains("\"pipeline\""), "non-default pipeline serialized");
    }
}
