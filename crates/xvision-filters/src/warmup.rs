//! Warmup-bar adequacy check for filter indicators.
//!
//! Some indicators require a long history window before they produce
//! meaningful values. `rvol_tod_N` (relative volume by time-of-day) is
//! the primary example: it needs N × bars_per_day same-slot bars of
//! history, which can exceed the scenario window for short scenarios.
//!
//! This module exposes a single pure function, [`check_filter_warmup`],
//! that takes a [`Filter`] and the scenario parameters and returns any
//! indicators whose required warmup exceeds the scenario's available
//! bar count. The check is non-fatal — callers should surface the
//! warnings to the user (CLI `eprintln!`, JSON output, etc.) but not
//! abort the eval.
//!
//! The function lives in `xvision-filters` so it can be exercised in
//! unit tests without a running server.

use crate::types::{Filter, IndicatorName, Operand};

/// A single warmup-adequacy warning produced by [`check_filter_warmup`].
#[derive(Debug, Clone)]
pub struct WarmupWarning {
    /// The indicator DSL token (e.g. `"rvol_tod_20"`).
    pub indicator: String,
    /// Minimum bars of history the indicator needs.
    pub required_bars: u64,
    /// Bars the scenario provides at the given cadence.
    pub available_bars: u64,
    /// Human-readable message suitable for printing to stderr or including
    /// in JSON output.
    pub message: String,
}

/// Check all filter conditions for indicators that require more warmup bars
/// than the scenario provides.
///
/// Returns a (possibly empty) list of [`WarmupWarning`]s — one per
/// indicator reference that is under-provisioned. The list may contain
/// duplicate indicator names if the same indicator appears more than once in
/// the filter (each occurrence is reported separately).
///
/// # Arguments
///
/// * `filter` — The validated filter to inspect.
/// * `cadence_minutes` — Strategy decision cadence in minutes (same as
///   `PublicManifest::decision_cadence_minutes`). Must be > 0; if 0, the
///   function returns an empty vec without panicking.
/// * `scenario_duration_minutes` — Total scenario length in minutes
///   (`scenario.time_window.end - scenario.time_window.start` in minutes).
///   Negative values are treated as 0 available bars.
pub fn check_filter_warmup(
    filter: &Filter,
    cadence_minutes: u32,
    scenario_duration_minutes: i64,
) -> Vec<WarmupWarning> {
    if cadence_minutes == 0 {
        return vec![];
    }

    let cadence = cadence_minutes as u64;
    let bars_per_day = 1440u64 / cadence;
    let available = (scenario_duration_minutes.max(0) as u64) / cadence;

    filter
        .conditions
        .leaves_dfs()
        .into_iter()
        .filter_map(|cond| {
            // Check both lhs and rhs for rvol_tod references.
            // In practice rvol_tod almost always appears on the lhs, but
            // the DSL technically allows it on either side.
            check_operand(&cond.lhs, cadence_minutes, bars_per_day, available)
                .or_else(|| check_operand(&cond.rhs, cadence_minutes, bars_per_day, available))
        })
        .collect()
}

fn check_operand(
    operand: &Operand,
    cadence_minutes: u32,
    bars_per_day: u64,
    available: u64,
) -> Option<WarmupWarning> {
    let ind = match operand {
        Operand::Indicator(i) => i,
        _ => return None,
    };

    if !matches!(ind.name, IndicatorName::RvolTod) {
        return None;
    }

    let period = ind.period.unwrap_or(20) as u64;
    let required = period * bars_per_day;

    if required > available {
        let indicator = format!("rvol_tod_{}", period);
        let message = format!(
            "indicator {indicator} requires {required} bars of same-slot history \
             ({period} sessions × {bars_per_day} bars/day at {cadence_minutes}m cadence); \
             scenario provides ~{available} bars — expect 0 decisions"
        );
        Some(WarmupWarning {
            indicator,
            required_bars: required,
            available_bars: available,
            message,
        })
    } else {
        None
    }
}
