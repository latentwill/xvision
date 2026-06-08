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
use std::collections::VecDeque;

use crate::errors::ValidationError;
use crate::indicators::Bar;
use crate::state::{FilterState, CONDITION_HISTORY_CAP};
use crate::types::{Condition, ConditionTree, Filter, IndicatorRef, Operand, Operator, WakeInPosition};
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
    /// True for any `Active` decision (Trip OR Hold) — i.e. the
    /// condition tree evaluated `true` on this bar without being
    /// suppressed by cooldown / cap / position gating. The engine uses
    /// this as its LLM dispatch gate so that both the first-crossing bar
    /// (Trip) and sustained-true bars (Hold) invoke the agent. The
    /// `FilterEventV1.triggered` field still uses `is_trip()` and is
    /// independent of dispatch.
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
    /// Per-condition boolean (index aligns with `conditions.leaves_dfs()`).
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
    pub fn evaluate(&self, state: &mut FilterState, bar: &Bar, ctx: EvalContext) -> FilterEvalOutcome {
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
        let leaves = self.filter.conditions.leaves_dfs();
        let mut results: Vec<ConditionResult> = Vec::with_capacity(leaves.len());
        let mut current_pairs: Vec<Option<(f64, f64)>> = Vec::with_capacity(leaves.len());

        for (i, cond) in leaves.iter().enumerate() {
            let prev_pair = state.prev_numeric_pairs.get(i).copied().unwrap_or(None);
            let current_pair = numeric_pair(*cond, &state.indicators);
            let (passed, raw_signal) = eval_condition(
                *cond,
                prev_pair,
                current_pair,
                state.numeric_pair_history.get(i),
                state.condition_history.get(i),
            );
            results.push(ConditionResult { passed });
            current_pairs.push(current_pair);
            push_bool_history(&mut state.condition_history[i], raw_signal);
            if let Some(pair) = current_pair {
                push_pair_history(&mut state.numeric_pair_history[i], pair);
            }
        }

        // Cache for next bar's diagnostics and crosses_* detection.
        for (i, r) in results.iter().enumerate() {
            if let Some(slot) = state.prev_conditions.get_mut(i) {
                *slot = Some(r.passed);
            }
            if let Some(slot) = state.prev_numeric_pairs.get_mut(i) {
                *slot = current_pairs[i];
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
        //
        // KTD3: the in-position wake policy distinguishes a fresh Trip (the
        // bar the tree first becomes true again — a NEW invalidation/target
        // signal the trader should see so it can close) from a sustained-true
        // Hold (a redundant per-bar re-eval). We must therefore compute the
        // Trip/Hold transition HERE, before the suppression decision, rather
        // than after it as the original code did.
        //
        //   * `Always`                     → wake on every active bar
        //                                     (Trip AND Hold) while holding.
        //   * `OnInvalidationOrTargetOnly` → wake on a fresh Trip while
        //                                     holding; suppress sustained-true
        //                                     Hold bars (the per-bar polling
        //                                     cost win). A gate true→false
        //                                     invalidation already returns
        //                                     `Inactive` above (before this
        //                                     gate) and is never suppressed.
        //   * `Never`                      → never wake while holding
        //                                     (entries-only filter; exits rely
        //                                     on the deterministic SL/TP).
        //
        // Before this fix `OnInvalidationOrTargetOnly` was treated as a full
        // suppression (== `Never`), so it never woke the trader on a fresh
        // in-position trip despite its name.
        let prev_true = state.prev_tree.unwrap_or(false);
        let transition = if prev_true {
            Transition::Hold
        } else {
            Transition::Trip
        };
        let suppressed_in_pos = ctx.in_position
            && match self.filter.wake_when_in_position {
                WakeInPosition::Always => false,
                WakeInPosition::Never => true,
                WakeInPosition::OnInvalidationOrTargetOnly => {
                    matches!(transition, Transition::Hold)
                }
            };

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

        // Transition (Trip vs Hold) was already determined above — the
        // suppression gate needs it, and nothing between here and there
        // mutates `state.prev_tree`. Reuse it.
        // Daily wakeup cap — checked on Trip only (a sustained Hold
        // doesn't consume a wakeup).
        if matches!(transition, Transition::Trip) {
            if let Some(cap) = self.filter.max_wakeups_per_day {
                let today = state.wakeups_on(ctx.ts);
                if today >= cap {
                    // Don't record the wakeup; report capped.
                    return FilterEvalOutcome {
                        decision: ActivationDecision::CappedForDay { wakeups_today: today },
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
        state.prev_tree = Some(true);

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

/// Evaluate one condition leaf against current indicator values.
fn eval_condition(
    cond: &Condition,
    prev_pair: Option<(f64, f64)>,
    current_pair: Option<(f64, f64)>,
    pair_history: Option<&VecDeque<(f64, f64)>>,
    signal_history: Option<&VecDeque<bool>>,
) -> (bool, bool) {
    let raw = match cond.op {
        Operator::Gt => current_pair.map(|(a, b)| a > b).unwrap_or(false),
        Operator::Lt => current_pair.map(|(a, b)| a < b).unwrap_or(false),
        Operator::Gte => current_pair.map(|(a, b)| a >= b).unwrap_or(false),
        Operator::Lte => current_pair.map(|(a, b)| a <= b).unwrap_or(false),
        Operator::Eq => current_pair
            .map(|(a, b)| (a - b).abs() < f64::EPSILON)
            .unwrap_or(false),
        Operator::Between => match (current_pair, &cond.rhs) {
            (Some((v, _)), Operand::Range(lo, hi)) => v >= *lo && v <= *hi,
            _ => false,
        },
        Operator::CrossesAbove => match (prev_pair, current_pair) {
            (Some((prev_lhs, prev_rhs)), Some((lhs, rhs))) => prev_lhs <= prev_rhs && lhs > rhs,
            _ => false,
        },
        Operator::CrossesBelow => match (prev_pair, current_pair) {
            (Some((prev_lhs, prev_rhs)), Some((lhs, rhs))) => prev_lhs >= prev_rhs && lhs < rhs,
            _ => false,
        },
        Operator::AboveFor(_) => current_pair.map(|(a, b)| a > b).unwrap_or(false),
        Operator::BelowFor(_) => current_pair.map(|(a, b)| a < b).unwrap_or(false),
        Operator::CrossedAbove(_) => match (prev_pair, current_pair) {
            (Some((prev_lhs, prev_rhs)), Some((lhs, rhs))) => prev_lhs <= prev_rhs && lhs > rhs,
            _ => false,
        },
        Operator::CrossedBelow(_) => match (prev_pair, current_pair) {
            (Some((prev_lhs, prev_rhs)), Some((lhs, rhs))) => prev_lhs >= prev_rhs && lhs < rhs,
            _ => false,
        },
        Operator::SlopeGt(n) => slope(pair_history, current_pair, n)
            .map(|(s, b)| s > b)
            .unwrap_or(false),
        Operator::SlopeLt(n) => slope(pair_history, current_pair, n)
            .map(|(s, b)| s < b)
            .unwrap_or(false),
        Operator::ZscoreGt(n) => zscore(pair_history, current_pair, n)
            .map(|(z, b)| z > b)
            .unwrap_or(false),
        Operator::ZscoreLt(n) => zscore(pair_history, current_pair, n)
            .map(|(z, b)| z < b)
            .unwrap_or(false),
        Operator::WithinPct(pct) => current_pair
            .map(|(a, b)| b.abs() > f64::EPSILON && ((a - b).abs() / b.abs()) * 100.0 <= pct)
            .unwrap_or(false),
    };

    let passed = match cond.op {
        Operator::AboveFor(n) | Operator::BelowFor(n) => {
            raw && all_recent(signal_history, n.saturating_sub(1))
        }
        Operator::CrossedAbove(n) | Operator::CrossedBelow(n) => {
            raw || any_recent(signal_history, n.saturating_sub(1))
        }
        _ => raw,
    };
    (passed, raw)
}

fn all_recent(history: Option<&VecDeque<bool>>, n: u32) -> bool {
    if n == 0 {
        return true;
    }
    let Some(history) = history else {
        return false;
    };
    if history.len() < n as usize {
        return false;
    }
    history.iter().rev().take(n as usize).all(|v| *v)
}

fn any_recent(history: Option<&VecDeque<bool>>, n: u32) -> bool {
    if n == 0 {
        return false;
    }
    history
        .map(|h| h.iter().rev().take(n as usize).any(|v| *v))
        .unwrap_or(false)
}

fn slope(
    history: Option<&VecDeque<(f64, f64)>>,
    current_pair: Option<(f64, f64)>,
    n: u32,
) -> Option<(f64, f64)> {
    let (current_lhs, rhs) = current_pair?;
    let history = history?;
    let n = n as usize;
    if history.len() < n {
        return None;
    }
    let prior = history.get(history.len() - n)?.0;
    Some((current_lhs - prior, rhs))
}

fn zscore(
    history: Option<&VecDeque<(f64, f64)>>,
    current_pair: Option<(f64, f64)>,
    n: u32,
) -> Option<(f64, f64)> {
    let (current_lhs, rhs) = current_pair?;
    let n = n as usize;
    if n < 2 {
        return None;
    }
    let history = history?;
    if history.len() + 1 < n {
        return None;
    }
    let mut values: Vec<f64> = history.iter().rev().take(n - 1).map(|(lhs, _)| *lhs).collect();
    values.push(current_lhs);
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let var = values
        .iter()
        .map(|v| {
            let d = *v - mean;
            d * d
        })
        .sum::<f64>()
        / values.len() as f64;
    let stddev = var.sqrt();
    if stddev <= f64::EPSILON {
        return None;
    }
    Some(((current_lhs - mean) / stddev, rhs))
}

fn push_bool_history(history: &mut VecDeque<bool>, value: bool) {
    history.push_back(value);
    if history.len() > CONDITION_HISTORY_CAP {
        history.pop_front();
    }
}

fn push_pair_history(history: &mut VecDeque<(f64, f64)>, value: (f64, f64)) {
    history.push_back(value);
    if history.len() > CONDITION_HISTORY_CAP {
        history.pop_front();
    }
}

/// Resolve a condition to the numeric pair needed by its operator. For
/// `between`, only the LHS is dynamic; the RHS range stays on the
/// condition itself.
fn numeric_pair(cond: &Condition, engine: &crate::indicators::IndicatorEngine) -> Option<(f64, f64)> {
    let a = resolve_numeric(&cond.lhs, engine)?;
    if matches!(cond.op, Operator::Between) {
        return Some((a, 0.0));
    }
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

// ---------------------------------------------------------------------------
// Phase C DSL → agent-graph bridge
// ---------------------------------------------------------------------------

/// Engine-side signal shape that the DSL bridge produces. Mirrors the
/// `xvision_engine::agent::dispatch_capability::FilterSignal` payload
/// shape — kept here as a plain struct so this crate stays
/// engine-independent.
///
/// The agent-graph dispatcher in `xvision-engine` either constructs a
/// `FilterSignal` from this bridge's output or accepts the bridge's
/// fields directly. We use a serde-compatible shape so the engine can
/// `serde_json::from_value::<engine::FilterSignal>(bridge_value)` if
/// it prefers to round-trip through JSON.
///
/// Note: `granularity` is hard-coded to `"bar"` because DSL filters
/// are always bar-cadence today (contract acceptance: "DSL filters
/// are always bar-cadence today").
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BridgedFilterSignal {
    pub name: String,
    pub payload: serde_json::Value,
    pub granularity: String,
}

/// Adapter wrapping `RuntimeFilter::evaluate()`'s `ActivationDecision`
/// into a `BridgedFilterSignal` whose `payload` is a stable
/// `{ "active": <bool>, "reason": <string?> }` shape — matching the
/// edge-predicate contract on the engine side.
///
/// The `active` flag reflects whether the DSL filter is "tripping" or
/// "holding" — i.e. `ActivationDecision::is_active()`. `reason` is the
/// human-readable tag for non-active outcomes (`"warming"`,
/// `"inactive"`, `"cooldown"`, `"capped_for_day"`,
/// `"suppressed_in_position"`) and `null` when active.
///
/// Why `null` instead of `Some("active")` when the filter is active?
/// Predicate-authoring ergonomics: an `Eq` on `payload.active = true`
/// is a one-liner; an additional `Eq` on `payload.reason = …` should
/// only need to be authored for the suppression cases.
pub fn dsl_to_filter_signal(filter_id: &str, decision: ActivationDecision) -> BridgedFilterSignal {
    let active = decision.is_active();
    let reason = if active {
        None
    } else {
        Some(decision.tag().to_string())
    };
    let payload = serde_json::json!({
        "active": active,
        "reason": reason,
    });
    BridgedFilterSignal {
        name: filter_id.to_string(),
        payload,
        granularity: "bar".to_string(),
    }
}

#[cfg(test)]
mod bridge_tests {
    use super::*;

    #[test]
    fn dsl_to_filter_signal_active_emits_true_with_null_reason() {
        let s = dsl_to_filter_signal(
            "regime_filter",
            ActivationDecision::Active {
                transition: Transition::Trip,
            },
        );
        assert_eq!(s.name, "regime_filter");
        assert_eq!(s.granularity, "bar");
        assert_eq!(s.payload["active"], serde_json::Value::Bool(true));
        assert_eq!(s.payload["reason"], serde_json::Value::Null);
    }

    #[test]
    fn dsl_to_filter_signal_inactive_emits_false_with_tag_reason() {
        let s = dsl_to_filter_signal("f", ActivationDecision::Inactive);
        assert_eq!(s.payload["active"], serde_json::Value::Bool(false));
        assert_eq!(s.payload["reason"], serde_json::Value::String("inactive".into()));
    }

    #[test]
    fn dsl_to_filter_signal_cooldown_carries_tag_in_reason() {
        let s = dsl_to_filter_signal("f", ActivationDecision::Cooldown { bars_left: 3 });
        assert_eq!(s.payload["active"], serde_json::Value::Bool(false));
        assert_eq!(s.payload["reason"], serde_json::Value::String("cooldown".into()));
    }

    #[test]
    fn dsl_to_filter_signal_capped_for_day_carries_tag_in_reason() {
        let s = dsl_to_filter_signal("f", ActivationDecision::CappedForDay { wakeups_today: 5 });
        assert_eq!(
            s.payload["reason"],
            serde_json::Value::String("capped_for_day".into())
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        AgentContextTemplateId, ConditionItem, FilterId, FilterStatus, IndicatorName, ScanCadence,
        StrategyId, Symbol, Timeframe,
    };
    use chrono::TimeZone;

    fn ts(min: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap() + chrono::Duration::hours(min)
    }

    fn bar(c: f64) -> Bar {
        Bar::new(c, c + 0.5, c - 0.5, c)
    }

    fn bar_with_open(open: f64, close: f64) -> Bar {
        Bar::new(open, open.max(close) + 0.5, open.min(close) - 0.5, close)
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
            fire: None,
            cooldown_bars: cooldown,
            max_wakeups_per_day: max_wake,
            wake_when_in_position: WakeInPosition::Always,
            agent_context_template: AgentContextTemplateId::new(
                crate::DEFAULT_AGENT_CONTEXT_TEMPLATE.to_string(),
            ),
        }
    }

    fn close_gt_threshold(threshold: f64) -> Filter {
        let tree = ConditionTree::All(vec![ConditionItem::Leaf(Condition {
            lhs: Operand::Indicator(IndicatorRef::close()),
            op: Operator::Gt,
            rhs: Operand::Numeric(threshold),
        })]);
        mk_filter(tree, 0, None)
    }

    fn close_op_threshold(op: Operator, threshold: f64) -> Filter {
        let tree = ConditionTree::All(vec![ConditionItem::Leaf(Condition {
            lhs: Operand::Indicator(IndicatorRef::close()),
            op,
            rhs: Operand::Numeric(threshold),
        })]);
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
    fn above_for_requires_consecutive_true_bars() {
        let f = close_op_threshold(Operator::AboveFor(3), 50.0);
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        let ctx = EvalContext {
            ts: ts(0),
            in_position: false,
        };

        assert_eq!(
            rt.evaluate(&mut state, &bar(51.0), ctx).decision,
            ActivationDecision::Inactive
        );
        assert_eq!(
            rt.evaluate(&mut state, &bar(52.0), ctx).decision,
            ActivationDecision::Inactive
        );
        let o = rt.evaluate(&mut state, &bar(53.0), ctx);
        assert!(matches!(o.decision, ActivationDecision::Active { .. }));
    }

    #[test]
    fn crossed_above_remains_true_inside_recent_window() {
        let tree = ConditionTree::All(vec![ConditionItem::Leaf(Condition {
            lhs: Operand::Indicator(IndicatorRef::close()),
            op: Operator::CrossedAbove(3),
            rhs: Operand::Indicator(IndicatorRef {
                name: IndicatorName::Open,
                period: None,
                bar_offset: None,
            }),
        })]);
        let f = mk_filter(tree, 0, None);
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        let ctx = EvalContext {
            ts: ts(0),
            in_position: false,
        };

        assert_eq!(
            rt.evaluate(&mut state, &bar_with_open(10.0, 9.0), ctx).decision,
            ActivationDecision::Inactive
        );
        assert!(matches!(
            rt.evaluate(&mut state, &bar_with_open(10.0, 11.0), ctx).decision,
            ActivationDecision::Active { .. }
        ));
        assert!(matches!(
            rt.evaluate(&mut state, &bar_with_open(10.0, 12.0), ctx).decision,
            ActivationDecision::Active { .. }
        ));
    }

    #[test]
    fn transform_operators_use_condition_history() {
        let ctx = EvalContext {
            ts: ts(0),
            in_position: false,
        };

        let slope = close_op_threshold(Operator::SlopeGt(2), 3.0);
        let rt = RuntimeFilter::new(&slope).unwrap();
        let mut state = rt.fresh_state();
        assert_eq!(
            rt.evaluate(&mut state, &bar(10.0), ctx).decision,
            ActivationDecision::Inactive
        );
        assert_eq!(
            rt.evaluate(&mut state, &bar(12.0), ctx).decision,
            ActivationDecision::Inactive
        );
        assert!(matches!(
            rt.evaluate(&mut state, &bar(15.0), ctx).decision,
            ActivationDecision::Active { .. }
        ));

        let zscore = close_op_threshold(Operator::ZscoreGt(3), 1.0);
        let rt = RuntimeFilter::new(&zscore).unwrap();
        let mut state = rt.fresh_state();
        assert_eq!(
            rt.evaluate(&mut state, &bar(100.0), ctx).decision,
            ActivationDecision::Inactive
        );
        assert_eq!(
            rt.evaluate(&mut state, &bar(100.0), ctx).decision,
            ActivationDecision::Inactive
        );
        assert!(matches!(
            rt.evaluate(&mut state, &bar(110.0), ctx).decision,
            ActivationDecision::Active { .. }
        ));
    }

    #[test]
    fn within_pct_compares_distance_to_rhs() {
        let tree = ConditionTree::All(vec![ConditionItem::Leaf(Condition {
            lhs: Operand::Indicator(IndicatorRef::close()),
            op: Operator::WithinPct(1.0),
            rhs: Operand::Indicator(IndicatorRef {
                name: IndicatorName::Open,
                period: None,
                bar_offset: None,
            }),
        })]);
        let f = mk_filter(tree, 0, None);
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        let ctx = EvalContext {
            ts: ts(0),
            in_position: false,
        };

        assert!(matches!(
            rt.evaluate(&mut state, &bar_with_open(100.0, 100.8), ctx)
                .decision,
            ActivationDecision::Active { .. }
        ));
        assert_eq!(
            rt.evaluate(&mut state, &bar_with_open(100.0, 103.0), ctx)
                .decision,
            ActivationDecision::Inactive
        );
    }

    #[test]
    fn cooldown_suppresses_re_trip() {
        let f = mk_filter(
            ConditionTree::All(vec![ConditionItem::Leaf(Condition {
                lhs: Operand::Indicator(IndicatorRef::close()),
                op: Operator::Gt,
                rhs: Operand::Numeric(50.0),
            })]),
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
        assert!(matches!(o.decision, ActivationDecision::Cooldown { .. }));
    }

    #[test]
    fn crosses_above_fires_once_on_actual_numeric_cross() {
        let f = mk_filter(
            ConditionTree::All(vec![ConditionItem::Leaf(Condition {
                lhs: Operand::Indicator(IndicatorRef::close()),
                op: Operator::CrossesAbove,
                rhs: Operand::Indicator(IndicatorRef::periodic(IndicatorName::Sma, 2)),
            })]),
            0,
            None,
        );
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        let ctx = EvalContext {
            ts: ts(0),
            in_position: false,
        };

        let decisions = [10.0, 10.0, 9.0, 12.0, 13.0, 14.0]
            .into_iter()
            .map(|close| rt.evaluate(&mut state, &bar(close), ctx).decision)
            .collect::<Vec<_>>();

        let trips = decisions.iter().filter(|decision| decision.is_trip()).count();
        assert_eq!(
            trips, 1,
            "sustained close > sma_2 must not retrigger crosses_above"
        );
    }

    #[test]
    fn crosses_above_numeric_threshold_fires_on_cross() {
        // `close crosses_above 50` — rhs is a constant. Passes validation
        // and the runtime evaluates prev_close <= 50 && close > 50.
        let f = mk_filter(
            ConditionTree::All(vec![ConditionItem::Leaf(Condition {
                lhs: Operand::Indicator(IndicatorRef::close()),
                op: Operator::CrossesAbove,
                rhs: Operand::Numeric(50.0),
            })]),
            0,
            None,
        );
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        let ctx = EvalContext {
            ts: ts(0),
            in_position: false,
        };

        // Bar 1: close=40, no prev_pair yet → Inactive.
        assert_eq!(rt.evaluate(&mut state, &bar(40.0), ctx).decision, ActivationDecision::Inactive);
        // Bar 2: prev=40 (<= 50), current=60 (> 50) → Trip.
        let o = rt.evaluate(&mut state, &bar(60.0), ctx);
        assert!(o.decision.is_trip(), "expected Trip on cross, got {:?}", o.decision);
        // Bar 3: prev=60 (> 50), current=70 (> 50) → no cross → Hold is false → Inactive.
        let o = rt.evaluate(&mut state, &bar(70.0), ctx);
        assert_eq!(o.decision, ActivationDecision::Inactive);
        // Bar 4: close=30, crosses below (not above) → Inactive.
        let o = rt.evaluate(&mut state, &bar(30.0), ctx);
        assert_eq!(o.decision, ActivationDecision::Inactive);
        // Bar 5: prev=30 (<= 50), current=55 (> 50) → Trip again.
        let o = rt.evaluate(&mut state, &bar(55.0), ctx);
        assert!(o.decision.is_trip(), "expected second Trip, got {:?}", o.decision);
    }

    #[test]
    fn crosses_below_numeric_threshold_fires_on_cross() {
        let f = mk_filter(
            ConditionTree::All(vec![ConditionItem::Leaf(Condition {
                lhs: Operand::Indicator(IndicatorRef::close()),
                op: Operator::CrossesBelow,
                rhs: Operand::Numeric(70.0),
            })]),
            0,
            None,
        );
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        let ctx = EvalContext {
            ts: ts(0),
            in_position: false,
        };

        assert_eq!(rt.evaluate(&mut state, &bar(80.0), ctx).decision, ActivationDecision::Inactive);
        // prev=80 (>= 70), current=60 (< 70) → Trip.
        let o = rt.evaluate(&mut state, &bar(60.0), ctx);
        assert!(o.decision.is_trip(), "expected Trip on cross below, got {:?}", o.decision);
    }

    #[test]
    fn capped_for_day_blocks_extra_trips() {
        let f = mk_filter(
            ConditionTree::All(vec![ConditionItem::Leaf(Condition {
                lhs: Operand::Indicator(IndicatorRef::close()),
                op: Operator::Gt,
                rhs: Operand::Numeric(50.0),
            })]),
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
        assert!(matches!(o.decision, ActivationDecision::CappedForDay { .. }));
    }

    #[test]
    fn capped_for_day_stays_capped_while_tree_stays_true() {
        let f = mk_filter(
            ConditionTree::All(vec![ConditionItem::Leaf(Condition {
                lhs: Operand::Indicator(IndicatorRef::close()),
                op: Operator::Gt,
                rhs: Operand::Numeric(50.0),
            })]),
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
        rt.evaluate(&mut state, &bar(40.0), ctx); // Inactive resets prev_tree.

        let capped = rt.evaluate(&mut state, &bar(60.0), ctx);
        assert!(matches!(capped.decision, ActivationDecision::CappedForDay { .. }));

        let still_capped = rt.evaluate(&mut state, &bar(65.0), ctx);
        assert!(matches!(
            still_capped.decision,
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
    fn on_invalidation_only_wakes_on_trip_but_suppresses_in_position_hold() {
        // KTD3 / R4 regression: with `OnInvalidationOrTargetOnly`, a fresh
        // Trip while holding still wakes (so the trader can close), but the
        // sustained-true Hold bars that follow are suppressed instead of
        // dispatching the trader LLM on every in-position bar.
        let mut f = close_gt_threshold(50.0);
        f.wake_when_in_position = WakeInPosition::OnInvalidationOrTargetOnly;
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();

        // Out of position, tree false -> Inactive.
        rt.evaluate(
            &mut state,
            &bar(40.0),
            EvalContext {
                ts: ts(0),
                in_position: false,
            },
        );
        // In position, tree becomes true -> fresh Trip -> NOT suppressed.
        let trip = rt.evaluate(
            &mut state,
            &bar(60.0),
            EvalContext {
                ts: ts(1),
                in_position: true,
            },
        );
        assert!(
            matches!(
                trip.decision,
                ActivationDecision::Active {
                    transition: Transition::Trip
                }
            ),
            "fresh in-position trip must wake the trader so it can close, got {:?}",
            trip.decision
        );
        // In position, tree stays true -> Hold -> suppressed (no per-bar LLM).
        for min in 2..6 {
            let hold = rt.evaluate(
                &mut state,
                &bar(65.0),
                EvalContext {
                    ts: ts(min),
                    in_position: true,
                },
            );
            assert_eq!(
                hold.decision,
                ActivationDecision::SuppressedInPosition,
                "sustained-true in-position bar must be suppressed, not dispatched"
            );
        }
    }

    #[test]
    fn on_invalidation_only_lets_gate_invalidation_through_as_inactive() {
        // R4: a gate true->false invalidation while holding is NEVER
        // suppressed — the tree-false branch returns `Inactive` before the
        // in-position suppression gate runs. (The trader's exit is then driven
        // by the deterministic SL/TP; the filter does not masquerade the
        // invalidation as a wake, but it also does not hide it as a
        // suppression.)
        let mut f = close_gt_threshold(50.0);
        f.wake_when_in_position = WakeInPosition::OnInvalidationOrTargetOnly;
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        // Out of position: trip true.
        rt.evaluate(
            &mut state,
            &bar(40.0),
            EvalContext { ts: ts(0), in_position: false },
        );
        rt.evaluate(
            &mut state,
            &bar(60.0),
            EvalContext { ts: ts(1), in_position: true },
        );
        // Gate invalidates (close drops below threshold) while holding.
        let invalidated = rt.evaluate(
            &mut state,
            &bar(40.0),
            EvalContext { ts: ts(2), in_position: true },
        );
        assert_eq!(
            invalidated.decision,
            ActivationDecision::Inactive,
            "gate invalidation while holding must NOT be reported as SuppressedInPosition"
        );
    }

    #[test]
    fn on_invalidation_only_still_wakes_every_active_bar_out_of_position() {
        // Out of position the policy is a no-op: both Trip and Hold are active
        // (regression guard for the cost win — it must not leak out of
        // position).
        let mut f = close_gt_threshold(50.0);
        f.wake_when_in_position = WakeInPosition::OnInvalidationOrTargetOnly;
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        let ctx = EvalContext {
            ts: ts(0),
            in_position: false,
        };
        rt.evaluate(&mut state, &bar(40.0), ctx);
        let trip = rt.evaluate(&mut state, &bar(60.0), ctx);
        assert!(matches!(
            trip.decision,
            ActivationDecision::Active {
                transition: Transition::Trip
            }
        ));
        let hold = rt.evaluate(&mut state, &bar(65.0), ctx);
        assert!(matches!(
            hold.decision,
            ActivationDecision::Active {
                transition: Transition::Hold
            }
        ));
    }

    #[test]
    fn always_policy_still_wakes_every_in_position_hold_bar() {
        // `Always` remains an explicit opt-in reproducing the per-bar
        // behavior — proof the fix is a semantics change, not a hard removal.
        let mut f = close_gt_threshold(50.0);
        f.wake_when_in_position = WakeInPosition::Always;
        let rt = RuntimeFilter::new(&f).unwrap();
        let mut state = rt.fresh_state();
        rt.evaluate(
            &mut state,
            &bar(40.0),
            EvalContext { ts: ts(0), in_position: false },
        );
        let trip = rt.evaluate(
            &mut state,
            &bar(60.0),
            EvalContext { ts: ts(1), in_position: true },
        );
        assert!(trip.decision.is_active());
        let hold = rt.evaluate(
            &mut state,
            &bar(65.0),
            EvalContext { ts: ts(2), in_position: true },
        );
        assert!(
            matches!(
                hold.decision,
                ActivationDecision::Active {
                    transition: Transition::Hold
                }
            ),
            "Always must still wake on sustained-true in-position bars, got {:?}",
            hold.decision
        );
    }

    #[test]
    fn crosses_above_fires_on_transition() {
        // close crosses_above ema_3 — uses two indicators so the
        // condition can genuinely flip false → true.
        let tree = ConditionTree::All(vec![ConditionItem::Leaf(Condition {
            lhs: Operand::Indicator(IndicatorRef::close()),
            op: Operator::CrossesAbove,
            rhs: Operand::Indicator(IndicatorRef::periodic(IndicatorName::Ema, 3)),
        })]);
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
