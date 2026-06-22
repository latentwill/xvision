pub mod agent_ref;
pub mod exec_mode;
pub mod id;
pub mod manifest;
pub mod mechanistic;
pub mod pine_import;
pub mod risk;
pub mod slot;
pub mod store;
pub mod templates;
pub mod validate;

// ── WU2: BriefingIndicator ────────────────────────────────────────────────────

/// A single indicator extracted from a Pine Script that cannot be reduced to a
/// deterministic `Filter` `Condition` (e.g., because it appears in a fuzzy
/// predicate, a `var`-counter expression, or a ternary condition).
///
/// These indicators are included in the decision seed as context for an Agentic
/// strategy so the LLM trader has their latest values available at decision time.
/// Populated by the WU2 mapper (`pine_import::map_script`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefingIndicator {
    /// The indicator kind from the xvision-filters catalog.
    pub name: xvision_filters::IndicatorName,
    /// Parameters: for period-based indicators `params[0]` is the period.
    /// For SuperTrend the packed `atr_period * 1000 + mult_times_10` is in
    /// `params[0]`. For PivotHigh/PivotLow the packed `left * 1000 + right`
    /// is in `params[0]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<f64>,
    /// The Pine Script variable name that held this indicator's value
    /// (e.g. `"atr_val"`, `"ema_fast"`). Used for seed key naming and
    /// WU4 fidelity reporting.
    pub source_token: String,
}

/// A single optimizer search-space bound derived from a Pine Script `input.*`
/// declaration. Persisted on `Strategy` so the optimizer and settings UI can
/// enforce/display the author-declared parameter ranges.
///
/// Populated by `pine_import::import_pine`; empty for non-Pine strategies.
/// Uses `InputKind` from `pine_import::inputs` (re-imported here to avoid
/// dep inversion — `TunableBound` lives in `strategies`, not in `pine_import`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TunableBound {
    /// Stable optimizer mutation path (same address space as
    /// `mechanistic_tunable_paths` / `filter_tunable_paths`).
    pub path: String,
    /// Minimum allowed value. `None` when not declared in the Pine `input.*` call.
    pub min: Option<f64>,
    /// Maximum allowed value. `None` when not declared.
    pub max: Option<f64>,
    /// Step size. `None` for `Bool` and when no explicit step was declared.
    pub step: Option<f64>,
    /// The Pine type this input was declared as.
    pub kind: crate::strategies::pine_import::inputs::InputKind,
}

use serde::{Deserialize, Serialize};
pub use xvision_filters::{ActivationMode, Filter};

pub use crate::strategies::agent_ref::{AgentRef, PipelineDef, PipelineEdge, PipelineKind};
pub use crate::strategies::exec_mode::{CapitalMode, ExecutionMode};
use crate::strategies::manifest::PublicManifest;
pub use crate::strategies::mechanistic::{
    ClosePolicy, DecisionMode, EntryDirection, EntryRule, ExitReason, MechanisticConfig,
};
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

    /// DEPRECATED — see `regime_slot`. Pre-refactor: at least one slot
    /// must be filled; trader was required. Post-refactor: presence in
    /// `agents` replaces this constraint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trader_slot: Option<LLMSlot>,

    pub risk: RiskConfig,

    // ── Filter v1 (track-plan-touches) ───────────────────────────────────
    /// When the strategy's pipeline should be invoked per bar.
    /// `EveryBar` (default) keeps the pre-filter-v1 behavior: every bar
    /// runs the pipeline. `FilterGated` requires `filter.is_some()` and
    /// only runs the pipeline on bars where the filter's runtime returns
    /// an `Active` decision. `CompiledRules` is reserved for v1.5 and
    /// rejected by [`Strategy::validate_filter`].
    #[serde(default = "default_activation_mode")]
    pub activation_mode: ActivationMode,

    /// Inline filter spec when `activation_mode == FilterGated`. Embedded
    /// in the strategy JSON for v1 (no separate file or DB row — Stage 4
    /// adds CRUD). Must be `None` when `activation_mode == EveryBar`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Filter>,

    /// Suppresses the no-Filter soft-warning that `validate_strategy`
    /// emits when a Trader agent has no upstream Filter wired
    /// into the pipeline. Operators who deliberately want every-bar
    /// dispatch (e.g. a long-horizon trader where Filter would be over-
    /// optimization) set this to `true` to acknowledge the cost.
    ///
    /// Default `false`; absent from disk for default-false strategies so
    /// pre-firing-filter-CLI JSON files round-trip byte-stable. Operators
    /// flip it via `xvn strategy create --no-filter-warning` /
    /// `xvn strategy edit --no-filter-warning`. See contract
    /// `team/contracts/agent-firing-filter-cli-verbs.md` Phase 2.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub acknowledge_no_filter: bool,

    /// Whether this strategy uses an LLM agent (default) or deterministic
    /// mechanistic rules for trade decisions. Absent from disk for agentic
    /// strategies so pre-existing JSON files round-trip byte-stable.
    #[serde(default, skip_serializing_if = "DecisionMode::is_agentic")]
    pub decision_mode: DecisionMode,

    /// Rule-based entry/exit configuration. Required when
    /// `decision_mode == Mechanistic`, must be `None` for `Agentic`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mechanistic_config: Option<MechanisticConfig>,

    // ── WU2: Pine Script import — briefing indicators ─────────────────────
    /// Indicators harvested from an imported Pine Script that could not be
    /// reduced to a deterministic `Filter` `Condition` (fuzzy predicates,
    /// `var`-counter expressions, ternaries). Non-empty only for Agentic
    /// strategies produced by `pine_import::map_script`.
    ///
    /// The decision seed builder injects their latest values into the seed
    /// JSON under `"briefing_indicators"` so the LLM trader agent sees them
    /// without needing to compute them from raw OHLCV bars.
    ///
    /// Skipped on serialization when empty so pre-WU2 strategy JSON files
    /// remain byte-stable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub briefing_indicators: Vec<BriefingIndicator>,

    // ── WU-A: Pine Script import — tunable bounds ─────────────────────────
    /// Per-input optimizer search-space bounds derived from a Pine Script
    /// `input.*` declaration. Populated by `pine_import::import_pine`;
    /// empty for non-Pine strategies. The optimizer enforces these bounds
    /// on proposed mutations (WU-B); the settings UI renders them (WU-C).
    ///
    /// Skipped on serialization when empty so pre-WU-A strategy JSON files
    /// remain byte-stable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tunable_bounds: Vec<TunableBound>,
}

fn default_activation_mode() -> ActivationMode {
    ActivationMode::EveryBar
}

fn is_default_pipeline(p: &PipelineDef) -> bool {
    p.kind == PipelineKind::Single && p.edges.is_empty()
}

/// Fallback warmup-bar count when `manifest.min_warmup_bars` is unset.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimeframeSupport {
    Native,
    Auxiliary,
}
pub const FALLBACK_MIN_WARMUP_BARS: u32 = 0;

impl Strategy {
    /// Minimum prior-bar context this strategy needs at decision t=0.
    ///
    /// Resolution order:
    /// 1. `manifest.min_warmup_bars`, if set.
    /// 2. [`FALLBACK_MIN_WARMUP_BARS`].
    ///

    pub fn native_timeframe(&self) -> crate::strategies::manifest::TimeframeSpec {
        crate::strategies::manifest::TimeframeSpec(match self.manifest.decision_cadence_minutes {
            1 => "1m",
            5 => "5m",
            15 => "15m",
            30 => "30m",
            60 => "1h",
            120 => "2h",
            240 => "4h",
            1440 => "1d",
            other => return crate::strategies::manifest::TimeframeSpec(format!("{other}m")),
        }
        .to_string())
    }

    pub fn supported_timeframes(
        &self,
    ) -> Vec<(crate::strategies::manifest::TimeframeSpec, TimeframeSupport)> {
        let mut out = Vec::with_capacity(1 + self.manifest.timeframe_requirements.auxiliary.len());
        out.push((self.native_timeframe(), TimeframeSupport::Native));
        out.extend(
            self.manifest
                .timeframe_requirements
                .auxiliary
                .iter()
                .cloned()
                .map(|tf| (tf, TimeframeSupport::Auxiliary)),
        );
        out
    }
    /// Warmup is purely a manifest concern — set `manifest.min_warmup_bars`
    /// explicitly when a strategy's indicators need prior-bar history.
    pub fn min_warmup_bars(&self) -> u32 {
        self.manifest.min_warmup_bars.unwrap_or(FALLBACK_MIN_WARMUP_BARS)
    }
}

// ── Custom Deserialize ───────────────────────────────────────────────
//
// The custom impl is a thin pass-through that lets
// `#[derive(Deserialize)]`-equivalent default field handling proceed
// against a private `StrategyRaw` mirror struct. `StrategyRaw` has no
// `deny_unknown_fields`, so legacy on-disk strategy JSON carrying
// removed keys (e.g. `mechanical_params`) deserializes cleanly — the
// unknown key is ignored.
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
    trader_slot: Option<LLMSlot>,
    risk: RiskConfig,
    #[serde(default = "default_activation_mode")]
    activation_mode: ActivationMode,
    #[serde(default)]
    filter: Option<Filter>,
    #[serde(default)]
    acknowledge_no_filter: bool,
    #[serde(default)]
    decision_mode: DecisionMode,
    #[serde(default)]
    mechanistic_config: Option<MechanisticConfig>,
    #[serde(default)]
    briefing_indicators: Vec<BriefingIndicator>,
    #[serde(default)]
    tunable_bounds: Vec<TunableBound>,
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
            trader_slot: raw.trader_slot,
            risk: raw.risk,
            activation_mode: raw.activation_mode,
            filter: raw.filter,
            acknowledge_no_filter: raw.acknowledge_no_filter,
            decision_mode: raw.decision_mode,
            mechanistic_config: raw.mechanistic_config,
            briefing_indicators: raw.briefing_indicators,
            tunable_bounds: raw.tunable_bounds,
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

    // ── q15-scenario-warmup-bars — min_warmup_bars resolution ──────────

    fn strategy_with_warmup(min_explicit: Option<u32>) -> Strategy {
        Strategy {
            manifest: PublicManifest {
                min_warmup_bars: min_explicit,
                ..make_manifest()
            },
            hypothesis: None,
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            activation_mode: ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: DecisionMode::Agentic,
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
        }
    }

    #[test]
    fn min_warmup_bars_uses_explicit_manifest_value() {
        let s = strategy_with_warmup(Some(42));
        assert_eq!(s.min_warmup_bars(), 42);
    }

    #[test]
    fn min_warmup_bars_falls_back_when_manifest_value_unset() {
        let s = strategy_with_warmup(None);
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
            timeframe_requirements: Default::default(),
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
        }
    }

    #[test]
    fn legacy_strategy_json_parses_with_empty_agents() {
        // Strategy authored before the refactor: has regime/trader_slot
        // fields and no `agents`/`pipeline`. Must still parse — serde(default)
        // gives empty agents and Single pipeline.
        let raw = json!({
            "manifest": make_manifest(),
            "trader_slot": {
                "role": "trader",
                "attested_with": "anthropic.claude-sonnet-4.6+",
                "allowed_tools": []
            },
            "risk": RiskPreset::Balanced.expand()
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
            "risk": RiskPreset::Balanced.expand()
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
                "attested_with": "anthropic.claude-sonnet-4.6+",
                "allowed_tools": []
            },
            "risk": RiskPreset::Balanced.expand()
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
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            activation_mode: ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: DecisionMode::Agentic,
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
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
                activates: None,
                prompt_override: None,
                model_override: None,
                checkpoint: None,
                veto: None,
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
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            activation_mode: ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: DecisionMode::Agentic,
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
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
            "risk": RiskPreset::Balanced.expand()
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
            "risk": RiskPreset::Balanced.expand()
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
            "risk": RiskPreset::Balanced.expand()
        });
        let strategy: Strategy = serde_json::from_value(raw).unwrap();
        let h = strategy.hypothesis.as_ref().unwrap();
        assert_eq!(h.family.as_deref(), Some("mean-reversion"));
        assert!(h.target_regime.is_empty());
        assert!(h.risk_logic.is_none());
    }
}
