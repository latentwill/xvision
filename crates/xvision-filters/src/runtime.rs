//! Runtime filter evaluator.
//!
//! Wraps a validated [`Filter`] with a public per-bar API
//! ([`RuntimeFilter::evaluate`]) that consumes one bar and a per-bar
//! evaluation context and returns a [`FilterEvalOutcome`] describing
//! whether the filter is active, in warmup, in cooldown, or suppressed.
//!
//! The runtime is engine-independent: it takes a thin [`Bar`] reduction
//! of OHLCV and a timestamp. The engine adapts its `Ohlcv` type at the
//! call site.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::ValidationError;
use crate::indicators::Bar;
use crate::state::FilterState;
use crate::types::{
    Condition, ConditionTree, Filter, IndicatorRef, Operand, Operator, WakeInPosition,
};
use crate::validate::validate;

/// Decision the runtime returns for a bar.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivationDecision {
    /// Indicators still warming up.
    Warming { bars_left: u32 },
    /// Condition tree evaluated `false`.
    Inactive,
    /// Condition tree evaluated `true`. `transition` distinguishes the
    /// first `false → true` flip (a "trip") from a sustained `true`.
    Active { transition: Transition },
    /// Conditions evaluated `true` but the cooldown gate suppressed it.
    Cooldown { bars_left: u32 },
    /// Daily wakeup cap reached.
    CappedForDay { wakeups_today: u32 },
    /// `wake_when_in_position` suppressed the trip while a position is
    /// open.
    SuppressedInPosition,
}

impl ActivationDecision {
    /// True only for `Active` decisions — the gate the engine consults
    /// to decide whether to invoke the agent pipeline.
    pub fn is_active(&self) -> bool {
        matches!(self, ActivationDecision::Active { .. })
    }

    /// True only for `Active { transition: Trip }` — the bar a "plan
    /// touch" was recorded.
    pub fn is_trip(&self) -> bool {
        matches!(
            self,
            ActivationDecision::Active {
                transition: Transition::Trip
            }
        )
    }

    /// Short string tag for persistence / event payloads.
    pub fn tag(&self) -> &'static str {
        match self {
            ActivationDecision::Warming { .. } => "warming",
            ActivationDecision::Inactive => "inactive",
            ActivationDecision::Active {
                transition: Transition::Trip,
            } => "trip",
            ActivationDecision::Active {
                transition: Transition::Hold,
            } => "hold",
            ActivationDecision::Cooldown { .. } => "cooldown",
            ActivationDecision::CappedForDay { .. } => "capped_for_day",
            ActivationDecision::SuppressedInPosition => "suppressed_in_position",
        }
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    /// Conditions just flipped `false → true`.
    Trip,
    /// Conditions were `true` last bar and remain `true`.
    Hold,
}

/// Boolean result of one condition leaf evaluated against a bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConditionResult {
    pub passed: bool,
}

/// Outcome of a single per-bar evaluation.
#[derive(Debug, Clone)]
pub struct FilterEvalOutcome {
    pub decision: ActivationDecision,
    /// Per-condition boolean (index aligns with `conditions.conditions()`).
    /// Empty during warmup.
    pub conditions_passed: Vec<ConditionResult>,
    /// True iff the tree itself evaluated to true on this bar (ignoring
    /// cooldown / cap / position suppression). Useful for diagnostics.
    pub tree_true: bool,
}

/// Per-bar context the engine supplies to the runtime.
#[derive(Debug, Clone, Copy)]
pub struct EvalContext {
    /// Bar timestamp (UTC). Used for the daily wakeup-cap rollover.
    pub ts: DateTime<Utc>,
    /// Whether the strategy currently holds a position. Drives
    /// `WakeInPosition` suppression.
    pub in_position: bool,
}

/// Runtime view of a validated filter.
///
/// Construct once per run via [`RuntimeFilter::new`]; the runtime borrows
/// the original `Filter` to avoid cloning the condition tree. The mutable
/// [`FilterState`] is held separately so the runtime itself is `&self`
/// during evaluation.
#[derive(Debug)]
pub struct RuntimeFilter<'f> {
    filter: &'f Filter,
}

impl<'f> RuntimeFilter<'f> {
    /// Build a runtime view of a filter. Returns an error if the filter
    /// does not pass [`crate::validate`].
    pub fn new(filter: &'f Filter) -> Result<Self, ValidationError> {
        validate(filter)?;
        Ok(Self { filter })
    }

    /// Build without re-validating. The caller asserts the filter has
    /// already been validated. Useful in tight engine loops.
    pub fn from_validated(filter: &'f Filter) -> Self {
        Self { filter }
    }

    /// Fresh per-run state matched to this filter.
    pub fn fresh_state(&self) -> FilterState {
        FilterState::new(self.filter)
    }

    /// The wrapped filter.
    pub fn filter(&self) -> &Filter {
        self.filter
    }

    /// Evaluate against one bar.
    ///
    /// **Order of operations:**
    /// 1. Push the bar into the indicator engine.
    /// 2. If still in warmup → return `Warming`. No condition evaluation.
    /// 3. Evaluate every condition leaf using current and previous-bar
    ///    indicator values. Update the previous-bar cache.
    /// 4. Combine via the All/Any rollup.
    /// 5. If tree is `false` → tick cooldown, return `Inactive`.
    /// 6. If tree is `true`:
    ///    - If `wake_when_in_position` suppresses it → return
    ///      `SuppressedInPosition`.
    ///    - If cooldown > 0 → return `Cooldown`.
    ///    - If daily cap reached → return `CappedForDay`.
    ///    - Else: determine transition (Trip vs Hold), arm cooldown
    ///      on Trip, return `Active`.
    pub fn evaluate(
        &self,
        state: &mut FilterState,
        bar: &Bar,
        ctx: EvalContext,
    ) -> FilterEvalOutcome {
        state.indicators.push(bar);

        if !state.is_warm() {
            return FilterEvalOutcome {
                decision: ActivationDecision::Warming {
                    bars_left: state.warmup_bars_left(),
                },
                conditions_passed: Vec::new(),
                tree_true: false,
            };
        }

        // Evaluate every condition leaf using the current indicator values.
        let leaves = self.filter.conditions.conditions();
        let mut results: Vec<ConditionResult> = Vec::with_capacity(leaves.len());

        for (i, cond) in leaves.iter().enumerate() {
            let prev = state.prev_conditions.get(i).copied().unwrap_or(None);
            let passed = eval_condition(cond, prev, &state.indicators);
            results.push(ConditionResult { passed });
        }

        // Cache for next bar's crosses_* detection.
        for (i, r) in results.iter().enumerate() {
            if let Some(slot) = state.prev_conditions.get_mut(i) {
                *slot = Some(r.passed);
            }
        }

        let tree_true = combine_tree(&self.filter.conditions, &results);

        if !tree_true {
            state.prev_tree = Some(false);
            state.tick_cooldown();
            return FilterEvalOutcome {
                decision: ActivationDecision::Inactive,
                conditions_passed: results,
                tree_true: false,
            };
        }

        // Tree is true; figure out which suppression (if any) applies.
        let suppressed_in_pos = matches!(
            (
                ctx.in_position,
                self.filter.wake_when_in_position,
            ),
            (true, WakeInPosition::Never | WakeInPosition::OnInvalidationOrTargetOnly)
        );

        if suppressed_in_pos {
            // We still tick cooldown so a position-open period doesn't
            // accumulate unbounded "ready to trip" state.
            state.tick_cooldown();
            state.prev_tree = Some(true);
            return FilterEvalOutcome {
                decision: ActivationDecision::SuppressedInPosition,
                conditions_passed: results,
                tree_true: true,
            };
        }

        if state.cooldown_left > 0 {
            let left = state.cooldown_left;
            state.tick_cooldown();
            state.prev_tree = Some(true);
            return FilterEvalOutcome {
                decision: ActivationDecision::Cooldown { bars_left: left },
                conditions_passed: results,
                tree_true: true,
            };
        }

        // Determine transition: Trip vs Hold.
        let prev_true = state.prev_tree.unwrap_or(false);
        let transition = if prev_true {
            Transition::Hold
        } else {
            Transition::Trip
        };
        state.prev_tree = Some(true);

        // Daily wakeup cap — checked on Trip only (a sustained Hold
        // doesn't consume a wakeup).
        if matches!(transition, Transition::Trip) {
            if let Some(cap) = self.filter.max_wakeups_per_day {
                let today = state.wakeups_on(ctx.ts);
                if today >= cap {
                    // Don't record the wakeup; report capped.
                    return FilterEvalOutcome {
                        decision: ActivationDecision::CappedForDay {
                            wakeups_today: today,
                        },
                        conditions_passed: results,
                        tree_true: true,
                    };
                }
            }
            state.note_wakeup(ctx.ts);
            // Arm cooldown on Trip.
            if self.filter.cooldown_bars > 0 {
                state.arm_cooldown(self.filter.cooldown_bars);
            }
        }

        FilterEvalOutcome {
            decision: ActivationDecision::Active { transition },
            conditions_passed: results,
            tree_true: true,
        }
    }
}

/// Combine per-condition booleans via the tree's logical op.
fn combine_tree(tree: &ConditionTree, results: &[ConditionResult]) -> bool {
    match tree {
        ConditionTree::All(_) => results.iter().all(|r| r.passed),
        ConditionTree::Any(_) => results.iter().any(|r| r.passed),
    }
}

/// Evaluate one condition leaf against current indicator values, using
/// the previous-bar leaf result to detect crosses.
fn eval_condition(
    cond: &Condition,
    prev_leaf_result: Option<bool>,
    engine: &crate::indicators::IndicatorEngine,
) -> bool {
    match cond.op {
        Operator::Gt => numeric_pair(cond, engine)
            .map(|(a, b)| a > b)
            .unwrap_or(false),
        Operator::Lt => numeric_pair(cond, engine)
            .map(|(a, b)| a < b)
            .unwrap_or(false),
        Operator::Gte => numeric_pair(cond, engine)
            .map(|(a, b)| a >= b)
            .unwrap_or(false),
        Operator::Lte => numeric_pair(cond, engine)
            .map(|(a, b)| a <= b)
            .unwrap_or(false),
        Operator::Eq => numeric_pair(cond, engine)
            .map(|(a, b)| (a - b).abs() < f64::EPSILON)
            .unwrap_or(false),
        Operator::Between => match (resolve_numeric(&cond.lhs, engine), &cond.rhs) {
            (Some(v), Operand::Range(lo, hi)) => v >= *lo && v <= *hi,
            _ => false,
        },
        Operator::CrossesAbove => {
            // Defined as: prev leaf result was false (lhs <= rhs) AND
            // current strict `>`. We don't have the prior numeric pair
            // available (we'd need to keep a 1-bar lag on every
            // indicator); instead we use the leaf cache: a strict ">"
            // result on this bar combined with prev_leaf_result == false
            // implies the cross. If prev_leaf_result is None we cannot
            // detect — return false.
            let now = numeric_pair(cond, engine)
                .map(|(a, b)| a > b)
                .unwrap_or(false);
            matches!((prev_leaf_result, now), (Some(false), true))
        }
        Operator::CrossesBelow => {
            let now = numeric_pair(cond, engine)
                .map(|(a, b)| a < b)
                .unwrap_or(false);
            matches!((prev_leaf_result, now), (Some(false), true))
        }
    }
}

/// Resolve both sides of a condition to numerics. Returns `None` if any
/// referenced indicator is still warming up.
fn numeric_pair(
    cond: &Condition,
    engine: &crate::indicators::IndicatorEngine,
) -> Option<(f64, f64)> {
    let a = resolve_numeric(&cond.lhs, engine)?;
    let b = resolve_numeric(&cond.rhs, engine)?;
    Some((a, b))
}

fn resolve_numeric(o: &Operand, engine: &crate::indicators::IndicatorEngine) -> Option<f64> {
    match o {
        Operand::Numeric(n) => Some(*n),
        Operand::Indicator(r) => engine.value(r),
        Operand::Range(_, _) => None,
    }
}

/// Public helper: list of indicator refs the runtime would feed for
/// `filter`. Exposed so the engine can pre-flight which indicators it
/// will track.
pub fn referenced_indicators(filter: &Filter) -> Vec<IndicatorRef> {
    crate::state::collect_indicator_refs(&filter.conditions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        AgentContextTemplateId, FilterId, FilterStatus, IndicatorName, ScanCadence, StrategyId,
        Symbol, Timeframe,
    };
    use chrono::TimeZone;

    fn ts(min: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap() + chrono::Duration::hours(min)
    }

    fn bar(c: f64) -> Bar {
        Bar::new(c, c + 0.5, c - 0.5, c)
    }

    fn mk_filter(tree: ConditionTree, cooldown: u32, max_wake: Option<u32>) -> Filter {
        Filter {
            id: FilterId::new("01H".to_string()),
            strategy_id: StrategyId::new("01S".to_string()),
            display_name: "t".into(),
            description: None,
            status: FilterStatus::Draft,
            asset_scope: vec![Symbol::new("BTC/USD".to_string())],
            timeframe: Timeframe::new("1h".to_string()),
            scan_cadence: ScanCadence::BarClose,
            conditions: tree,
            cooldown_bars: cooldown,
            max_wakeups_per_day: max_wake,
            wake_when_in_position: WakeInPosition::Always,
            agent_context_template: AgentContextTemplateId::new(
                crate::DEFAULT_AGENT_CONTEXT_TEMPLATE.to_string(),
            ),
        }
    }

    fn close_gt_threshold(threshold: f64) -> Filter {
        let tree = ConditionTree::All(vec![Condition {
            lhs: Operand::Indicator(IndicatorRef::close()),
            op: Operator::Gt,
            rhs: Operand::Numeric(threshold),
        }]);
        mk_filter(tree, 0, None)
    }

    #[test]
    fn inactive_then_active_then_hold() {
        let f = close_gt_threshold(50.0);
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();

        // close-only filter: no indicator warmup; evaluates from bar 1.
        let ctx = EvalContext {
            ts: ts(0),
            in_position: false,
        };
        let o = rt.evaluate(&mut state, &bar(40.0), ctx);
        assert_eq!(o.decision, ActivationDecision::Inactive);

        let o = rt.evaluate(&mut state, &bar(60.0), ctx);
        assert!(matches!(
            o.decision,
            ActivationDecision::Active {
                transition: Transition::Trip
            }
        ));

        let o = rt.evaluate(&mut state, &bar(65.0), ctx);
        assert!(matches!(
            o.decision,
            ActivationDecision::Active {
                transition: Transition::Hold
            }
        ));
    }

    #[test]
    fn cooldown_suppresses_re_trip() {
        let f = mk_filter(
            ConditionTree::All(vec![Condition {
                lhs: Operand::Indicator(IndicatorRef::close()),
                op: Operator::Gt,
                rhs: Operand::Numeric(50.0),
            }]),
            2,
            None,
        );
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        let ctx = EvalContext {
            ts: ts(0),
            in_position: false,
        };
        rt.evaluate(&mut state, &bar(40.0), ctx); // inactive
        let o = rt.evaluate(&mut state, &bar(60.0), ctx);
        assert!(o.decision.is_trip());
        // Cooldown armed = 2; next true bar reports Cooldown.
        let o = rt.evaluate(&mut state, &bar(60.0), ctx);
        assert!(matches!(
            o.decision,
            ActivationDecision::Cooldown { .. }
        ));
    }

    #[test]
    fn capped_for_day_blocks_extra_trips() {
        let f = mk_filter(
            ConditionTree::All(vec![Condition {
                lhs: Operand::Indicator(IndicatorRef::close()),
                op: Operator::Gt,
                rhs: Operand::Numeric(50.0),
            }]),
            0,
            Some(1),
        );
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        let ctx = EvalContext {
            ts: ts(0),
            in_position: false,
        };
        rt.evaluate(&mut state, &bar(40.0), ctx);
        rt.evaluate(&mut state, &bar(60.0), ctx); // Trip, wakeup=1
        rt.evaluate(&mut state, &bar(40.0), ctx); // Inactive (resets prev_tree)
        let o = rt.evaluate(&mut state, &bar(60.0), ctx);
        assert!(matches!(
            o.decision,
            ActivationDecision::CappedForDay { .. }
        ));
    }

    #[test]
    fn suppressed_in_position_when_never() {
        let mut f = close_gt_threshold(50.0);
        f.wake_when_in_position = WakeInPosition::Never;
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        rt.evaluate(
            &mut state,
            &bar(40.0),
            EvalContext {
                ts: ts(0),
                in_position: false,
            },
        );
        let o = rt.evaluate(
            &mut state,
            &bar(60.0),
            EvalContext {
                ts: ts(1),
                in_position: true,
            },
        );
        assert_eq!(o.decision, ActivationDecision::SuppressedInPosition);
    }

    #[test]
    fn crosses_above_fires_on_transition() {
        // close crosses_above ema_3 — uses two indicators so the
        // condition can genuinely flip false → true.
        let tree = ConditionTree::All(vec![Condition {
            lhs: Operand::Indicator(IndicatorRef::close()),
            op: Operator::CrossesAbove,
            rhs: Operand::Indicator(IndicatorRef::periodic(IndicatorName::Ema, 3)),
        }]);
        let f = mk_filter(tree, 0, None);
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        let ctx = EvalContext {
            ts: ts(0),
            in_position: false,
        };
        // Closes 10, 10, 10 → EMA seed = 10. Close == EMA, not crossing.
        rt.evaluate(&mut state, &bar(10.0), ctx);
        rt.evaluate(&mut state, &bar(10.0), ctx);
        let o3 = rt.evaluate(&mut state, &bar(10.0), ctx);
        // Bar 3 is the first post-warmup eval; prev_leaf is None so
        // CrossesAbove can't fire — Inactive.
        assert_eq!(o3.decision, ActivationDecision::Inactive);
        // Bar 4: close=20, ema_4 = 0.5*20 + 0.5*10 = 15 → close > ema.
        // prev_leaf was false (10 !> 10) → cross fires → Trip.
        let o4 = rt.evaluate(&mut state, &bar(20.0), ctx);
        assert!(o4.decision.is_trip(), "expected Trip, got {:?}", o4.decision);
    }
}
