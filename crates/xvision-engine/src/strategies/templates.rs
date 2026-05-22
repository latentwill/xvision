//! Example template ids and constructors for the `xvn example seed` flow.
//!
//! Produces a curated set of `Strategy` and `Scenario` artifacts the CLI
//! drops into a fresh `XVN_HOME` so the Driver.js tour and in-app docs
//! (V2A items 1 and 2) have something concrete to point at.
//!
//! Identification rules — keep operator data safe:
//!
//! * Strategies use [`EXAMPLE_ID_PREFIX`] on the manifest id and
//!   [`EXAMPLE_STRATEGY_CREATOR`] on the manifest creator. Both must match
//!   for [`is_example_strategy`] to return true; the seed only reads or
//!   replaces rows it positively identifies as examples.
//! * Scenarios use [`EXAMPLE_ID_PREFIX`] on the scenario id and carry the
//!   [`EXAMPLE_SOURCE_TAG`] in `tags`. The `ScenarioSource` enum stays
//!   `Generated` — adding a new enum variant would change the DB column
//!   mapping and would not be worth a migration for a labeling concern.

use chrono::{TimeZone, Utc};
use xvision_core::Capital;
use xvision_data::alpaca::BarGranularity;

use crate::eval::bars::compute_cache_key;
use crate::eval::scenario::{
    AdjustmentMode, AssetClass, AssetRef, BarCachePolicy, CalendarRef, DataSource, Fees, FillModel,
    LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario,
    ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings, DEFAULT_WARMUP_BARS,
};
use crate::safety::VenueLabel;
use crate::strategies::manifest::{PublicManifest, RegimeFit};
use crate::strategies::risk::RiskPreset;
use crate::strategies::slot::LLMSlot;
use crate::strategies::{PipelineDef, Strategy};

/// Stable id prefix used by both example strategies and example scenarios.
pub const EXAMPLE_ID_PREFIX: &str = "example-";

/// Manifest `creator` value on every example strategy. Combined with
/// [`EXAMPLE_ID_PREFIX`] it identifies seed-owned rows.
pub const EXAMPLE_STRATEGY_CREATOR: &str = "@xvision-examples";

/// Tag attached to every example scenario. Lets list/filter callers
/// surface examples without depending on the id prefix.
pub const EXAMPLE_SOURCE_TAG: &str = "source:example";

const EXAMPLE_TREND_FOLLOWER_PROMPT: &str = r#"You are a trend-following crypto trader. Inputs:
- ohlcv_history: last 200 bars
- indicator_panel: EMA(12), EMA(26), EMA(50), ATR(14)
- portfolio_state: open positions, available capital

Decide ONE of: long_open | short_open | flat | hold.
Trend logic:
  enter long  when EMA(12) > EMA(26) > EMA(50) AND price > EMA(12);
  enter short when EMA(12) < EMA(26) < EMA(50) AND price < EMA(12);
  otherwise flat or hold.
Output JSON: {action, conviction (0-1), justification (one line)}.
"#;

const EXAMPLE_MEAN_REVERSION_TRADER_PROMPT: &str = r#"You are a mean-reversion crypto trader. Inputs:
- ohlcv_history: last 200 bars
- indicator_panel: RSI(14), Bollinger(20, 2), ATR(14)
- portfolio_state: open positions, available capital

Decide ONE of: long_open | short_open | flat | hold.
Mean-reversion logic: enter long when RSI < 30 AND price < lower_bollinger;
enter short when RSI > 70 AND price > upper_bollinger; otherwise flat or hold.
Output JSON: {action, conviction (0-1), justification (one line)}.
"#;

const EXAMPLE_MEAN_REVERSION_REGIME_PROMPT: &str = r#"Classify the current crypto market regime as one of:
trending_bull | trending_bear | range_bound | chop.
Use indicator_panel + recent ohlcv_history. Return JSON: {regime, confidence (0-1)}.
"#;

const EXAMPLE_BREAKOUT_PROMPT: &str = r#"You are a breakout crypto trader. Inputs:
- ohlcv_history: last 200 bars
- indicator_panel: Donchian(20), ATR(14)
- portfolio_state: open positions, available capital

Decide ONE of: long_open | short_open | flat | hold.
Breakout logic:
  enter long  when close > donchian_upper(20);
  enter short when close < donchian_lower(20);
  otherwise flat or hold (avoid trading inside the channel).
Output JSON: {action, conviction (0-1), justification (one line)}.
"#;

/// Stable example strategy ids surfaced by [`example_strategies`]. Exported
/// so the Driver.js tour (V2A item 1) and CLI tests can reference them
/// without hardcoding the strings.
pub const EXAMPLE_STRATEGY_TREND_FOLLOWER: &str = "example-trend-follower";
pub const EXAMPLE_STRATEGY_MEAN_REVERSION: &str = "example-mean-reversion";
pub const EXAMPLE_STRATEGY_BREAKOUT: &str = "example-breakout";

/// Stable example scenario ids surfaced by [`example_scenarios`].
pub const EXAMPLE_SCENARIO_QUICKSTART_BULL: &str = "example-quickstart-btc-bull-jan-2025";
pub const EXAMPLE_SCENARIO_QUICKSTART_FLASH: &str = "example-quickstart-btc-flash-aug-2024";

/// Build the curated example strategies that `xvn example seed` writes.
///
/// Three strategies are returned, covering the three primary regime
/// behaviors the driver tour highlights:
///
/// * `example-trend-follower` — single-slot trend follower (the 80% case)
/// * `example-mean-reversion` — regime classifier + mean-reversion trader
/// * `example-breakout` — single-slot Donchian breakout
pub fn example_strategies() -> Vec<Strategy> {
    vec![
        Strategy {
            manifest: PublicManifest {
                id: EXAMPLE_STRATEGY_TREND_FOLLOWER.into(),
                display_name: "Example — Trend follower".into(),
                plain_summary: "Trades with the dominant trend on BTC/ETH. \
                     A worked example of the trend_follower template — \
                     swap the prompt or model to make it your own."
                    .into(),
                creator: EXAMPLE_STRATEGY_CREATOR.into(),
                template: "trend_follower".into(),
                regime_fit: vec![RegimeFit::TrendingBull, RegimeFit::TrendingBear],
                asset_universe: vec!["BTC/USD".into(), "ETH/USD".into()],
                decision_cadence_minutes: 60,
                attested_with: vec!["anthropic.claude-sonnet-4.6".into()],
                required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
            },
            hypothesis: None,
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: Some(LLMSlot {
                role: "trader".into(),
                prompt: EXAMPLE_TREND_FOLLOWER_PROMPT.into(),
                attested_with: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                provider: None,
                model: None,
            }),
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({
                "ema_fast": 12,
                "ema_mid": 26,
                "ema_slow": 50,
                "atr_period": 14
            }),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
        },
        Strategy {
            manifest: PublicManifest {
                id: EXAMPLE_STRATEGY_MEAN_REVERSION.into(),
                display_name: "Example — Mean reversion".into(),
                plain_summary: "Two-stage pipeline: a regime classifier filters \
                     the briefing, then the trader buys oversold dips and sells \
                     overbought rallies on ETH/USD."
                    .into(),
                creator: EXAMPLE_STRATEGY_CREATOR.into(),
                template: "mean_reversion".into(),
                regime_fit: vec![RegimeFit::RangeBound, RegimeFit::LowVol],
                asset_universe: vec!["ETH/USD".into()],
                decision_cadence_minutes: 60,
                attested_with: vec!["anthropic.claude-sonnet-4.6".into()],
                required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
            },
            hypothesis: None,
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: Some(LLMSlot {
                role: "regime".into(),
                prompt: EXAMPLE_MEAN_REVERSION_REGIME_PROMPT.into(),
                attested_with: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["indicator_panel".into()],
                provider: None,
                model: None,
            }),
            intern_slot: None,
            trader_slot: Some(LLMSlot {
                role: "trader".into(),
                prompt: EXAMPLE_MEAN_REVERSION_TRADER_PROMPT.into(),
                attested_with: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                provider: None,
                model: None,
            }),
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({
                "rsi_oversold": 30,
                "rsi_overbought": 70,
                "bollinger_period": 20,
                "bollinger_sigma": 2.0,
                "atr_period": 14
            }),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
        },
        Strategy {
            manifest: PublicManifest {
                id: EXAMPLE_STRATEGY_BREAKOUT.into(),
                display_name: "Example — Breakout".into(),
                plain_summary: "Donchian channel breakout trader. Long the \
                     upper break, short the lower break, sit out the middle. \
                     A worked example of the breakout template."
                    .into(),
                creator: EXAMPLE_STRATEGY_CREATOR.into(),
                template: "breakout".into(),
                regime_fit: vec![RegimeFit::TrendingBull, RegimeFit::HighVol],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 60,
                attested_with: vec!["anthropic.claude-sonnet-4.6".into()],
                required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                risk_preset_or_config: "conservative".into(),
                published_at: None,
                min_warmup_bars: None,
            },
            hypothesis: None,
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: Some(LLMSlot {
                role: "trader".into(),
                prompt: EXAMPLE_BREAKOUT_PROMPT.into(),
                attested_with: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                provider: None,
                model: None,
            }),
            risk: RiskPreset::Conservative.expand(),
            mechanical_params: serde_json::json!({
                "donchian_period": 20,
                "atr_period": 14
            }),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
        },
    ]
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
    let asset = AssetRef {
        class: AssetClass::Crypto,
        symbol: "BTC".into(),
        venue_symbol: "BTC/USD".into(),
    };
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
        asset: vec![asset.clone()],
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
        },
        replay_mode: ReplayMode::Continuous,
        capital: Capital::default(),
        bar_cache_policy: BarCachePolicy {
            cache_key: compute_cache_key(
                &asset.venue_symbol,
                granularity,
                start,
                end,
                "alpaca-historical-v1",
            ),
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
    use crate::strategies::validate::validate_strategy;

    #[test]
    fn example_strategies_are_labelled_and_valid() {
        let strategies = example_strategies();
        assert_eq!(strategies.len(), 3, "three example strategies");
        let ids: Vec<&str> = strategies.iter().map(|s| s.manifest.id.as_str()).collect();
        assert!(ids.contains(&EXAMPLE_STRATEGY_TREND_FOLLOWER));
        assert!(ids.contains(&EXAMPLE_STRATEGY_MEAN_REVERSION));
        assert!(ids.contains(&EXAMPLE_STRATEGY_BREAKOUT));
        for s in &strategies {
            assert!(
                is_example_strategy(s),
                "example {} must satisfy is_example_strategy",
                s.manifest.id
            );
            validate_strategy(s).unwrap_or_else(|e| {
                panic!("example strategy {} failed validate_strategy: {e}", s.manifest.id)
            });
        }
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
        let mut s = example_strategies().remove(0);
        assert!(is_example_strategy(&s));

        // Wrong creator: matched id but different author -> not an example.
        s.manifest.creator = "@operator".into();
        assert!(!is_example_strategy(&s));

        // Wrong id prefix: matched creator but renamed -> not an example.
        let mut s = example_strategies().remove(0);
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
