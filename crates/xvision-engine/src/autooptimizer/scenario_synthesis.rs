use anyhow::{bail, Result};
use chrono::{TimeZone, Utc};
use ulid::Ulid;

use crate::autooptimizer::config::{BaselineUntouchedWindow, DayWindow};
use crate::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, CalendarRef, Capital, DataSource, Fees, FillModel, LatencyModel,
    LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario, ScenarioSource,
    SlippageModel, TimeWindow, Venue, VenueSettings, DEFAULT_WARMUP_BARS,
};
use crate::safety::VenueLabel;
use crate::strategies::bar_granularity_for_cadence;

pub const OPTIMIZER_SCENARIO_TAG: &str = "source:autooptimizer";

/// F10 (2026-06-04): the single builder for an autooptimizer "day window"
/// scenario. Both entry points that synthesize an optimizer scenario — the CLI
/// `xvn optimizer run-cycle` and the dashboard `POST /api/autooptimizer/run-cycle`
/// — were hand-rolling an identical `Scenario` literal (BTC/USD 1h, Alpaca
/// fees maker 10 / taker 25, full-at-close fills, `Capital::default()`). Two
/// copies meant a fee or fill-model tweak in one would silently diverge the
/// optimizer's scoring conditions from the other. They now call this; the
/// only per-caller variation is `created_by`.
///
/// `synthesize_baseline_untouched_scenario` derives the holdout window from
/// the scenario this returns, so the two windows always share venue settings.
pub fn synthesize_optimizer_day_scenario(
    day_window: &DayWindow,
    cadence_minutes: u32,
    created_by: &str,
) -> Scenario {
    let start = Utc.from_utc_datetime(&day_window.start.and_hms_opt(0, 0, 0).expect("valid midnight"));
    let end = Utc.from_utc_datetime(&day_window.end.and_hms_opt(0, 0, 0).expect("valid midnight"));
    let granularity = bar_granularity_for_cadence(cadence_minutes);
    Scenario {
        id: format!("ec-day-{}", Ulid::new()),
        parent_scenario_id: None,
        source: ScenarioSource::Generated,
        display_name: "Optimizer cycle day window".into(),
        description: format!("Synthesized day window {} – {}", day_window.start, day_window.end),
        tags: vec![OPTIMIZER_SCENARIO_TAG.into()],
        notes: None,
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow { start, end },
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
                decision_to_fill_ms: 250,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::FullAtClose,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
            borrow_bps_per_day: 5.0,
            overrides: vec![],
        },
        replay_mode: ReplayMode::Continuous,
        capital: Capital::default(),
        bar_cache_policy: BarCachePolicy {
            // B9: encode granularity in the cache key so 1h-cached bars are never
            // served for a 15m (or other) cadence strategy and vice-versa.
            cache_key: format!(
                "ec-day-{}-{}-{}",
                day_window.start,
                day_window.end,
                granularity.canonical()
            ),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        warmup_bars: DEFAULT_WARMUP_BARS,
        regime_label: None,
        volatility_label: None,
        trend_direction: None,
        regime_derived: false,
        created_at: Utc::now(),
        created_by: created_by.into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    }
}

pub fn synthesize_baseline_untouched_scenario(
    day_scenario: &Scenario,
    baseline_untouched_window: &BaselineUntouchedWindow,
) -> Result<Scenario> {
    let win_start = Utc.from_utc_datetime(&baseline_untouched_window.start.and_hms_opt(0, 0, 0).unwrap());
    let win_end = Utc.from_utc_datetime(&baseline_untouched_window.end.and_hms_opt(0, 0, 0).unwrap());

    if win_start >= win_end {
        bail!("baseline-untouched window is empty");
    }

    let day_start = day_scenario.time_window.start;
    let day_end = day_scenario.time_window.end;

    if win_start < day_end && win_end > day_start {
        bail!("baseline window overlaps day window");
    }

    let new_id = Ulid::new().to_string();
    let cache_key = format!(
        "holdout-{}-{}-{}",
        day_scenario.bar_cache_policy.cache_key,
        win_start.format("%Y%m%d"),
        win_end.format("%Y%m%d"),
    );

    let mut synthesized = day_scenario.clone();
    synthesized.id = new_id;
    synthesized.parent_scenario_id = Some(day_scenario.id.clone());
    synthesized.source = ScenarioSource::Generated;
    synthesized.display_name = format!("{} (baseline untouched)", day_scenario.display_name);
    synthesized.description = format!(
        "Baseline-untouched window synthesized from \"{}\".",
        day_scenario.display_name
    );
    synthesized.notes = None;
    push_optimizer_scenario_tag(&mut synthesized.tags);
    synthesized.time_window = TimeWindow {
        start: win_start,
        end: win_end,
    };
    synthesized.bar_cache_policy = BarCachePolicy {
        cache_key,
        refresh_policy: RefreshPolicy::NeverRefresh,
        data_fetched_at: None,
    };
    synthesized.regime_label = None;
    synthesized.volatility_label = None;
    synthesized.trend_direction = None;
    synthesized.regime_derived = false;
    synthesized.archived_at = None;

    Ok(synthesized)
}

/// Backward-compatible optimizer helper for callers/tests that still import
/// the old name. The canonical mapping lives with strategies because strategy
/// cadence, not scenario shape, owns timeframe.
pub fn granularity_for_cadence(cadence_minutes: u32) -> crate::eval::scenario::BarGranularity {
    bar_granularity_for_cadence(cadence_minutes)
}

fn push_optimizer_scenario_tag(tags: &mut Vec<String>) {
    if !tags.iter().any(|t| t == OPTIMIZER_SCENARIO_TAG) {
        tags.push(OPTIMIZER_SCENARIO_TAG.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use xvision_data::alpaca::BarGranularity;

    fn day_window() -> DayWindow {
        DayWindow {
            start: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2025, 4, 1).unwrap(),
        }
    }

    fn baseline_window() -> BaselineUntouchedWindow {
        BaselineUntouchedWindow {
            start: NaiveDate::from_ymd_opt(2025, 4, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2025, 5, 1).unwrap(),
        }
    }

    #[test]
    fn granularity_for_cadence_maps_sub_hour_to_minutes() {
        assert_eq!(granularity_for_cadence(15), BarGranularity::Minute15);
        assert_eq!(granularity_for_cadence(5), BarGranularity::Minute5);
        assert_eq!(granularity_for_cadence(1), BarGranularity::Minute1);
    }

    #[test]
    fn granularity_for_cadence_maps_60_to_hour1() {
        assert_eq!(granularity_for_cadence(60), BarGranularity::Hour1);
    }

    #[test]
    fn granularity_for_cadence_maps_hour_multiples() {
        use xvision_data::alpaca::BarGranularityUnit;
        assert_eq!(
            granularity_for_cadence(120),
            BarGranularity::new(2, BarGranularityUnit::Hour).unwrap()
        );
        assert_eq!(granularity_for_cadence(240), BarGranularity::Hour4);
    }

    #[test]
    fn granularity_for_cadence_falls_back_to_hour1() {
        // 0 is degenerate; 90 is not a clean hour multiple and >59 so not minutes.
        assert_eq!(granularity_for_cadence(0), BarGranularity::Hour1);
        assert_eq!(granularity_for_cadence(90), BarGranularity::Hour1);
    }

    #[test]
    fn day_scenario_uses_cadence_granularity_15m() {
        let s = synthesize_optimizer_day_scenario(&day_window(), 15, "test");
        assert!(
            s.bar_cache_policy.cache_key.contains("15m"),
            "cache_key must encode granularity, got {}",
            s.bar_cache_policy.cache_key
        );
    }

    #[test]
    fn day_scenario_uses_cadence_granularity_60m() {
        assert_eq!(granularity_for_cadence(60), BarGranularity::Hour1);
    }

    #[test]
    fn day_scenario_uses_cadence_granularity_240m() {
        assert_eq!(granularity_for_cadence(240), BarGranularity::Hour4);
    }

    #[test]
    fn baseline_inherits_cadence_cache_key() {
        let day = synthesize_optimizer_day_scenario(&day_window(), 15, "test");
        let baseline = synthesize_baseline_untouched_scenario(&day, &baseline_window()).unwrap();
        assert!(baseline.bar_cache_policy.cache_key.contains(&day.bar_cache_policy.cache_key));
    }
}
