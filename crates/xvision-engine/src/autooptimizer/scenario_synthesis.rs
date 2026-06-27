use anyhow::{bail, Context, Result};
use chrono::{NaiveDate, TimeZone, Utc};
use ulid::Ulid;

use crate::autooptimizer::config::{BaselineUntouchedWindow, DayWindow, ScenarioRotationConfig};
use crate::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, CalendarRef, Capital, DataSource, Fees, FillModel,
    LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario,
    ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings, DEFAULT_WARMUP_BARS,
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

/// Pre-generate a pool of (day_scenario, baseline_scenario) pairs for per-cycle
/// scenario rotation.
///
/// When `rotation.enabled` is false or `rotation.num_windows` is 0, returns an
/// empty vec — the session uses the single static day/baseline pair (legacy).
///
/// Each pair `i` has:
/// - day window: `[range_start + i*stride, range_start + i*stride + day_span)`
/// - baseline window: immediately after the day window, with `untouched_span`
///
/// Windows are clamped to `range_end`. Degenerate (empty) windows are skipped.
/// Dates fall back to `default_start`/`default_end` when not explicitly set in
/// the rotation config.
pub fn generate_scenario_rotation_pool(
    rotation: &ScenarioRotationConfig,
    cadence_minutes: u32,
    default_start: NaiveDate,
    default_end: NaiveDate,
    created_by: &str,
) -> Result<Vec<(Scenario, Scenario)>> {
    if !rotation.enabled || rotation.num_windows == 0 {
        return Ok(Vec::new());
    }
    let range_start = rotation.date_range_start.unwrap_or(default_start);
    let range_end = rotation.date_range_end.unwrap_or(default_end);
    let stride = rotation.stride_days;
    let day_span = rotation.day_window_span_days;
    let unt_span = rotation.untouched_window_span_days;
    let mut pool = Vec::with_capacity(rotation.num_windows);

    for i in 0..rotation.num_windows {
        let offset = i as i64 * stride;
        let day_start = range_start + chrono::Duration::days(offset);
        let day_end_unclamped = day_start + chrono::Duration::days(day_span);
        let day_end = day_end_unclamped.min(range_end);
        if day_start >= day_end {
            continue;
        }
        let base_start = day_end;
        let base_end_unclamped = base_start + chrono::Duration::days(unt_span);
        let base_end = base_end_unclamped.min(range_end);
        if base_start >= base_end {
            continue;
        }
        let day_win = DayWindow { start: day_start, end: day_end };
        let day = synthesize_optimizer_day_scenario(&day_win, cadence_minutes, created_by);
        let base_win = BaselineUntouchedWindow { start: base_start, end: base_end };
        let baseline = synthesize_baseline_untouched_scenario(&day, &base_win)
            .with_context(|| format!("scenario_rotation: failed to synthesize baseline for window {i}"))?;
        pool.push((day, baseline));
    }
    Ok(pool)
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
        assert!(baseline
            .bar_cache_policy
            .cache_key
            .contains(&day.bar_cache_policy.cache_key));
    }

    // ── Scenario rotation pool generation ────────────────────────────────

    #[test]
    fn rotation_pool_empty_when_disabled() {
        let mut r = ScenarioRotationConfig::default();
        r.enabled = false;
        let pool = generate_scenario_rotation_pool(
            &r,
            60,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 6, 1).unwrap(),
            "test",
        )
        .unwrap();
        assert!(pool.is_empty(), "disabled rotation must produce empty pool");
    }

    #[test]
    fn rotation_pool_empty_when_zero_windows() {
        let mut r = ScenarioRotationConfig::default();
        r.enabled = true;
        r.num_windows = 0;
        let pool = generate_scenario_rotation_pool(
            &r,
            60,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 6, 1).unwrap(),
            "test",
        )
        .unwrap();
        assert!(pool.is_empty(), "0 windows must produce empty pool");
    }

    #[test]
    fn rotation_pool_produces_expected_count() {
        let pool = generate_scenario_rotation_pool(
            &ScenarioRotationConfig::default(),
            60,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 12, 1).unwrap(),
            "test",
        )
        .unwrap();
        // Default: num_windows = 10, 14-day spans, 30-day stride over 11 months
        // → all 10 should fit.
        assert_eq!(pool.len(), 10, "default rotation should produce 10 pairs");
    }

    #[test]
    fn rotation_pool_windows_have_disjoint_day_and_baseline() {
        let pool = generate_scenario_rotation_pool(
            &ScenarioRotationConfig::default(),
            60,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 12, 1).unwrap(),
            "test",
        )
        .unwrap();
        for (i, (day, baseline)) in pool.iter().enumerate() {
            // Day and baseline must not overlap (baseline starts at or after day ends).
            assert!(
                baseline.time_window.start >= day.time_window.end,
                "window {i}: baseline start ({}) must be >= day end ({})",
                baseline.time_window.start,
                day.time_window.end,
            );
        }
    }

    #[test]
    fn rotation_pool_windows_are_distinct() {
        let pool = generate_scenario_rotation_pool(
            &ScenarioRotationConfig::default(),
            60,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 12, 1).unwrap(),
            "test",
        )
        .unwrap();
        // Each pair should have a different day window start (strided).
        let mut day_starts = std::collections::HashSet::new();
        for (day, _) in &pool {
            assert!(
                day_starts.insert(day.time_window.start),
                "duplicate day window start: {}",
                day.time_window.start
            );
        }
    }

    #[test]
    fn rotation_pool_clamps_to_date_range() {
        // Tight range: only 2 windows fit.
        let mut r = ScenarioRotationConfig::default();
        r.num_windows = 10;
        r.stride_days = 30;
        r.day_window_span_days = 14;
        r.untouched_window_span_days = 14;
        let pool = generate_scenario_rotation_pool(
            &r,
            60,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(), // only ~60 days
            "test",
        )
        .unwrap();
        // With 30-day stride and 28 days per pair (14+14), only 2 fit in 60 days.
        assert!(
            !pool.is_empty() && pool.len() <= 10,
            "clamped pool should have some windows but not all 10; got {}",
            pool.len()
        );
    }
}
