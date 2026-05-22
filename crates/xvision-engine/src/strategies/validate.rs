use thiserror::Error;

use std::collections::HashSet;

use crate::eval::scenario::Scenario;
use crate::strategies::agent_ref::canonical_role;
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
    #[error("asset universe cannot be empty")]
    EmptyAssetUniverse,
    #[error("invalid risk config: {0}")]
    InvalidRisk(String),
    #[error("required tool '{0}' not in any slot's allowed_tools")]
    UndeclaredTool(String),
    #[error("prompt/manifest drift: {0}")]
    PromptManifestDrift(String),
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

    result.eval_ready = result.errors.is_empty() && result.warnings.is_empty();
    result
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
    validate_prompt_manifest_alignment(b)?;
    Ok(())
}

fn validate_prompt_manifest_alignment(b: &Strategy) -> Result<(), ValidationError> {
    let prompt = legacy_prompt_text(b);
    if prompt.trim().is_empty() {
        return Ok(());
    }

    let manifest_assets: HashSet<String> = b
        .manifest
        .asset_universe
        .iter()
        .map(|asset| normalize_asset(asset))
        .collect();
    let mut problems = Vec::new();

    for mentioned in mentioned_assets(&prompt) {
        if !manifest_assets.contains(&mentioned) {
            problems.push(format!(
                "prompt mentions {mentioned} but manifest asset_universe is [{}]",
                b.manifest.asset_universe.join(", ")
            ));
        }
    }

    for mentioned in mentioned_cadences_minutes(&prompt) {
        if mentioned != b.manifest.decision_cadence_minutes {
            problems.push(format!(
                "prompt mentions {} but manifest decision_cadence_minutes is {}m",
                cadence_label(mentioned),
                b.manifest.decision_cadence_minutes
            ));
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(ValidationError::PromptManifestDrift(problems.join("; ")))
    }
}

fn legacy_prompt_text(b: &Strategy) -> String {
    [&b.regime_slot, &b.intern_slot, &b.trader_slot]
        .into_iter()
        .flatten()
        .map(|slot| slot.prompt.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn mentioned_assets(prompt: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in prompt.split_whitespace() {
        let token = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '/');
        if token.contains('/') {
            let asset = normalize_asset(token);
            if asset.ends_with("/USD") && !out.contains(&asset) {
                out.push(asset);
            }
        }
    }
    out
}

fn mentioned_cadences_minutes(prompt: &str) -> Vec<u32> {
    let words: Vec<String> = prompt
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                .to_ascii_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect();
    let mut out = Vec::new();
    for (index, word) in words.iter().enumerate() {
        if let Some(minutes) = cadence_word_minutes(word) {
            push_unique_minutes(&mut out, minutes);
            continue;
        }
        // Cross-word form: a bare integer followed by an exact unit
        // token. Must be an exact match — `starts_with('m')` collided
        // with English numbered lists like "3. Mean Reversion".
        if let Ok(value) = word.parse::<u32>() {
            if let Some(unit) = words.get(index + 1) {
                if let Some(multiplier) = cadence_unit_multiplier(unit) {
                    push_unique_minutes(&mut out, value * multiplier);
                }
            }
        }
    }
    out
}

fn cadence_unit_multiplier(unit: &str) -> Option<u32> {
    match unit {
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(60),
        "m" | "min" | "mins" | "minute" | "minutes" => Some(1),
        _ => None,
    }
}

fn cadence_word_minutes(word: &str) -> Option<u32> {
    let trimmed = word
        .strip_suffix("-hour")
        .or_else(|| word.strip_suffix("-hours"))
        .or_else(|| word.strip_suffix("hour"))
        .or_else(|| word.strip_suffix("hours"))
        .map(|value| (value, 60))
        .or_else(|| {
            word.strip_suffix("-minute")
                .or_else(|| word.strip_suffix("-minutes"))
                .or_else(|| word.strip_suffix("minute"))
                .or_else(|| word.strip_suffix("minutes"))
                .map(|value| (value, 1))
        })
        .or_else(|| word.strip_suffix('h').map(|value| (value, 60)))
        .or_else(|| word.strip_suffix('m').map(|value| (value, 1)))?;
    let (value, multiplier) = trimmed;
    value.parse::<u32>().ok().map(|n| n * multiplier)
}

fn normalize_asset(asset: &str) -> String {
    asset.trim().to_ascii_uppercase()
}

fn cadence_label(minutes: u32) -> String {
    if minutes.is_multiple_of(60) {
        format!("{}h", minutes / 60)
    } else {
        format!("{minutes}m")
    }
}

fn push_unique_minutes(out: &mut Vec<u32>, minutes: u32) {
    if !out.contains(&minutes) {
        out.push(minutes);
    }
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
        }
    }
    Ok(())
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
            }],
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({}),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
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

#[cfg(test)]
mod cadence_tests {
    use super::{cadence_unit_multiplier, mentioned_cadences_minutes};

    #[test]
    fn numbered_list_items_do_not_trigger_cadence_match() {
        // Regression: the live "Multi-Factor Logic Agent" prompt
        // (strategy 01KRZ0ZWER9HE2CTYNWT83ESYQ, 2026-05-19) tripped a
        // false positive because "3. Mean Reversion" tokenizes as
        // `3` + `mean`, which the old `starts_with('m')` heuristic
        // matched as "3 minutes". Only the real "4 hours" cadence
        // should be detected.
        let prompt = "In every decision step (which occurs every 4 hours), \
                      evaluate the market using three factors: \
                      1. Trend: Is the price consistently above the 50-period MA? \
                      2. Conviction: Has volume increased significantly? \
                      3. Mean Reversion: Is RSI indicating an extreme condition?";
        let found = mentioned_cadences_minutes(prompt);
        assert_eq!(found, vec![240], "expected only 4h cadence, got {found:?}");
    }

    #[test]
    fn cross_word_form_accepts_exact_unit_tokens() {
        assert_eq!(mentioned_cadences_minutes("every 4 hours"), vec![240]);
        assert_eq!(mentioned_cadences_minutes("every 15 minutes"), vec![15]);
        assert_eq!(mentioned_cadences_minutes("every 1 hour"), vec![60]);
        assert_eq!(mentioned_cadences_minutes("every 30 min"), vec![30]);
        assert_eq!(mentioned_cadences_minutes("every 2 hrs"), vec![120]);
    }

    #[test]
    fn attached_unit_suffix_still_works() {
        // The single-token "5m" / "1h" path lives in
        // cadence_word_minutes; preserve it.
        assert_eq!(mentioned_cadences_minutes("rebalance every 5m"), vec![5]);
        assert_eq!(mentioned_cadences_minutes("rebalance every 1h"), vec![60]);
        assert_eq!(mentioned_cadences_minutes("every 4-hour bar"), vec![240]);
    }

    #[test]
    fn unit_multiplier_rejects_unrelated_words() {
        // Words that merely start with 'm' or 'h' must NOT match.
        assert_eq!(cadence_unit_multiplier("mean"), None);
        assert_eq!(cadence_unit_multiplier("market"), None);
        assert_eq!(cadence_unit_multiplier("high"), None);
        assert_eq!(cadence_unit_multiplier("hold"), None);
    }
}
