//! Per-filter mutable runtime state.
//!
//! Holds everything the evaluator needs to remember across bars:
//!
//! * The [`IndicatorEngine`](crate::indicators::IndicatorEngine) feeding
//!   indicator values.
//! * The previous-bar boolean results of each condition leaf (so
//!   `crosses_above` / `crosses_below` can detect transitions).
//! * The previous bar's tree result, so the evaluator can detect a
//!   `false → true` trip vs. a sustained `true`.
//! * Cooldown countdown in bars (set when the filter trips active).
//! * Daily wakeup counter, with the day the count belongs to (UTC).
//! * Whether the filter has ever fired at all.

use std::collections::{BTreeMap, VecDeque};

use chrono::{DateTime, Datelike, Utc};

use crate::indicators::IndicatorEngine;
use crate::types::{Condition, ConditionTree, Filter, IndicatorRef, Operand};

pub(crate) const CONDITION_HISTORY_CAP: usize = 512;

/// Internal record of one UTC day for the daily-wakeup cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UtcDay {
    year: i32,
    ordinal: u32, // 1..=366
}

impl UtcDay {
    fn from_ts(ts: DateTime<Utc>) -> Self {
        Self {
            year: ts.year(),
            ordinal: ts.ordinal(),
        }
    }
}

/// Per-filter runtime state.
#[derive(Debug)]
pub struct FilterState {
    pub(crate) indicators: IndicatorEngine,
    /// Per-condition previous boolean result (index aligns with the
    /// flat condition list returned by [`ConditionTree::conditions`]).
    /// `None` until the first bar after warmup.
    pub(crate) prev_conditions: Vec<Option<bool>>,
    /// Previous resolved numeric pair for each condition. This is what
    /// `crosses_*` needs: previous `lhs <= rhs` / `lhs >= rhs` plus the
    /// current strict comparison.
    pub(crate) prev_numeric_pairs: Vec<Option<(f64, f64)>>,
    /// Recent raw per-condition operator signals. Parameterized
    /// operators such as `above_for_3` and `crossed_above_5` use this
    /// to express persistence without expanding the condition tree.
    pub(crate) condition_history: Vec<VecDeque<bool>>,
    /// Recent resolved numeric pairs for each condition, newest at the
    /// back. Used by slope/z-score operator transforms.
    pub(crate) numeric_pair_history: Vec<VecDeque<(f64, f64)>>,
    /// Previous tree result (the rollup of the per-condition results
    /// under All/Any). `None` until after the first post-warmup bar.
    pub(crate) prev_tree: Option<bool>,
    /// Bars remaining until cooldown expires. `0` means inactive.
    pub(crate) cooldown_left: u32,
    /// Day the wakeup counter belongs to.
    pub(crate) wakeup_day: Option<UtcDay>,
    /// Number of times the filter has tripped today.
    pub(crate) wakeups_today: u32,
}

impl FilterState {
    /// Build a state for the given filter. The indicator engine is
    /// pre-seeded with every `IndicatorRef` mentioned in the filter's
    /// conditions.
    pub fn new(filter: &Filter) -> Self {
        let refs = collect_filter_indicator_refs(filter);
        let indicators = IndicatorEngine::new(refs.iter());
        let n_conditions = filter.conditions.conditions().len();
        Self {
            indicators,
            prev_conditions: vec![None; n_conditions],
            prev_numeric_pairs: vec![None; n_conditions],
            condition_history: (0..n_conditions)
                .map(|_| VecDeque::with_capacity(CONDITION_HISTORY_CAP.min(64)))
                .collect(),
            numeric_pair_history: (0..n_conditions)
                .map(|_| VecDeque::with_capacity(CONDITION_HISTORY_CAP.min(64)))
                .collect(),
            prev_tree: None,
            cooldown_left: 0,
            wakeup_day: None,
            wakeups_today: 0,
        }
    }

    /// Bars of warmup the indicator engine still needs.
    pub fn warmup_bars_left(&self) -> u32 {
        let needed = self.indicators.warmup_bars();
        let seen = self.indicators.bars_seen() as u32;
        needed.saturating_sub(seen)
    }

    /// True once the indicator engine has warmed up AND one more bar
    /// has been observed (so each condition has a `t-1` reference).
    pub fn is_warm(&self) -> bool {
        self.warmup_bars_left() == 0
    }

    /// Bars of cooldown remaining.
    pub fn cooldown_left(&self) -> u32 {
        self.cooldown_left
    }

    /// Wakeups recorded for `day`. Returns 0 if a different day's
    /// counter is currently held.
    pub fn wakeups_on(&self, day: DateTime<Utc>) -> u32 {
        let d = UtcDay::from_ts(day);
        match self.wakeup_day {
            Some(held) if held == d => self.wakeups_today,
            _ => 0,
        }
    }

    /// Current sparse indicator values for every indicator referenced
    /// by `filter`, keyed by the stable DSL string (`ema_20`, `close`,
    /// etc.). Indicators still in warmup are omitted.
    pub fn indicator_snapshot(&self, filter: &Filter) -> BTreeMap<String, f64> {
        collect_filter_indicator_refs(filter)
            .into_iter()
            .filter_map(|r| self.indicators.value(&r).map(|v| (r.to_string(), v)))
            .collect()
    }

    /// Update the wakeup counter for `day`, rolling over if the day
    /// changed since the last recorded wakeup.
    pub(crate) fn note_wakeup(&mut self, day: DateTime<Utc>) {
        let d = UtcDay::from_ts(day);
        match self.wakeup_day {
            Some(held) if held == d => {
                self.wakeups_today += 1;
            }
            _ => {
                self.wakeup_day = Some(d);
                self.wakeups_today = 1;
            }
        }
    }

    /// Begin cooldown for `n` bars.
    pub(crate) fn arm_cooldown(&mut self, n: u32) {
        self.cooldown_left = n;
    }

    /// Decrement cooldown by one bar (saturating at 0).
    pub(crate) fn tick_cooldown(&mut self) {
        if self.cooldown_left > 0 {
            self.cooldown_left -= 1;
        }
    }
}

/// Walk a condition tree and collect every `IndicatorRef` it touches.
pub fn collect_indicator_refs(tree: &ConditionTree) -> Vec<IndicatorRef> {
    let mut out = Vec::new();
    for cond in tree.conditions() {
        collect_from_condition(cond, &mut out);
    }
    // Dedup while preserving order.
    let mut seen: Vec<IndicatorRef> = Vec::new();
    for r in out {
        if !seen.iter().any(|s| s == &r) {
            seen.push(r);
        }
    }
    seen
}

/// Collect every indicator a filter needs at runtime: condition operands
/// plus optional fire-context indicators.
pub fn collect_filter_indicator_refs(filter: &Filter) -> Vec<IndicatorRef> {
    let mut out = collect_indicator_refs(&filter.conditions);
    if let Some(fire) = &filter.fire {
        out.extend(fire.context.iter().cloned());
    }
    let mut seen: Vec<IndicatorRef> = Vec::new();
    for r in out {
        if !seen.iter().any(|s| s == &r) {
            seen.push(r);
        }
    }
    seen
}

fn collect_from_condition(c: &Condition, out: &mut Vec<IndicatorRef>) {
    collect_from_operand(&c.lhs, out);
    collect_from_operand(&c.rhs, out);
}

fn collect_from_operand(o: &Operand, out: &mut Vec<IndicatorRef>) {
    if let Operand::Indicator(r) = o {
        out.push(r.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Condition, ConditionTree, FilterId, IndicatorName, Operator, StrategyId, Symbol, Timeframe,
    };
    use chrono::TimeZone;

    fn mk_filter(tree: ConditionTree, cooldown: u32, max_wake: Option<u32>) -> Filter {
        Filter {
            id: FilterId::new("01H".to_string()),
            strategy_id: StrategyId::new("01S".to_string()),
            display_name: "t".into(),
            description: None,
            status: crate::types::FilterStatus::Draft,
            asset_scope: vec![Symbol::new("BTC/USD")],
            timeframe: Timeframe::new("1h"),
            scan_cadence: crate::types::ScanCadence::BarClose,
            conditions: tree,
            fire: None,
            cooldown_bars: cooldown,
            max_wakeups_per_day: max_wake,
            wake_when_in_position: crate::types::WakeInPosition::Always,
            agent_context_template: crate::types::AgentContextTemplateId::new(
                crate::DEFAULT_AGENT_CONTEXT_TEMPLATE,
            ),
        }
    }

    fn one_indicator_filter() -> Filter {
        let tree = ConditionTree::All(vec![Condition {
            lhs: Operand::Indicator(IndicatorRef::periodic(IndicatorName::Sma, 3)),
            op: Operator::Gt,
            rhs: Operand::Numeric(0.0),
        }]);
        mk_filter(tree, 0, None)
    }

    #[test]
    fn warmup_matches_max_period() {
        let f = one_indicator_filter();
        let s = FilterState::new(&f);
        // SMA 3 needs 3 bars before producing a value.
        assert_eq!(s.warmup_bars_left(), 3);
    }

    #[test]
    fn cooldown_arm_and_tick() {
        let f = one_indicator_filter();
        let mut s = FilterState::new(&f);
        s.arm_cooldown(3);
        assert_eq!(s.cooldown_left(), 3);
        s.tick_cooldown();
        s.tick_cooldown();
        s.tick_cooldown();
        s.tick_cooldown();
        assert_eq!(s.cooldown_left(), 0);
    }

    #[test]
    fn wakeup_rollover() {
        let f = one_indicator_filter();
        let mut s = FilterState::new(&f);
        let day1 = Utc.with_ymd_and_hms(2026, 5, 21, 10, 0, 0).unwrap();
        let day2 = Utc.with_ymd_and_hms(2026, 5, 22, 0, 0, 0).unwrap();
        s.note_wakeup(day1);
        s.note_wakeup(day1);
        assert_eq!(s.wakeups_on(day1), 2);
        s.note_wakeup(day2);
        assert_eq!(s.wakeups_on(day2), 1);
        assert_eq!(s.wakeups_on(day1), 0);
    }

    #[test]
    fn collect_indicator_refs_dedups() {
        let r = IndicatorRef::periodic(IndicatorName::Ema, 10);
        let tree = ConditionTree::All(vec![
            Condition {
                lhs: Operand::Indicator(r.clone()),
                op: Operator::Gt,
                rhs: Operand::Numeric(0.0),
            },
            Condition {
                lhs: Operand::Indicator(r.clone()),
                op: Operator::Lt,
                rhs: Operand::Numeric(100.0),
            },
        ]);
        let refs = collect_indicator_refs(&tree);
        assert_eq!(refs.len(), 1);
    }
}
