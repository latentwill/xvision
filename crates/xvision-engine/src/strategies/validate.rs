use thiserror::Error;

use std::collections::{HashMap, HashSet};

use crate::eval::scenario::Scenario;
use crate::strategies::agent_ref::{canonical_role, EdgePredicate};
use crate::strategies::{DecisionMode, PipelineKind, Strategy};
use xvision_filters::ActivationMode;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("strategy must have at least one agent or filled LLM slot")]
    NoAgents,
    #[error("strategy must have a trader slot (slot ④ Decision Arbiter)")]
    MissingTraderSlot,
    #[error("agent role cannot be empty")]
    EmptyAgentRole,
    #[error("duplicate agent role '{0}'")]
    DuplicateAgentRole(String),
    #[error("single-agent pipeline cannot include multiple agents")]
    InvalidSinglePipeline,
    #[error("graph pipeline edge references unknown role '{0}'")]
    UnknownPipelineRole(String),
    #[error("graph pipeline edge from '{from}' to '{to}' carries a condition predicate but no upstream Filter precedes it")]
    PredicateWithoutUpstreamFilter { from: String, to: String },
    #[error("graph pipeline edge from '{from}' to '{to}' must target a strictly later agent (DAG-strict)")]
    BackwardEdge { from: String, to: String },
    #[error("asset universe cannot be empty")]
    EmptyAssetUniverse,
    #[error("invalid risk config: {0}")]
    InvalidRisk(String),
    #[error("required tool '{0}' not in any slot's allowed_tools")]
    UndeclaredTool(String),
    #[error("mechanistic strategy requires mechanistic_config to be set")]
    MechanisticConfigMissing,
    #[error("mechanistic strategy's mechanistic_config must have at least one entry rule or close policy")]
    MechanisticConfigEmpty,
    #[error("slot '{role}' has both `checkpoint` and `model_override` set; they are mutually exclusive")]
    CheckpointAndModelOverrideConflict { role: String },
    #[error(
        "slot '{role}' checkpoint requires indicators {missing:?} \
         that are not in the strategy's tool registry; \
         add them to the strategy's tools, pick a different checkpoint, \
         or remove the nanochat slot"
    )]
    MissingCheckpointIndicators { role: String, missing: Vec<String> },
    #[error(
        "slot '{role}' checkpoint '{model_id}' has not been live-approved; \
         run the backtest comparison in the strategy builder and confirm \
         it before attaching"
    )]
    CheckpointNotLiveApproved { role: String, model_id: String },
    #[error("auxiliary timeframe '{0}' duplicates the native strategy timeframe")]
    DuplicateNativeTimeframe(String),
    #[error("auxiliary timeframe '{0}' is duplicated in timeframe_requirements")]
    DuplicateAuxiliaryTimeframe(String),
    #[error("unsupported auxiliary timeframe '{0}'. Accepted: 1m, 5m, 15m, 30m, 1h, 2h, 4h, 1d")]
    UnsupportedAuxiliaryTimeframe(String),
    #[error("auxiliary timeframe '{auxiliary}' must be an integer multiple of native timeframe '{native}'")]
    NonNestedAuxiliaryTimeframe { native: String, auxiliary: String },
}

pub fn validate_strategy(b: &Strategy) -> Result<(), ValidationError> {
    if b.decision_mode == DecisionMode::Mechanistic {
        match b.mechanistic_config.as_ref() {
            None => return Err(ValidationError::MechanisticConfigMissing),
            Some(cfg) if !cfg.has_rules() => return Err(ValidationError::MechanisticConfigEmpty),
            _ => {}
        }
        return validate_common(b);
    }
    if !b.agents.is_empty() {
        validate_agent_pipeline(b)?;
        validate_common(b)?;
        return Ok(());
    }

    if b.regime_slot.is_none() && b.trader_slot.is_none() {
        return Err(ValidationError::NoAgents);
    }
    if b.trader_slot.is_none() {
        return Err(ValidationError::MissingTraderSlot);
    }
    validate_common(b)?;

    // Every tool the manifest declares must appear in at least one filled
    // slot's allowed_tools — otherwise the runtime would never grant it.
    for required in &b.manifest.required_tools {
        let granted = [&b.regime_slot, &b.trader_slot]
            .into_iter()
            .flatten()
            .any(|slot| slot.allowed_tools.iter().any(|t| t == required));
        if !granted {
            return Err(ValidationError::UndeclaredTool(required.clone()));
        }
    }
    Ok(())
}

fn parse_supported_timeframe_minutes(tf: &str) -> Option<u32> {
    match tf {
        "1m" => Some(1),
        "5m" => Some(5),
        "15m" => Some(15),
        "30m" => Some(30),
        "1h" => Some(60),
        "2h" => Some(120),
        "4h" => Some(240),
        "1d" => Some(1440),
        _ => None,
    }
}

fn validate_timeframe_requirements(b: &Strategy) -> Result<(), ValidationError> {
    let native = b.native_timeframe();
    let native_minutes = b.manifest.decision_cadence_minutes;
    let mut seen = HashSet::new();
    for tf in &b.manifest.timeframe_requirements.auxiliary {
        let name = tf.as_str();
        if name == native.as_str() {
            return Err(ValidationError::DuplicateNativeTimeframe(name.to_string()));
        }
        if !seen.insert(name.to_string()) {
            return Err(ValidationError::DuplicateAuxiliaryTimeframe(name.to_string()));
        }
        let Some(minutes) = parse_supported_timeframe_minutes(name) else {
            return Err(ValidationError::UnsupportedAuxiliaryTimeframe(name.to_string()));
        };
        if minutes <= native_minutes || minutes % native_minutes != 0 {
            return Err(ValidationError::NonNestedAuxiliaryTimeframe {
                native: native.as_str().to_string(),
                auxiliary: name.to_string(),
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Preflight validate — richer eval-readiness check (track cli-strategy-validate)
// ---------------------------------------------------------------------------

/// The result of a preflight validation. Errors block eval; warnings are
/// surfaced as informational but do not prevent an eval run from launching.
/// `eval_ready` is `true` iff `errors` is empty AND `warnings` is empty.
#[derive(Debug, Clone, Default)]
pub struct PreflightResult {
    /// Hard blockers — the strategy or scenario combination is definitely not
    /// eval-runnable until these are resolved.
    pub errors: Vec<String>,
    /// Soft signals — eval can proceed but the operator may want to review
    /// these before burning credits.
    pub warnings: Vec<String>,
    /// `true` iff both `errors` and `warnings` are empty.
    pub eval_ready: bool,
}

/// Perform a preflight eval-readiness check on a strategy, optionally
/// cross-referenced against a scenario.
///
/// Without a scenario this degrades to a shape-only check (the same checks
/// `validate_strategy` performs, expressed as structured `PreflightResult`
/// rather than a `Result<(), ValidationError>`).
///
/// With a scenario the check additionally verifies:
/// - Scenario's primary asset is in the strategy's `asset_universe`.
/// - Scenario's granularity matches `manifest.decision_cadence_minutes`.
///
/// Provider/model liveness (checks 3-4 in the spec) cannot be performed here
/// without access to the runtime config — that layer lives in `xvision-cli`
/// and passes any provider-enabled errors in via `PreflightResult::errors`
/// after calling this function.
pub fn preflight_validate(strategy: &Strategy, scenario: Option<&Scenario>) -> PreflightResult {
    let mut result = PreflightResult::default();

    // Run shape validation; fold any error into the errors list.
    if let Err(e) = validate_strategy(strategy) {
        result.errors.push(e.to_string());
    }
    if let Err(e) = validate_timeframe_requirements(strategy) {
        result.errors.push(e.to_string());
    }

    if let Some(sc) = scenario {
        // Scenarios are asset-free — the asset a run trades comes from the
        // strategy's `asset_universe`, so there is no scenario-asset to
        // cross-check against the universe. (The former "scenario asset
        // not in strategy asset_universe" warning is removed.)

        // Check 6: scenario granularity matches decision_cadence_minutes.
        let scenario_minutes = (sc.granularity.seconds() / 60) as u32;
        if scenario_minutes != strategy.manifest.decision_cadence_minutes {
            result.warnings.push(format!(
                "timeframe mismatch: scenario granularity is {} min but strategy decision_cadence_minutes is {}",
                scenario_minutes,
                strategy.manifest.decision_cadence_minutes
            ));
        }
    }

    // Fold no-filter warnings into preflight. `no_filter_warnings` (topology-
    // based) and `every_bar_warning` (activation-mode-based) address the same
    // operator concern; emit only one to avoid near-duplicate lines.
    let topo_warnings = no_filter_warnings(strategy);
    if topo_warnings.is_empty() {
        if let Some(w) = every_bar_warning(strategy) {
            result.warnings.push(w);
        }
    } else {
        result.warnings.extend(topo_warnings);
    }

    if let Some(w) = high_position_size_warning(strategy) {
        result.warnings.push(w);
    }
    result.eval_ready = result.errors.is_empty() && result.warnings.is_empty();
    result
}

/// Creation-time warning: strategy will dispatch the LLM pipeline on every bar.
///
/// Fires when `activation_mode == EveryBar && !acknowledge_no_filter`. Independent
/// of agent-graph topology — this is an activation-state check, not a wiring check.
/// Callers that already emit a topology-based warning (see `no_filter_warnings`)
/// should suppress this one to avoid near-duplicate lines for the same concern.
pub fn every_bar_warning(s: &Strategy) -> Option<String> {
    if !s.decision_mode.is_agentic() {
        return None;
    }
    if s.acknowledge_no_filter {
        return None;
    }
    if s.activation_mode == ActivationMode::EveryBar {
        Some(format!(
            "Strategy '{}' has no filter and will activate on every bar — it runs \
             the LLM pipeline on every candle and burns tokens. Attach a \
             deterministic filter so it only acts on good setups. (Pass \
             --no-filter-warning / set acknowledge_no_filter to silence.)",
            s.manifest.display_name
        ))
    } else {
        None
    }
}

/// Returns a warning when `max_position_pct_nav` exceeds the 20% caution threshold.
pub fn high_position_size_warning(strategy: &Strategy) -> Option<String> {
    if strategy.risk.max_position_pct_nav > 20.0 {
        Some(format!(
            "Strategy '{}' has max_position_pct_nav set to {:.1}% (above the 20% caution threshold). \
             The risk layer will allow this but log a warning on each oversized trade.",
            strategy.manifest.display_name, strategy.risk.max_position_pct_nav,
        ))
    } else {
        None
    }
}

/// Phase 2 (firing-filter CLI) — no-Filter soft-warning.
///
/// Returns one warning per decision-like `AgentRef` that has no incoming
/// `PipelineEdge` from an upstream filter-like role. The warning is
/// suppressed entirely when the strategy carries `acknowledge_no_filter = true`.
///
/// The text is stable: downstream tooling (SPA validate panel,
/// scriptable `xvn strategy validate --json`) treats the string
/// verbatim as the warning surface. See contract
/// `team/contracts/agent-firing-filter-cli-verbs.md` acceptance #5.
pub fn no_filter_warnings(strategy: &Strategy) -> Vec<String> {
    if strategy.acknowledge_no_filter {
        return Vec::new();
    }

    let filter_roles: HashSet<String> = strategy
        .agents
        .iter()
        .filter(|a| role_is_filter_like(&a.role))
        .map(|a| canonical_role(&a.role))
        .collect();

    let mut warnings = Vec::new();
    for agent in &strategy.agents {
        if !role_is_decision_like(&agent.role) {
            continue;
        }
        // Only emit the topology-based warning for agents that have explicitly
        // declared an `activates` capability. Agents with `activates: None` are
        // not yet wired into the capability-dispatch pipeline — the activation-
        // mode warning (`every_bar_warning`) covers that case and must not be
        // suppressed by a spurious topology warning. See the contract in
        // `preflight_tests::preflight_every_bar_warning_appears_when_no_topo_warning`
        // and `preflight_no_duplicate_warnings_when_both_checks_fire`.
        if agent.activates.is_none() {
            continue;
        }
        let role = canonical_role(&agent.role);
        let has_upstream_filter_edge = strategy.pipeline.edges.iter().any(|e| {
            canonical_role(&e.to_role) == role && filter_roles.contains(&canonical_role(&e.from_role))
        });
        if has_upstream_filter_edge {
            continue;
        }
        warnings.push(format!(
            "strategy '{}' has a Trader agent with no saved JSON filter — it will dispatch on every bar. Attach a strategy filter to reduce LLM cost.",
            strategy.manifest.display_name,
        ));
    }
    warnings
}

/// Phase C extension — predicate `signal_field` warning.
///
/// For each `PipelineEdge.condition`, walk every `signal_field`
/// referenced (recursively into `All`/`Any`/`Not`) and look up the
/// upstream Filter slot's `system_prompt`. If the prompt does not
/// mention the field name (case-insensitive substring scan), record a
/// warning. The scan is intentionally lenient — operators may rename
/// fields between schema changes and we'd rather not error.
///
/// `system_prompts_by_role` maps the canonicalised role name to the
/// owning `AgentSlot.system_prompt`. The caller (api/eval/strategy
/// CRUD) builds this map from the strategy's bound agents before
/// invoking this helper. Missing entries (role exists in the strategy
/// but no slot was supplied) are skipped — the caller has already
/// rejected unknown roles via `validate_strategy`.
///
/// Returns the list of warning strings — the caller folds these into
/// its own `PreflightResult` or `Vec<String>` surface.
///
/// Note: this is the field-existence side of the contract's
/// `validate.rs` extension. The "no upstream Filter at all" check
/// stays an error in Phase B's `validate_strategy`
/// (`PredicateWithoutUpstreamFilter`) — operators can't silence that
/// one, but the field-existence drift below is just a warning.
pub fn predicate_signal_field_warnings(
    strategy: &Strategy,
    system_prompts_by_role: &HashMap<String, String>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if strategy.pipeline.kind != PipelineKind::Graph {
        return warnings;
    }
    for edge in &strategy.pipeline.edges {
        let Some(predicate) = edge.condition.as_ref() else {
            continue;
        };
        let from = canonical_role(&edge.from_role);
        let to = canonical_role(&edge.to_role);
        let from_idx = strategy
            .agents
            .iter()
            .position(|a| canonical_role(&a.role) == from);
        let Some(from_idx) = from_idx else { continue };

        // Collect every upstream Filter prompt at index <= from_idx —
        // any of them may declare the field. The scan is lenient: if
        // any upstream Filter mentions the field, we accept it (no
        // warning). If none mention it, we warn.
        let upstream_filter_prompts: Vec<&str> = strategy
            .agents
            .iter()
            .take(from_idx + 1)
            .filter(|a| role_is_filter_like(&a.role))
            .filter_map(|a| {
                system_prompts_by_role
                    .get(&canonical_role(&a.role))
                    .map(String::as_str)
            })
            .collect();
        if upstream_filter_prompts.is_empty() {
            // No upstream Filter has a known prompt — defer to the
            // Phase B `PredicateWithoutUpstreamFilter` error path. We
            // intentionally do not warn here so we don't double-fire.
            continue;
        }

        for field in collect_signal_fields(predicate) {
            let head = field.split('.').next().unwrap_or(&field).to_ascii_lowercase();
            let mentioned = upstream_filter_prompts
                .iter()
                .any(|p| p.to_ascii_lowercase().contains(&head));
            if !mentioned {
                warnings.push(format!(
                    "graph pipeline edge from '{}' to '{}' references signal_field '{}' which is not declared in any upstream Filter system_prompt; predicate may never match",
                    from, to, field,
                ));
            }
        }
    }
    warnings
}

/// Walk an `EdgePredicate` and collect every `signal_field` leaf
/// referenced. Recurses through `All`/`Any`/`Not` so nested predicates
/// surface their fields.
fn collect_signal_fields(predicate: &EdgePredicate) -> Vec<String> {
    let mut out = Vec::new();
    collect_signal_fields_into(predicate, &mut out);
    out
}

fn collect_signal_fields_into(predicate: &EdgePredicate, out: &mut Vec<String>) {
    match predicate {
        EdgePredicate::Eq { signal_field, .. }
        | EdgePredicate::Neq { signal_field, .. }
        | EdgePredicate::Gte { signal_field, .. }
        | EdgePredicate::Lte { signal_field, .. }
        | EdgePredicate::In { signal_field, .. } => {
            out.push(signal_field.clone());
        }
        EdgePredicate::All(inner) | EdgePredicate::Any(inner) => {
            for p in inner {
                collect_signal_fields_into(p, out);
            }
        }
        EdgePredicate::Not(inner) => collect_signal_fields_into(inner, out),
    }
}

fn validate_common(b: &Strategy) -> Result<(), ValidationError> {
    if b.manifest.asset_universe.is_empty() {
        return Err(ValidationError::EmptyAssetUniverse);
    }
    if b.risk.risk_pct_per_trade <= 0.0 || b.risk.risk_pct_per_trade > 0.5 {
        return Err(ValidationError::InvalidRisk(format!(
            "risk_pct_per_trade must be in (0, 0.5], got {}",
            b.risk.risk_pct_per_trade
        )));
    }
    if b.risk.max_leverage <= 0.0 || b.risk.max_leverage > 100.0 {
        return Err(ValidationError::InvalidRisk(format!(
            "max_leverage must be in (0, 100], got {}",
            b.risk.max_leverage
        )));
    }
    Ok(())
}

fn validate_agent_pipeline(b: &Strategy) -> Result<(), ValidationError> {
    // Canonical form across the engine: trim + ASCII lowercase. The
    // serde layer normalizes roles on deserialize/serialize, so most
    // strategies arrive here already canonical — but programmatic
    // constructions can carry raw values, and validation must produce
    // the same answer for both paths.
    let mut roles: HashSet<String> = HashSet::new();
    for agent in &b.agents {
        let role = canonical_role(&agent.role);
        if role.is_empty() {
            return Err(ValidationError::EmptyAgentRole);
        }
        if !roles.insert(role.clone()) {
            return Err(ValidationError::DuplicateAgentRole(role));
        }
    }
    // Task 4.1: checkpoint and model_override are mutually exclusive per slot.
    for agent in &b.agents {
        if agent.checkpoint.is_some() && agent.model_override.is_some() {
            return Err(ValidationError::CheckpointAndModelOverrideConflict {
                role: agent.role.clone(),
            });
        }
    }

    if b.pipeline.kind == PipelineKind::Single && b.agents.len() > 1 {
        return Err(ValidationError::InvalidSinglePipeline);
    }
    if b.pipeline.kind == PipelineKind::Graph {
        for edge in &b.pipeline.edges {
            let from = canonical_role(&edge.from_role);
            let to = canonical_role(&edge.to_role);
            if !roles.contains(&from) {
                return Err(ValidationError::UnknownPipelineRole(edge.from_role.clone()));
            }
            if !roles.contains(&to) {
                return Err(ValidationError::UnknownPipelineRole(edge.to_role.clone()));
            }

            // Phase B (capability-dispatch): edges with a predicate
            // must have at least one upstream Filter (by `activates`)
            // in strategy order; otherwise the predicate could never
            // fire because no `FilterSignal` would have been produced.
            // Edges with `condition: None` always pass — they are the
            // unconditional fall-through case.
            if let Some(predicate) = edge.condition.as_ref() {
                let from_idx = b
                    .agents
                    .iter()
                    .position(|a| canonical_role(&a.role) == from)
                    .unwrap_or(usize::MAX);
                let to_idx = b
                    .agents
                    .iter()
                    .position(|a| canonical_role(&a.role) == to)
                    .unwrap_or(usize::MAX);

                // DAG-strict: forward-only edges. Backward / self
                // targets are cycle introductions — reject at draft
                // time so Router-style runtime fall-through cannot
                // smuggle them in.
                if from_idx != usize::MAX && to_idx != usize::MAX && to_idx <= from_idx {
                    return Err(ValidationError::BackwardEdge {
                        from: edge.from_role.clone(),
                        to: edge.to_role.clone(),
                    });
                }

                if !predicate_has_upstream_filter(b, from_idx, predicate) {
                    return Err(ValidationError::PredicateWithoutUpstreamFilter {
                        from: edge.from_role.clone(),
                        to: edge.to_role.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Returns true when at least one `AgentRef` at index `<= from_idx`
/// has a filter-like role. Conservative on `usize::MAX` (unknown role)
/// because the upstream caller has already rejected that case.
///
/// `_predicate` is reserved for a future enrichment where the validator
/// inspects the predicate's `signal_field` against the Filter's typed
/// payload schema. Phase B only checks existence.
fn predicate_has_upstream_filter(strategy: &Strategy, from_idx: usize, _predicate: &EdgePredicate) -> bool {
    if from_idx == usize::MAX {
        return false;
    }
    strategy
        .agents
        .iter()
        .take(from_idx + 1)
        .any(|a| role_is_filter_like(&a.role))
}

fn role_is_filter_like(role: &str) -> bool {
    let role = canonical_role(role);
    role.contains("filter") || role.contains("regime") || role.contains("signal")
}

fn role_is_decision_like(role: &str) -> bool {
    let role = canonical_role(role);
    role.contains("trader") || role.contains("executor") || role.contains("arbiter") || role == "main"
}

#[cfg(test)]
mod preflight_tests {
    use super::*;
    use crate::eval::scenario::{
        AdjustmentMode, AssetClass, BarCachePolicy, CalendarRef, DataSource, Fees, FillModel, LatencyModel,
        LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario, ScenarioSource,
        SlippageModel, TimeWindow, Venue, VenueSettings,
    };
    use crate::safety::VenueLabel;
    use crate::strategies::{manifest::PublicManifest, risk::RiskPreset, AgentRef, PipelineDef, Strategy};
    use chrono::{TimeZone, Utc};
    use xvision_data::alpaca::BarGranularity;

    fn make_strategy_with_agent(asset: &str, cadence_minutes: u32) -> Strategy {
        Strategy {
            manifest: PublicManifest {
                id: "01HZSTRAT".into(),
                display_name: "Test Strategy".into(),
                plain_summary: "test".into(),
                creator: "@test".into(),
                template: "custom".into(),
                regime_fit: vec![],
                asset_universe: vec![asset.to_string()],
                decision_cadence_minutes: cadence_minutes,
                timeframe_requirements: Default::default(),
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
            agents: vec![AgentRef {
                agent_id: "01HZAGENT".into(),
                role: "trader".into(),
                activates: None,
                prompt_override: None,
                model_override: None,
                checkpoint: None,
                veto: None,
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
        }
    }

    fn make_eth_4h_scenario() -> Scenario {
        Scenario {
            id: "sc_test".into(),
            parent_scenario_id: None,
            source: ScenarioSource::User,
            display_name: "ETH 4h sprint".into(),
            description: "".into(),
            tags: vec![],
            notes: None,
            asset_class: AssetClass::Crypto,
            quote_currency: QuoteCurrency::Usd,
            time_window: TimeWindow {
                start: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2025, 1, 8, 0, 0, 0).unwrap(),
            },
            granularity: BarGranularity::Hour4,
            timezone: "UTC".into(),
            calendar: CalendarRef::Continuous24x7,
            data_source: DataSource::AlpacaHistorical {
                feed: None,
                adjustment: AdjustmentMode::Raw,
            },
            venue: VenueSettings {
                venue: Venue::Alpaca,
                fees: Fees {
                    maker_bps: 10,
                    taker_bps: 25,
                },
                slippage: SlippageModel::None,
                latency: LatencyModel {
                    decision_to_fill_ms: 0,
                },
                fill_model: FillModel {
                    market_order_fill: MarketOrderFill::FullAtClose,
                    limit_order_fill: LimitOrderFill::NeverFills,
                    partial_fills: false,
                    volume_constraints: None,
                },
                overrides: Vec::new(),
                borrow_bps_per_day: 5.0,
            },
            replay_mode: ReplayMode::Continuous,
            capital: xvision_core::Capital::default(),
            bar_cache_policy: BarCachePolicy {
                cache_key: "k".into(),
                refresh_policy: RefreshPolicy::NeverRefresh,
                data_fetched_at: None,
            },
            warmup_bars: 200,
            regime_label: None,
            volatility_label: None,
            trend_direction: None,
            regime_derived: false,
            created_at: Utc::now(),
            created_by: "t".into(),
            archived_at: None,
            venue_label: VenueLabel::Paper,
            safety_limits: None,
        }
    }

    // ── shape-only validation (no scenario) ─────────────────────────────

    #[test]
    fn preflight_no_scenario_is_shape_only_accepts_valid_strategy() {
        let strategy = make_strategy_with_agent("ETH/USD", 240);
        let result = preflight_validate(&strategy, None);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    }

    #[test]
    fn preflight_no_scenario_rejects_strategy_with_no_agents() {
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.agents.clear();
        // also clear legacy slots to trigger NoAgents
        strategy.trader_slot = None;
        let result = preflight_validate(&strategy, None);
        assert!(
            !result.errors.is_empty(),
            "expected error for zero agents, got none"
        );
    }

    // ── with-scenario checks ─────────────────────────────────────────────

    #[test]
    fn preflight_with_scenario_timeframe_match_produces_no_timeframe_warning() {
        // Strategy cadence 240 (4h); scenario granularity Hour4 → 240 min
        let strategy = make_strategy_with_agent("ETH/USD", 240);
        let scenario = make_eth_4h_scenario();
        let result = preflight_validate(&strategy, Some(&scenario));
        let tf_warnings: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| w.contains("timeframe") || w.contains("cadence"))
            .collect();
        assert!(
            tf_warnings.is_empty(),
            "unexpected timeframe warnings: {tf_warnings:?}"
        );
    }

    #[test]
    fn preflight_with_scenario_timeframe_mismatch_produces_warning() {
        // Strategy cadence 60 (1h); scenario granularity Hour4 (240 min)
        let strategy = make_strategy_with_agent("ETH/USD", 60);
        let scenario = make_eth_4h_scenario(); // granularity = 4h = 240 min
        let result = preflight_validate(&strategy, Some(&scenario));
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("timeframe") || w.contains("cadence")),
            "expected timeframe mismatch warning, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn preflight_eval_ready_true_when_no_errors_and_no_warnings() {
        // A strategy that acknowledges the no-filter state produces no warnings,
        // so eval_ready is true given a matching scenario.
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.acknowledge_no_filter = true;
        let scenario = make_eth_4h_scenario();
        let result = preflight_validate(&strategy, Some(&scenario));
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
        assert!(result.eval_ready);
    }

    #[test]
    fn every_bar_warning_fires_for_every_bar_without_acknowledge() {
        let strategy = make_strategy_with_agent("ETH/USD", 240);
        assert!(strategy.activation_mode == xvision_filters::ActivationMode::EveryBar);
        assert!(!strategy.acknowledge_no_filter);
        let w = every_bar_warning(&strategy);
        assert!(w.is_some(), "expected warning for EveryBar without acknowledge");
        let msg = w.unwrap();
        assert!(msg.contains("burns tokens"), "warning must mention token cost");
        assert!(msg.contains("good setups"), "warning must mention setups");
    }

    #[test]
    fn every_bar_warning_suppressed_when_acknowledged() {
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.acknowledge_no_filter = true;
        assert!(every_bar_warning(&strategy).is_none());
    }

    #[test]
    fn every_bar_warning_suppressed_when_filter_gated() {
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.activation_mode = xvision_filters::ActivationMode::FilterGated;
        assert!(every_bar_warning(&strategy).is_none());
    }

    #[test]
    fn preflight_every_bar_warning_appears_when_no_topo_warning() {
        // A fresh EveryBar strategy with activates:None agents gets the
        // activation-mode warning (topology check is silent for activates:None).
        let strategy = make_strategy_with_agent("ETH/USD", 240);
        let result = preflight_validate(&strategy, None);
        assert!(
            result.warnings.iter().any(|w| w.contains("burns tokens")),
            "expected every_bar_warning in preflight output, got: {:?}",
            result.warnings,
        );
    }

    // ── mechanistic validation ───────────────────────────────────────────────

    #[test]
    fn mechanistic_strategy_with_rules_passes_validate() {
        use crate::strategies::mechanistic::{ClosePolicy, MechanisticConfig};
        let mut s = make_strategy_with_agent("BTC/USD", 60);
        s.decision_mode = DecisionMode::Mechanistic;
        s.mechanistic_config = Some(MechanisticConfig {
            entry_rules: vec![],
            close_policies: vec![ClosePolicy::StopLoss { pct: 2.0 }],
        });
        s.agents.clear();
        s.trader_slot = None;
        validate_strategy(&s).expect("mechanistic with close policy must pass validate_strategy");
    }

    #[test]
    fn mechanistic_strategy_with_empty_config_fails_validate() {
        use crate::strategies::mechanistic::MechanisticConfig;
        let mut s = make_strategy_with_agent("BTC/USD", 60);
        s.decision_mode = DecisionMode::Mechanistic;
        s.mechanistic_config = Some(MechanisticConfig::default());
        s.agents.clear();
        let err = validate_strategy(&s).expect_err("empty mechanistic_config must fail");
        assert!(
            matches!(err, ValidationError::MechanisticConfigEmpty),
            "expected MechanisticConfigEmpty, got: {err:?}",
        );
    }

    #[test]
    fn every_bar_warning_returns_none_for_mechanistic_strategy() {
        let mut s = make_strategy_with_agent("BTC/USD", 60);
        s.decision_mode = DecisionMode::Mechanistic;
        assert!(
            every_bar_warning(&s).is_none(),
            "mechanistic strategy must not produce every_bar_warning (no LLM cost)"
        );
    }

    #[test]
    fn preflight_no_duplicate_warnings_when_both_checks_fire() {
        // A strategy with an explicit Trader (activates) and no Filter edge
        // would fire the topology check. In that case only the topology warning
        // should appear; the every_bar_warning is suppressed to avoid duplication.
        use crate::agents::Capability;
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        // Set activates so no_filter_warnings fires.
        strategy.agents[0].activates = Some(Capability::Trader);
        let result = preflight_validate(&strategy, None);
        // Should have exactly one warning (topo-based), not two.
        assert_eq!(
            result.warnings.len(),
            1,
            "expected exactly one warning, got: {:?}",
            result.warnings
        );
        assert!(
            result.warnings[0].contains("dispatch on every bar"),
            "expected topology-based warning, got: {}",
            result.warnings[0]
        );
    }

    #[test]
    fn preflight_eval_ready_false_when_errors_present() {
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.agents.clear();
        strategy.trader_slot = None;
        let result = preflight_validate(&strategy, None);
        assert!(!result.eval_ready);
    }

    // ── high_position_size_warning ───────────────────────────────────────

    #[test]
    fn high_position_size_warning_returns_none_at_exactly_20() {
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.risk.max_position_pct_nav = 20.0;
        assert!(
            high_position_size_warning(&strategy).is_none(),
            "exactly 20.0 must not trigger the warning"
        );
    }

    #[test]
    fn high_position_size_warning_returns_none_below_20() {
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.risk.max_position_pct_nav = 10.0;
        assert!(high_position_size_warning(&strategy).is_none());
    }

    #[test]
    fn high_position_size_warning_returns_some_above_20() {
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.risk.max_position_pct_nav = 50.0;
        let w = high_position_size_warning(&strategy);
        assert!(w.is_some(), "50.0 must trigger the warning");
    }

    #[test]
    fn high_position_size_warning_includes_pct_and_name() {
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.manifest.display_name = "MyStrat".into();
        strategy.risk.max_position_pct_nav = 50.0;
        let msg = high_position_size_warning(&strategy).unwrap();
        assert!(msg.contains("MyStrat"), "warning must include strategy name");
        assert!(msg.contains("50.0%"), "warning must include formatted pct");
        assert!(
            msg.contains("20% caution"),
            "warning must mention the 20% threshold"
        );
    }

    #[test]
    fn high_position_size_warning_boundary_just_above_20() {
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.risk.max_position_pct_nav = 20.0001;
        assert!(
            high_position_size_warning(&strategy).is_some(),
            "any value strictly above 20.0 must trigger the warning"
        );
    }

    #[test]
    fn preflight_includes_high_position_size_warning() {
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.risk.max_position_pct_nav = 50.0;
        strategy.acknowledge_no_filter = true; // suppress the filter warning
        let result = preflight_validate(&strategy, None);
        assert!(
            result.warnings.iter().any(|w| w.contains("max_position_pct_nav")),
            "preflight must propagate high_position_size_warning, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn preflight_both_filter_and_position_warnings_appear() {
        // EveryBar + no filter + high position → two distinct warnings
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.risk.max_position_pct_nav = 50.0;
        // leave acknowledge_no_filter = false and activation_mode = EveryBar
        let result = preflight_validate(&strategy, None);
        assert!(
            result.warnings.iter().any(|w| w.contains("burns tokens")),
            "must include every_bar_warning"
        );
        assert!(
            result.warnings.iter().any(|w| w.contains("max_position_pct_nav")),
            "must include high_position_size_warning"
        );
        assert_eq!(
            result.warnings.len(),
            2,
            "exactly 2 warnings expected, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn preflight_eval_ready_false_when_only_high_position_warning() {
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.risk.max_position_pct_nav = 50.0;
        strategy.acknowledge_no_filter = true; // suppress filter warning
        let result = preflight_validate(&strategy, None);
        assert!(!result.errors.is_empty() || !result.warnings.is_empty());
        assert!(
            !result.eval_ready,
            "eval_ready must be false when warnings present"
        );
    }

    // ── Task 4.1: CheckpointAndModelOverrideConflict ─────────────────────

    fn strategy_with_checkpoint_and_model_override() -> Strategy {
        use crate::agents::Capability;
        use crate::strategies::agent_ref::{CheckpointRef, PipelineDef};
        let mut s = make_strategy_with_agent("ETH/USD", 240);
        s.agents = vec![
            AgentRef {
                agent_id: "01HZAGENT".into(),
                role: "filter".into(),
                activates: Some(Capability::Filter),
                prompt_override: None,
                model_override: Some("anthropic/claude-haiku-4-5".into()),
                checkpoint: Some(CheckpointRef {
                    model_id: "01HZMODEL".into(),
                }),
                veto: Some(true),
            },
            AgentRef {
                agent_id: "01HZTRADER".into(),
                role: "trader".into(),
                activates: Some(Capability::Trader),
                prompt_override: None,
                model_override: None,
                checkpoint: None,
                veto: None,
            },
        ];
        s.pipeline = PipelineDef::sequential();
        s
    }

    #[test]
    fn checkpoint_and_model_override_on_same_slot_is_rejected() {
        let s = strategy_with_checkpoint_and_model_override();
        let err = validate_strategy(&s).expect_err("must reject checkpoint+model_override");
        assert!(
            matches!(err, ValidationError::CheckpointAndModelOverrideConflict { .. }),
            "wrong error: {err:?}"
        );
        assert!(
            err.to_string().contains("filter"),
            "error must name the offending role: {err}"
        );
    }

    #[test]
    fn checkpoint_without_model_override_is_accepted() {
        let mut s = strategy_with_checkpoint_and_model_override();
        s.agents[0].model_override = None; // remove the conflicting field
                                           // Should pass the mutual-exclusion check (other validations may fail
                                           // if the Strategy fixture is incomplete, but not this one).
        let err = validate_strategy(&s);
        match err {
            Ok(()) => {}
            Err(ValidationError::CheckpointAndModelOverrideConflict { .. }) => {
                panic!("checkpoint without model_override must not produce conflict error")
            }
            Err(_) => {} // other validation errors are fine for this fixture
        }
    }
}
