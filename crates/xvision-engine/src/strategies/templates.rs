//! Example scenario ids and constructors for the `xvn example seed` flow.
//!
//! Produces curated `Scenario` artifacts the CLI drops into a fresh
//! `XVN_HOME` so the Driver.js tour and in-app docs have concrete market
//! windows to point at.
//!
//! Identification rules — keep operator data safe:
//!
//! * Legacy example strategies used [`EXAMPLE_ID_PREFIX`] on the manifest id
//!   and [`EXAMPLE_STRATEGY_CREATOR`] on the manifest creator. Both must match
//!   for [`is_example_strategy`] to return true; the seed only removes rows it
//!   positively identifies as legacy examples.
//! * Scenarios use [`EXAMPLE_ID_PREFIX`] on the scenario id and carry the
//!   [`EXAMPLE_SOURCE_TAG`] in `tags`. The `ScenarioSource` enum stays
//!   `Generated` — adding a new enum variant would change the DB column
//!   mapping and would not be worth a migration for a labeling concern.

use chrono::{TimeZone, Utc};
use xvision_core::Capital;
use xvision_data::alpaca::BarGranularity;

use crate::eval::bars::compute_scenario_cache_key;
use crate::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, CalendarRef, DataSource, Fees, FillModel, LatencyModel,
    LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario, ScenarioSource,
    SlippageModel, TimeWindow, Venue, VenueSettings, DEFAULT_WARMUP_BARS,
};
use crate::safety::VenueLabel;
use crate::strategies::Strategy;

/// Stable id prefix used by both example strategies and example scenarios.
pub const EXAMPLE_ID_PREFIX: &str = "example-";

/// Manifest `creator` value on every example strategy. Combined with
/// [`EXAMPLE_ID_PREFIX`] it identifies seed-owned rows.
pub const EXAMPLE_STRATEGY_CREATOR: &str = "@xvision-examples";

/// Tag attached to every example scenario. Lets list/filter callers
/// surface examples without depending on the id prefix.
pub const EXAMPLE_SOURCE_TAG: &str = "source:example";

/// Stable example scenario ids surfaced by [`example_scenarios`].
pub const EXAMPLE_SCENARIO_QUICKSTART_BULL: &str = "example-quickstart-btc-bull-jan-2025";
pub const EXAMPLE_SCENARIO_QUICKSTART_FLASH: &str = "example-quickstart-btc-flash-aug-2024";

/// Stable strategy id for the seeded example trend-follower strategy.
/// Used by `xvn example seed` to create the strategy and its scoped agent,
/// and by the cleanup path to identify the seed-owned row.
pub const EXAMPLE_STRATEGY_TREND_FOLLOWER_ID: &str = "example-trend-follower";

/// Return app-facing example strategies for `xvn example seed`.
/// Kept as an empty compatibility seam so older cleanup tests and callers can
/// ask for example strategies without reintroducing broken agentless rows.
pub fn example_strategies() -> Vec<Strategy> {
    Vec::new()
}

/// Build the curated example scenarios that `xvn example seed` writes.
///
/// Two short BTC/USD windows so a first-run eval finishes in seconds:
///
/// * `example-quickstart-btc-bull-jan-2025` — one-week trending window.
/// * `example-quickstart-btc-flash-aug-2024` — one-week flash-crash window.
pub fn example_scenarios() -> Vec<Scenario> {
    vec![
        build_example_scenario(
            EXAMPLE_SCENARIO_QUICKSTART_BULL,
            "Example — BTC bull week (Jan 2025)",
            "Seven days of BTC/USD inside the Q1-2025 uptrend. Short window so \
             a first-run eval finishes in seconds. Used by the in-app driver \
             tour to demonstrate a single backtest.",
            Utc.with_ymd_and_hms(2025, 1, 6, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2025, 1, 13, 0, 0, 0).unwrap(),
            &["regime:bull", "duration:short"],
        ),
        build_example_scenario(
            EXAMPLE_SCENARIO_QUICKSTART_FLASH,
            "Example — BTC flash crash (Aug 2024)",
            "Seven days spanning the August 2024 flash-crash event. \
             Companion scenario to the bull-week example so the tour can \
             show how a single strategy behaves across regimes.",
            Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 8, 8, 0, 0, 0).unwrap(),
            &["regime:event", "duration:short"],
        ),
    ]
}

fn build_example_scenario(
    id: &str,
    display_name: &str,
    description: &str,
    start: chrono::DateTime<Utc>,
    end: chrono::DateTime<Utc>,
    extra_tags: &[&str],
) -> Scenario {
    let granularity = BarGranularity::Hour1;
    let mut tags: Vec<String> = vec![EXAMPLE_SOURCE_TAG.into()];
    tags.extend(extra_tags.iter().map(|t| t.to_string()));
    Scenario {
        id: id.into(),
        parent_scenario_id: None,
        // Scenarios authored programmatically by the engine map to
        // `Generated`; the example label rides on the id prefix + tag.
        source: ScenarioSource::Generated,
        display_name: display_name.into(),
        description: description.into(),
        tags,
        notes: None,
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow { start, end },
        granularity,
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
            slippage: SlippageModel::Linear { bps: 5 },
            latency: LatencyModel {
                decision_to_fill_ms: 500,
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
        capital: Capital::default(),
        bar_cache_policy: BarCachePolicy {
            cache_key: compute_scenario_cache_key(granularity, start, end, "alpaca-historical-v1"),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        warmup_bars: DEFAULT_WARMUP_BARS,
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: Utc.with_ymd_and_hms(2026, 5, 17, 0, 0, 0).unwrap(),
        created_by: "system:examples".into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    }
}

/// Identify a strategy as seed-owned. Returns true only when both the id
/// prefix and the creator match the example labels — guards against
/// accidentally matching operator-authored rows that happen to share a
/// prefix.
pub fn is_example_strategy(s: &Strategy) -> bool {
    s.manifest.id.starts_with(EXAMPLE_ID_PREFIX) && s.manifest.creator == EXAMPLE_STRATEGY_CREATOR
}

/// Identify a scenario as seed-owned. Returns true only when both the id
/// prefix and the source tag match — guards against accidentally matching
/// operator-authored rows that happen to share a prefix or tag.
pub fn is_example_scenario(s: &Scenario) -> bool {
    s.id.starts_with(EXAMPLE_ID_PREFIX) && s.tags.iter().any(|t| t == EXAMPLE_SOURCE_TAG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_strategies_function_returns_empty_compatibility_seam() {
        // `example_strategies()` is a compatibility seam — it always returns an
        // empty Vec. The actual strategy seeding (with a properly-wired agent)
        // is driven by `xvision_cli::commands::example::seed::seed_strategies`
        // which creates the agent in the AgentStore and the strategy via
        // FilesystemStore. This function exists so callers can iterate over
        // "example strategies" without needing to know about the seeder's
        // internal mechanism.
        let strategies = example_strategies();
        assert!(
            strategies.is_empty(),
            "example_strategies() must remain an empty seam"
        );
    }

    #[test]
    fn example_strategies_have_distinct_ids() {
        let strategies = example_strategies();
        let mut ids: Vec<&str> = strategies.iter().map(|s| s.manifest.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), strategies.len(), "strategy ids must be distinct");
    }

    #[test]
    fn example_scenarios_are_labelled_and_valid() {
        let scenarios = example_scenarios();
        assert_eq!(scenarios.len(), 2, "two example scenarios");
        let ids: Vec<&str> = scenarios.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&EXAMPLE_SCENARIO_QUICKSTART_BULL));
        assert!(ids.contains(&EXAMPLE_SCENARIO_QUICKSTART_FLASH));
        for s in &scenarios {
            assert!(
                is_example_scenario(s),
                "example scenario {} must satisfy is_example_scenario",
                s.id
            );
            s.validate_v1()
                .unwrap_or_else(|e| panic!("scenario {} failed validate_v1: {e}", s.id));
            assert_eq!(s.source, ScenarioSource::Generated);
            assert!(s.tags.iter().any(|t| t == EXAMPLE_SOURCE_TAG));
        }
    }

    #[test]
    fn is_example_strategy_rejects_unrelated_id_or_creator() {
        use crate::strategies::manifest::PublicManifest;
        use crate::strategies::risk::RiskPreset;
        use crate::strategies::{ActivationMode, DecisionMode, PipelineDef};

        let make_example = || Strategy {
            manifest: PublicManifest {
                id: "example-test".into(),
                display_name: "Test".into(),
                plain_summary: "".into(),
                creator: EXAMPLE_STRATEGY_CREATOR.into(),
                template: "test".into(),
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
            decision_mode: DecisionMode::default(),
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
        };

        let mut s = make_example();
        assert!(is_example_strategy(&s));

        // Wrong creator: matched id but different author -> not an example.
        s.manifest.creator = "@operator".into();
        assert!(!is_example_strategy(&s));

        // Wrong id prefix: matched creator but renamed -> not an example.
        let mut s = make_example();
        s.manifest.id = "operator-trend".into();
        assert!(!is_example_strategy(&s));
    }

    #[test]
    fn is_example_scenario_rejects_unrelated_id_or_tag() {
        let mut s = example_scenarios().remove(0);
        assert!(is_example_scenario(&s));

        s.tags.retain(|t| t != EXAMPLE_SOURCE_TAG);
        assert!(!is_example_scenario(&s));

        let mut s = example_scenarios().remove(0);
        s.id = "user-quickstart".into();
        assert!(!is_example_scenario(&s));
    }
}
