use anyhow::{bail, Result};
use chrono::{TimeZone, Utc};
use ulid::Ulid;

use crate::autooptimizer::config::{BaselineUntouchedWindow, DayWindow};
use crate::eval::scenario::{
    AdjustmentMode, AssetClass, BarCachePolicy, BarGranularity, CalendarRef, Capital, DataSource, Fees,
    FillModel, LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode,
    Scenario, ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings, DEFAULT_WARMUP_BARS,
};
use crate::safety::VenueLabel;

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
pub fn synthesize_optimizer_day_scenario(day_window: &DayWindow, created_by: &str) -> Scenario {
    let start = Utc.from_utc_datetime(&day_window.start.and_hms_opt(0, 0, 0).expect("valid midnight"));
    let end = Utc.from_utc_datetime(&day_window.end.and_hms_opt(0, 0, 0).expect("valid midnight"));
    Scenario {
        id: format!("ec-day-{}", Ulid::new()),
        parent_scenario_id: None,
        source: ScenarioSource::Generated,
        display_name: "Optimizer cycle day window".into(),
        description: format!("Synthesized day window {} – {}", day_window.start, day_window.end),
        tags: vec![],
        notes: None,
        asset_class: AssetClass::Crypto,
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow { start, end },
        granularity: BarGranularity::Hour1,
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
            cache_key: format!("ec-day-{}-{}", day_window.start, day_window.end),
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
