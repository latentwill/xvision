use crate::autooptimizer::mutator::MutationDiff;
use crate::eval::MetricsSummary;
use anyhow::{bail, Result};
use serde::{Deserialize, Deserializer, Serialize};

/// Factor by which the child's worst drawdown may exceed the parent's before rejection.
const DRAWDOWN_DETERIORATION_FACTOR: f64 = 1.5;

/// Tolerance applied to boundary comparisons.
/// Prevents identical inputs from flipping at the threshold due to FP rounding.
const CMP_EPS: f64 = 1e-9;

/// F24: the metric a mutation cycle optimizes. Higher-is-better for all but
/// `MaxDrawdown`, which the gate minimizes. (`sortino` and a cost/efficiency axis
/// are deferred: Sortino isn't computed in `MetricsSummary`, and a cost objective
/// needs the F11/F23 realized-cost metering as its input.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Objective {
    #[default]
    Sharpe,
    TotalReturn,
    MaxDrawdown,
    WinRate,
}

impl Objective {
    /// The objective's value for a window's metrics, oriented so that a LARGER
    /// number is always better (drawdown is negated so "reduce drawdown" reads as
    /// an increase, unifying the gate's delta comparison).
    pub fn oriented_value(&self, m: &MetricsSummary) -> f64 {
        match self {
            Self::Sharpe => m.sharpe,
            Self::TotalReturn => m.total_return_pct,
            Self::WinRate => m.win_rate,
            // Lower drawdown is better → negate the magnitude so a reduction is a
            // positive delta.
            Self::MaxDrawdown => -m.max_drawdown_pct.abs(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Sharpe => "sharpe",
            Self::TotalReturn => "total_return",
            Self::MaxDrawdown => "max_drawdown",
            Self::WinRate => "win_rate",
        }
    }

    /// Parse an operator-supplied objective name (CLI flag / config). Accepts the
    /// canonical snake_case labels plus a couple of common aliases.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sharpe" => Some(Self::Sharpe),
            "total_return" | "return" | "total-return" => Some(Self::TotalReturn),
            "max_drawdown" | "drawdown" | "max-drawdown" => Some(Self::MaxDrawdown),
            "win_rate" | "winrate" | "win-rate" => Some(Self::WinRate),
            _ => None,
        }
    }

    /// All selectable objective labels, for CLI help / error messages.
    pub fn all_labels() -> &'static [&'static str] {
        &["sharpe", "total_return", "max_drawdown", "win_rate"]
    }
}

/// Inputs to the deterministic numeric gate.
///
/// `min_improvement` applies to the day (in-sample) window;
/// `holdout_min_improvement` applies to the baseline-untouched (out-of-sample) window.
#[derive(Debug, Clone, Serialize)]
pub struct GateInput {
    pub parent_day_metrics: MetricsSummary,
    pub child_day_metrics: MetricsSummary,
    pub parent_untouched_metrics: MetricsSummary,
    pub child_untouched_metrics: MetricsSummary,
    pub min_improvement: f64,
    /// Minimum improvement threshold for the holdout (baseline-untouched) window.
    /// Separate from `min_improvement` so the out-of-sample bar can differ from
    /// the in-sample bar.
    pub holdout_min_improvement: f64,
    /// F24: which metric to optimize. Defaults to `Sharpe` (serde default) so
    /// existing call sites/fixtures that omit it keep the prior behavior.
    #[serde(default)]
    pub objective: Objective,
}

impl<'de> Deserialize<'de> for GateInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct GateInputWire {
            parent_day_metrics: MetricsSummary,
            child_day_metrics: MetricsSummary,
            parent_untouched_metrics: MetricsSummary,
            child_untouched_metrics: MetricsSummary,
            min_improvement: f64,
            #[serde(default)]
            holdout_min_improvement: Option<f64>,
            #[serde(default)]
            objective: Objective,
        }

        let wire = GateInputWire::deserialize(deserializer)?;
        Ok(Self {
            parent_day_metrics: wire.parent_day_metrics,
            child_day_metrics: wire.child_day_metrics,
            parent_untouched_metrics: wire.parent_untouched_metrics,
            child_untouched_metrics: wire.child_untouched_metrics,
            min_improvement: wire.min_improvement,
            holdout_min_improvement: wire.holdout_min_improvement.unwrap_or(wire.min_improvement),
            objective: wire.objective,
        })
    }
}

/// Outcome of `evaluate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateVerdict {
    Pass,
    Fail { reason: String },
}

impl GateVerdict {
    pub fn as_str(&self) -> String {
        match self {
            Self::Pass => "passed".to_string(),
            Self::Fail { reason } => format!("rejected:{reason}"),
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "pass" | "passed" => Ok(Self::Pass),
            "fail" | "rejected" => Ok(Self::Fail {
                reason: "stored fail verdict".to_string(),
            }),
            _ if s.starts_with("fail:") => Ok(Self::Fail {
                reason: s.trim_start_matches("fail:").to_string(),
            }),
            _ if s.starts_with("rejected:") => Ok(Self::Fail {
                reason: s.trim_start_matches("rejected:").to_string(),
            }),
            _ => bail!("unknown GateVerdict: {s}"),
        }
    }
}

/// Passes only when all three hold:
/// 1. Δ score (day window)       ≥ `min_improvement`
/// 2. Δ score (untouched window) ≥ `holdout_min_improvement`
/// 3. Child worst drawdown       ≤ parent worst drawdown × 1.5
///
pub fn evaluate(input: &GateInput) -> GateVerdict {
    debug_assert!(
        input.min_improvement.is_finite(),
        "min_improvement must be finite"
    );

    // F24: evaluate the operator-selected objective on BOTH windows (the
    // held-out discipline is preserved — a candidate must improve on the day
    // window AND the untouched window, each against its own threshold).
    // `oriented_value` makes larger always better.
    let obj = input.objective;
    let delta_day =
        obj.oriented_value(&input.child_day_metrics) - obj.oriented_value(&input.parent_day_metrics);
    let delta_untouched = obj.oriented_value(&input.child_untouched_metrics)
        - obj.oriented_value(&input.parent_untouched_metrics);

    // B16: evaluate EVERY condition before returning so the rejection reason
    // surfaces all failing checks, not just the first. A near-miss on the
    // untouched (holdout) window must stay visible even when the day window is
    // the one that tripped the gate. Order is deterministic: day, untouched,
    // drawdown — so the reason string is byte-stable for identical inputs.
    let mut failures: Vec<String> = Vec::new();

    let day_failed = delta_day < input.min_improvement - CMP_EPS;
    if day_failed {
        failures.push(format!(
            "today's score ({}) improved by {delta_day:.6} \
             but minimum-improvement threshold is {:.6}",
            obj.label(),
            input.min_improvement
        ));
    }

    let untouched_failed = delta_untouched < input.holdout_min_improvement - CMP_EPS;
    if untouched_failed {
        failures.push(format!(
            "baseline-untouched-score ({}) improved by {delta_untouched:.6} \
             but holdout minimum-improvement threshold is {:.6}",
            obj.label(),
            input.holdout_min_improvement
        ));
    } else if day_failed {
        // The day check failed but the holdout passed: surface the holdout delta
        // anyway so a holdout near-miss (or near-pass) is never invisible.
        failures.push(format!(
            "baseline-untouched-score ({}) improved by {delta_untouched:.6} \
             (cleared the {:.6} holdout minimum)",
            obj.label(),
            input.holdout_min_improvement
        ));
    }

    // Non-objective risk guard: don't let drawdown blow up while optimizing some
    // OTHER axis. Skipped when the objective IS drawdown (it's already the thing
    // being checked, and the guard would double-count / conflict).
    if obj != Objective::MaxDrawdown {
        let parent_worst = input
            .parent_day_metrics
            .max_drawdown_pct
            .abs()
            .max(input.parent_untouched_metrics.max_drawdown_pct.abs());
        let child_worst = input
            .child_day_metrics
            .max_drawdown_pct
            .abs()
            .max(input.child_untouched_metrics.max_drawdown_pct.abs());

        if child_worst > parent_worst * DRAWDOWN_DETERIORATION_FACTOR + CMP_EPS {
            failures.push(format!(
                "max drawdown deteriorated: candidate worst {child_worst:.4}% exceeds \
                 {DRAWDOWN_DETERIORATION_FACTOR}× parent worst {parent_worst:.4}%"
            ));
        }
    }

    if failures.is_empty() {
        GateVerdict::Pass
    } else {
        GateVerdict::Fail {
            reason: failures.join("; "),
        }
    }
}

/// Per-dimension candidate quality gates (Phase 2: binding-constraint pattern).
///
/// Each dimension must pass. Any failing dimension causes rejection with a
/// structured reason. This is the "theorist as binding constraint" pattern from
/// the AutoResearch self-play paper — the weakest dimension determines the outcome.

/// Maximum total changes (params + prose + tools + filter) a candidate may carry.
/// Beyond this, the mutation is flagged as parameter explosion — too many knobs
/// changed at once to attribute improvement to any single hypothesis.
const MAX_TOTAL_CHANGES: usize = 8;

/// Check simplicity: the candidate must not change too many things at once.
/// Parameter explosion without clear justification is an overfitting smell.
pub fn check_dimension_simplicity(diff: &MutationDiff) -> GateVerdict {
    let total = diff.params.len()
        + diff.prose.len()
        + diff.tools.added.len()
        + diff.tools.removed.len()
        + diff.filter.len();
    if total > MAX_TOTAL_CHANGES {
        GateVerdict::Fail {
            reason: format!(
                "simplicity: candidate changes {total} items (params={p}, prose={r}, \
                 tools={t}, filter={f}) exceeding the {MAX_TOTAL_CHANGES}-item limit. \
                 Split the experiment into smaller, focused changes with clear hypotheses.",
                p = diff.params.len(),
                r = diff.prose.len(),
                t = diff.tools.added.len() + diff.tools.removed.len(),
                f = diff.filter.len(),
            ),
        }
    } else {
        GateVerdict::Pass
    }
}

use crate::autooptimizer::config::RegimeSide;
use crate::autooptimizer::lineage::LineageStatus;

/// Aggregate per-regime gate verdicts into a lineage status per the anti-overfit
/// rule: Kept (Active) iff a Bull AND a BearOrShock regime both pass; Suspect
/// (Quarantined) if any regime passes but not the both-sides rule; Dropped
/// (Rejected) if no regime passes.
pub fn aggregate_regime_verdicts(results: &[(RegimeSide, GateVerdict)]) -> LineageStatus {
    let passed = |s: RegimeSide| {
        results
            .iter()
            .any(|(side, v)| *side == s && matches!(v, GateVerdict::Pass))
    };
    let any_pass = results.iter().any(|(_, v)| matches!(v, GateVerdict::Pass));
    if passed(RegimeSide::Bull) && passed(RegimeSide::BearOrShock) {
        LineageStatus::Active
    } else if any_pass {
        LineageStatus::Quarantined
    } else {
        LineageStatus::Rejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregation_kept_needs_bull_and_bear() {
        use crate::autooptimizer::config::RegimeSide::*;
        let pass = GateVerdict::Pass;
        let fail = GateVerdict::Fail { reason: "neg".into() };
        assert_eq!(
            aggregate_regime_verdicts(&[(Bull, pass.clone()), (BearOrShock, pass.clone())]),
            LineageStatus::Active
        );
        assert_eq!(
            aggregate_regime_verdicts(&[(Bull, pass.clone()), (BearOrShock, fail.clone())]),
            LineageStatus::Quarantined
        );
        assert_eq!(
            aggregate_regime_verdicts(&[(Bull, fail.clone()), (BearOrShock, fail.clone())]),
            LineageStatus::Rejected
        );
        assert_eq!(
            aggregate_regime_verdicts(&[(Bull, pass.clone()), (Chop, pass.clone())]),
            LineageStatus::Quarantined
        );
    }
}
