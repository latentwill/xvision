use chrono::{TimeZone, Utc};
use xvision_engine::autooptimizer::{synthesize_baseline_untouched_scenario, BaselineUntouchedWindow};
use xvision_engine::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, CalendarRef, Capital, DataSource, Fees, FillModel,
    LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode, Scenario,
    ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings,
};
use xvision_engine::safety::VenueLabel;

const OPTIMIZER_SCENARIO_TAG: &str = "source:autooptimizer";

fn d(y: i32, m: u32, day: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn dt(y: i32, m: u32, day: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, day, 0, 0, 0).unwrap()
}

fn make_day_scenario() -> Scenario {
    Scenario {
        id: "day-sc-001".into(),
        parent_scenario_id: None,
        source: ScenarioSource::User,
        display_name: "Day scenario".into(),
        description: "Test day scenario.".into(),
        tags: vec!["regime:trending_bull".into()],
        notes: Some("day notes".into()),
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: dt(2025, 1, 1),
            end: dt(2025, 4, 1),
        },
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        data_source: DataSource::AlpacaHistorical {
            feed: Some("crypto".into()),
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
                decision_to_fill_ms: 250,
            },
            fill_model: FillModel {
                market_order_fill: MarketOrderFill::NextBarOpen,
                limit_order_fill: LimitOrderFill::NeverFills,
                partial_fills: false,
                volume_constraints: None,
            },
            overrides: vec![],
            borrow_bps_per_day: 5.0,
        },
        replay_mode: ReplayMode::Continuous,
        capital: Capital::default(),
        bar_cache_policy: BarCachePolicy {
            cache_key: "scenario-bull-q1-2025".into(),
            refresh_policy: RefreshPolicy::NeverRefresh,
            data_fetched_at: None,
        },
        warmup_bars: 200,
        regime_label: Some("trend".into()),
        volatility_label: Some("low".into()),
        trend_direction: Some("up".into()),
        regime_derived: true,
        created_at: dt(2025, 1, 1),
        created_by: "test".into(),
        archived_at: None,
        venue_label: VenueLabel::Paper,
        safety_limits: None,
    }
}

#[test]
fn valid_non_overlapping_window_produces_correct_scenario() {
    let day = make_day_scenario();
    let window = BaselineUntouchedWindow {
        start: d(2024, 7, 1),
        end: d(2024, 10, 1),
    };
    let result = synthesize_baseline_untouched_scenario(&day, &window).unwrap();

    assert_ne!(result.id, day.id, "synthesized id must differ from parent");
    assert_eq!(result.parent_scenario_id, Some(day.id.clone()));
    assert_eq!(result.time_window.start, dt(2024, 7, 1));
    assert_eq!(result.time_window.end, dt(2024, 10, 1));
    assert!(matches!(result.source, ScenarioSource::Generated));
    assert!(result.tags.iter().any(|t| t == OPTIMIZER_SCENARIO_TAG));
    assert_eq!(result.bar_cache_policy.data_fetched_at, None);
    assert!(matches!(
        result.bar_cache_policy.refresh_policy,
        RefreshPolicy::NeverRefresh
    ));
}

#[test]
fn preserves_data_source_venue_and_feed() {
    let day = make_day_scenario();
    let window = BaselineUntouchedWindow {
        start: d(2024, 7, 1),
        end: d(2024, 10, 1),
    };
    let result = synthesize_baseline_untouched_scenario(&day, &window).unwrap();

    assert_eq!(result.data_source, day.data_source);
    assert_eq!(result.venue, day.venue);
    assert_eq!(result.asset_class, day.asset_class);
    assert!(result.tags.iter().any(|t| t == "regime:trending_bull"));
    assert!(result.tags.iter().any(|t| t == OPTIMIZER_SCENARIO_TAG));
    assert_eq!(result.regime_label, None);
    assert_eq!(result.volatility_label, None);
    assert_eq!(result.trend_direction, None);
    assert!(!result.regime_derived);
    assert_eq!(result.notes, None);
    assert_eq!(result.archived_at, None);
}

#[test]
fn overlapping_window_returns_operator_vocabulary_error() {
    let day = make_day_scenario();
    let window = BaselineUntouchedWindow {
        start: d(2025, 2, 1),
        end: d(2025, 5, 1),
    };
    let err = synthesize_baseline_untouched_scenario(&day, &window)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("baseline window overlaps day window"),
        "expected 'baseline window overlaps day window' in error, got: {err}"
    );
}

#[test]
fn empty_window_returns_error() {
    let day = make_day_scenario();
    let ts = d(2024, 6, 1);
    let window = BaselineUntouchedWindow { start: ts, end: ts };
    let err = synthesize_baseline_untouched_scenario(&day, &window)
        .unwrap_err()
        .to_string();
    assert!(err.contains("empty"), "expected 'empty' in error, got: {err}");
}

#[test]
fn valid_window_after_day_is_accepted() {
    let day = make_day_scenario();
    // day window: [2025-01-01, 2025-04-01]; holdout window is entirely after it
    let window = BaselineUntouchedWindow {
        start: d(2025, 6, 1),
        end: d(2025, 9, 1),
    };
    let result = synthesize_baseline_untouched_scenario(&day, &window).unwrap();
    assert_eq!(result.time_window.start, dt(2025, 6, 1));
    assert_eq!(result.time_window.end, dt(2025, 9, 1));
    assert_eq!(result.parent_scenario_id, Some(day.id.clone()));
}

#[test]
fn window_fully_containing_day_is_overlap_error() {
    let day = make_day_scenario();
    // window spans before and after the day window — should still overlap
    let window = BaselineUntouchedWindow {
        start: d(2024, 1, 1),
        end: d(2026, 1, 1),
    };
    let err = synthesize_baseline_untouched_scenario(&day, &window)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("baseline window overlaps day window"),
        "expected overlap error, got: {err}"
    );
}
