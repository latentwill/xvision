pub mod agent_ref;
pub mod id;
pub mod manifest;
pub mod mechanical;
pub mod risk;
pub mod slot;
pub mod store;
pub mod templates;
pub mod validate;

use serde::{Deserialize, Serialize};

pub use crate::strategies::agent_ref::{AgentRef, PipelineDef, PipelineEdge, PipelineKind};
use crate::strategies::manifest::PublicManifest;
pub use crate::strategies::mechanical::MechanicalParams;
use crate::strategies::risk::RiskConfig;
use crate::strategies::slot::LLMSlot;

// ── Strategy hypothesis (intake #7) ──────────────────────────────────────────
//
// Stored as an optional JSON blob inside the strategy's filesystem JSON file
// (not in SQLite — strategies are file-backed via FilesystemStore, not in
// xvn.db). Design rationale in migration 022 comment.
//
// All fields are `Option<_>` / `Vec<_>` with skip_serializing_if so existing
// strategy JSON files deserialise cleanly (unknown field → ignored; missing
// field → None/empty).

/// Optional hints about risk sizing and flip behaviour for a strategy's
/// hypothesis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct RiskLogicHints {
    /// Rough trade frequency expectation: `"low"` | `"medium"` | `"high"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_trade_frequency: Option<String>,
    /// When `true`, the strategy should avoid flip-flop direction changes
    /// between consecutive decisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_direct_flips: Option<bool>,
}

/// Structured hypothesis annotation for a strategy.
///
/// Every field is optional; strategies without a hypothesis round-trip
/// with `hypothesis: null` in the JSON (or the field is absent entirely).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Hypothesis {
    /// Template / family label, e.g. `"compression-breakout"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Free-form 1-2 sentence hypothesis statement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement: Option<String>,
    /// Market regimes this strategy is expected to perform well in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_regime: Vec<String>,
    /// Market regimes this strategy should avoid or be expected to underperform in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub avoid_regime: Vec<String>,
    /// Assumptions about asset characteristics (e.g. `"high liquidity"`,
    /// `"overnight gaps acceptable"`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asset_assumptions: Vec<String>,
    /// Preferred bar / decision timeframe, e.g. `"4h"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeframe_preferred: Option<String>,
    /// Conditions under which the strategy enters a position.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entry_logic: Vec<String>,
    /// Conditions under which the strategy exits a position.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exit_logic: Vec<String>,
    /// Optional risk-sizing and flip-behaviour hints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_logic: Option<RiskLogicHints>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Strategy {
    pub manifest: PublicManifest,

    // ── Strategy hypothesis (intake #7) ──────────────────────────────────
    /// Optional structured hypothesis for this strategy. Stored in the
    /// strategy's JSON file alongside the rest of the struct.
    /// `None` when the operator has not annotated the strategy with a
    /// hypothesis; serialises as `"hypothesis": null` / absent in JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hypothesis: Option<Hypothesis>,

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
    /// 2. JSON walker over `mechanical_params` — picks the largest
    ///    period-like field × 2.
    /// 3. [`FALLBACK_MIN_WARMUP_BARS`].
    ///
    /// Post-template-registry-removal there is no per-template typed
    /// dispatch; every strategy is treated as operator-authored and
    /// the period-key walker is the single derivation path.
    pub fn min_warmup_bars(&self) -> u32 {
        if let Some(explicit) = self.manifest.min_warmup_bars {
            return explicit;
        }
        let derived = self.typed_params().min_warmup_bars();
        if derived == 0 {
            FALLBACK_MIN_WARMUP_BARS
        } else {
            derived
        }
    }

    /// View of `mechanical_params` wrapped as [`MechanicalParams`].
    /// Always succeeds — the enum has a single `Custom` arm that
    /// preserves arbitrary JSON. Kept as `typed_params()` for
    /// call-site compatibility with the pre-registry-removal API.
    pub fn typed_params(&self) -> MechanicalParams {
        MechanicalParams::Custom(self.mechanical_params.clone())
    }
}

// ── Custom Deserialize ───────────────────────────────────────────────
//
// Before the 2026-05-21 template-registry removal this seam ran
// `MechanicalParams::from_value(template, value)` to surface
// `deny_unknown_fields` violations on canonical templates as
// structured deserialize errors. Post-removal there is no per-template
// dispatch, so the custom impl is a thin pass-through that lets
// `#[derive(Deserialize)]`-equivalent default field handling proceed
// against the same private `StrategyRaw` mirror struct.
//
// The mirror struct is kept (rather than reverting to a plain derive)
// so the `serde(default)` semantics on agents/pipeline/slots stay
// explicit and so adding back per-strategy schema validation in a
// future change has a fixed seam to slot into.

#[derive(Deserialize)]
struct StrategyRaw {
    manifest: PublicManifest,
    #[serde(default)]
    hypothesis: Option<Hypothesis>,
    #[serde(default)]
    agents: Vec<AgentRef>,
    #[serde(default)]
    pipeline: PipelineDef,
    #[serde(default)]
    regime_slot: Option<LLMSlot>,
    #[serde(default)]
    intern_slot: Option<LLMSlot>,
    #[serde(default)]
    trader_slot: Option<LLMSlot>,
    risk: RiskConfig,
    mechanical_params: serde_json::Value,
}

impl<'de> Deserialize<'de> for Strategy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = StrategyRaw::deserialize(deserializer)?;
        Ok(Strategy {
            manifest: raw.manifest,
            hypothesis: raw.hypothesis,
            agents: raw.agents,
            pipeline: raw.pipeline,
            regime_slot: raw.regime_slot,
            intern_slot: raw.intern_slot,
            trader_slot: raw.trader_slot,
            risk: raw.risk,
            mechanical_params: raw.mechanical_params,
        })
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
            hypothesis: None,
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
        let s = strategy_with_params(None, json!({"ema_fast": 12, "ema_mid": 26, "ema_slow": 50}));
        // Max period is 50 -> doubled to 100.
        assert_eq!(s.min_warmup_bars(), 100);
    }

    #[test]
    fn min_warmup_bars_walks_nested_objects_and_arrays() {
        let s = strategy_with_params(
            None,
            json!({
                "outer": {"inner_period": 25},
                "list": [
                    {"fast_window": 3},
                    {"slow_window": 30},
                    {"threshold": 70}
                ],
                "non_int": "ignored",
            }),
        );
        assert_eq!(s.min_warmup_bars(), 60);
    }

    #[test]
    fn min_warmup_bars_ignores_non_period_thresholds() {
        let s = strategy_with_params(
            None,
            json!({
                "rsi_oversold": 30,
                "rsi_overbought": 70,
                "bollinger_period": 20,
                "atr_period": 14
            }),
        );
        assert_eq!(s.min_warmup_bars(), 40);
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
            hypothesis: None,
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
        assert!(!s.contains("\"hypothesis\""), "None hypothesis omitted: {s}");
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

    // ── hypothesis round-trip ───────────────────────────────────────────────

    #[test]
    fn hypothesis_none_omitted_in_serialization() {
        let strategy = Strategy {
            manifest: make_manifest(),
            hypothesis: None,
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: json!({}),
        };
        let s = serde_json::to_string(&strategy).unwrap();
        assert!(
            !s.contains("\"hypothesis\""),
            "None hypothesis must be omitted: {s}"
        );
    }

    #[test]
    fn hypothesis_some_serializes_and_deserializes() {
        let raw = json!({
            "manifest": make_manifest(),
            "hypothesis": {
                "family": "compression-breakout",
                "statement": "BTC consolidates then breaks out.",
                "target_regime": ["post-compression trend"],
                "avoid_regime": ["chop", "late parabolic move"],
                "asset_assumptions": ["high liquidity"],
                "timeframe_preferred": "4h",
                "entry_logic": ["price breaks above compression high"],
                "exit_logic": ["price reverses 2 ATR"],
                "risk_logic": {
                    "max_trade_frequency": "low",
                    "no_direct_flips": true
                }
            },
            "risk": RiskPreset::Balanced.expand(),
            "mechanical_params": {}
        });
        let strategy: Strategy = serde_json::from_value(raw).unwrap();
        let h = strategy.hypothesis.as_ref().expect("hypothesis must be Some");
        assert_eq!(h.family.as_deref(), Some("compression-breakout"));
        assert_eq!(h.statement.as_deref(), Some("BTC consolidates then breaks out."));
        assert_eq!(h.target_regime, vec!["post-compression trend"]);
        assert_eq!(h.avoid_regime, vec!["chop", "late parabolic move"]);
        assert_eq!(h.timeframe_preferred.as_deref(), Some("4h"));
        assert_eq!(h.entry_logic, vec!["price breaks above compression high"]);
        let rl = h.risk_logic.as_ref().expect("risk_logic must be Some");
        assert_eq!(rl.max_trade_frequency.as_deref(), Some("low"));
        assert_eq!(rl.no_direct_flips, Some(true));

        // Round-trip: serialize back and the hypothesis field is present
        let s = serde_json::to_string(&strategy).unwrap();
        assert!(
            s.contains("\"hypothesis\""),
            "populated hypothesis must be serialized: {s}"
        );
        assert!(
            s.contains("compression-breakout"),
            "family must appear in JSON: {s}"
        );
    }

    #[test]
    fn strategy_without_hypothesis_field_deserializes_to_none() {
        // Legacy strategy JSON with no hypothesis key → defaults to None.
        let raw = json!({
            "manifest": make_manifest(),
            "risk": RiskPreset::Balanced.expand(),
            "mechanical_params": {}
        });
        let strategy: Strategy = serde_json::from_value(raw).unwrap();
        assert!(
            strategy.hypothesis.is_none(),
            "missing hypothesis field must deserialise to None"
        );
    }

    #[test]
    fn hypothesis_partial_fields_round_trip() {
        // Only statement + family set; vecs and optionals default to empty/None.
        let raw = json!({
            "manifest": make_manifest(),
            "hypothesis": {
                "family": "mean-reversion",
                "statement": "Asset reverts after extension."
            },
            "risk": RiskPreset::Balanced.expand(),
            "mechanical_params": {}
        });
        let strategy: Strategy = serde_json::from_value(raw).unwrap();
        let h = strategy.hypothesis.as_ref().unwrap();
        assert_eq!(h.family.as_deref(), Some("mean-reversion"));
        assert!(h.target_regime.is_empty());
        assert!(h.risk_logic.is_none());
    }
}
