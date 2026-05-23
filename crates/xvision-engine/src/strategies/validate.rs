use thiserror::Error;

use std::collections::{HashMap, HashSet};

use crate::agents::Capability;
use crate::eval::scenario::Scenario;
use crate::strategies::agent_ref::{canonical_role, EdgePredicate};
use crate::strategies::{PipelineKind, Strategy};

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
}

pub fn validate_strategy(b: &Strategy) -> Result<(), ValidationError> {
    if !b.agents.is_empty() {
        validate_agent_pipeline(b)?;
        validate_common(b)?;
        return Ok(());
    }

    if b.regime_slot.is_none() && b.intern_slot.is_none() && b.trader_slot.is_none() {
        return Err(ValidationError::NoAgents);
    }
    if b.trader_slot.is_none() {
        return Err(ValidationError::MissingTraderSlot);
    }
    validate_common(b)?;

    // Every tool the manifest declares must appear in at least one filled
    // slot's allowed_tools — otherwise the runtime would never grant it.
    for required in &b.manifest.required_tools {
        let granted = [&b.regime_slot, &b.intern_slot, &b.trader_slot]
            .into_iter()
            .flatten()
            .any(|slot| slot.allowed_tools.iter().any(|t| t == required));
        if !granted {
            return Err(ValidationError::UndeclaredTool(required.clone()));
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

    if let Some(sc) = scenario {
        // Check 5: scenario asset is in strategy's asset_universe.
        let scenario_venue_symbol = sc.asset.first().map(|a| a.venue_symbol.as_str()).unwrap_or("");
        let scenario_symbol = sc.asset.first().map(|a| a.symbol.as_str()).unwrap_or("");
        let in_universe = strategy.manifest.asset_universe.iter().any(|a| {
            let a_norm = normalize_asset(a);
            a_norm == normalize_asset(scenario_venue_symbol)
                || a_norm == normalize_asset(&format!("{scenario_symbol}/USD"))
                || a_norm == normalize_asset(scenario_symbol)
        });
        if !in_universe {
            let strategy_assets = strategy.manifest.asset_universe.join(", ");
            result.warnings.push(format!(
                "scenario asset {scenario_venue_symbol} is not in strategy asset_universe [{strategy_assets}]"
            ));
        }

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

    // Phase 2 (firing-filter CLI) — fold the no-Filter soft-warning
    // into the preflight surface so the SPA validate panel and
    // `xvn strategy validate --json` paths see it without duplicating
    // the check. `eval_ready` is intentionally re-derived AFTER folding
    // so a no-Filter warning prevents the green-checkmark UI.
    result.warnings.extend(no_filter_warnings(strategy));

    result.eval_ready = result.errors.is_empty() && result.warnings.is_empty();
    result
}

/// Phase 2 (firing-filter CLI) — no-Filter soft-warning.
///
/// Returns one warning per `AgentRef` whose `activates` is explicitly
/// `Trader` or `Critic` and which has no incoming `PipelineEdge` from
/// an upstream Filter `AgentRef`. The warning is suppressed entirely
/// when the strategy carries `acknowledge_no_filter = true`.
///
/// The check intentionally fires only on agents whose `activates` is
/// explicitly set — legacy strategies with `activates: None` are
/// considered "pre-capability-model" and not nagged. Phase E
/// (`agent-graph-template-capabilities`) flips every starter template
/// to explicit `activates`, at which point this warning starts firing
/// in practice for any unfiltered trader.
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
        .filter(|a| matches!(a.activates, Some(Capability::Filter)))
        .map(|a| canonical_role(&a.role))
        .collect();

    let mut warnings = Vec::new();
    for agent in &strategy.agents {
        let acts_as_trader_or_critic = matches!(
            agent.activates,
            Some(Capability::Trader) | Some(Capability::Critic)
        );
        if !acts_as_trader_or_critic {
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
            "strategy '{}' has a Trader agent with no upstream Filter — it will dispatch on every bar. Consider adding a Filter to reduce LLM cost. (See: xvn agent create --capability filter)",
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
            .filter(|a| matches!(a.activates, Some(Capability::Filter)))
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

fn normalize_asset(asset: &str) -> String {
    asset.trim().to_ascii_uppercase()
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

                // Walk agents[0..=from_idx] looking for a Filter on
                // `activates`. Phase B heuristic — the slot's full
                // `capabilities` set isn't accessible from the
                // strategy alone. Phase C may strengthen this once
                // Filter signal schemas are typed.
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
/// activates `Capability::Filter`. Conservative on `usize::MAX`
/// (unknown role) — the upstream caller has already rejected the
/// `UnknownPipelineRole` case so this is defensive.
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
        .any(|a| matches!(a.activates, Some(Capability::Filter)))
}

#[cfg(test)]
mod preflight_tests {
    use super::*;
    use crate::eval::scenario::{
        AdjustmentMode, AssetClass, AssetRef, BarCachePolicy, CalendarRef, DataSource, Fees, FillModel,
        LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario,
        ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings,
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
                attested_with: vec![],
                required_tools: vec![],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
            },
            hypothesis: None,
            agents: vec![AgentRef {
                agent_id: "01HZAGENT".into(),
                role: "trader".into(),
                activates: None,
            }],
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({}),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
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
            asset: vec![AssetRef {
                class: AssetClass::Crypto,
                symbol: "ETH".into(),
                venue_symbol: "ETH/USD".into(),
            }],
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
    fn preflight_with_scenario_asset_match_produces_no_warnings() {
        let strategy = make_strategy_with_agent("ETH/USD", 240);
        let scenario = make_eth_4h_scenario();
        let result = preflight_validate(&strategy, Some(&scenario));
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
        // Asset universe ["ETH/USD"] contains scenario asset "ETH/USD" → no warning
        let asset_warnings: Vec<_> = result.warnings.iter().filter(|w| w.contains("asset")).collect();
        assert!(
            asset_warnings.is_empty(),
            "expected no asset warnings, got: {asset_warnings:?}"
        );
    }

    #[test]
    fn preflight_with_scenario_asset_mismatch_produces_warning() {
        // Strategy has ETH/USD; scenario has SOL/USD
        let strategy = make_strategy_with_agent("ETH/USD", 240);
        let mut scenario = make_eth_4h_scenario();
        scenario.asset = vec![AssetRef {
            class: AssetClass::Crypto,
            symbol: "SOL".into(),
            venue_symbol: "SOL/USD".into(),
        }];
        let result = preflight_validate(&strategy, Some(&scenario));
        assert!(result.errors.is_empty(), "asset mismatch is a warning, not error");
        assert!(
            result.warnings.iter().any(|w| w.contains("SOL/USD")),
            "expected SOL/USD warning, got: {:?}",
            result.warnings
        );
    }

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
        let strategy = make_strategy_with_agent("ETH/USD", 240);
        let scenario = make_eth_4h_scenario();
        let result = preflight_validate(&strategy, Some(&scenario));
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
        assert!(result.eval_ready);
    }

    #[test]
    fn preflight_eval_ready_false_when_errors_present() {
        let mut strategy = make_strategy_with_agent("ETH/USD", 240);
        strategy.agents.clear();
        strategy.trader_slot = None;
        let result = preflight_validate(&strategy, None);
        assert!(!result.eval_ready);
    }
}
