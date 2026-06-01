use anyhow::{bail, Result};
use chrono::{TimeZone, Utc};
use ulid::Ulid;

use crate::autooptimizer::config::BaselineUntouchedWindow;
use crate::eval::scenario::{BarCachePolicy, RefreshPolicy, Scenario, ScenarioSource, TimeWindow};

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
