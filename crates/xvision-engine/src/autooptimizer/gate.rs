use crate::eval::MetricsSummary;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

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
/// `min_improvement` is the pre-committed ε threshold (operator flag: `--min-improvement`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateInput {
    pub parent_day_metrics: MetricsSummary,
    pub child_day_metrics: MetricsSummary,
    pub parent_untouched_metrics: MetricsSummary,
    pub child_untouched_metrics: MetricsSummary,
    pub min_improvement: f64,
    /// F24: which metric to optimize. Defaults to `Sharpe` (serde default) so
    /// existing call sites/fixtures that omit it keep the prior behavior.
    #[serde(default)]
    pub objective: Objective,
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

/// Deterministic numeric gate for AR-1 mutations.
///
/// Passes only when all three hold:
/// 1. Δ Sharpe (day window)       ≥ `min_improvement`
/// 2. Δ Sharpe (untouched window) ≥ `min_improvement`  ← the stricter check
/// 3. Child worst drawdown        ≤ parent worst drawdown × 1.5
///
/// Pure function — same inputs always yield the same verdict.
pub fn evaluate(input: &GateInput) -> GateVerdict {
    debug_assert!(
        input.min_improvement.is_finite(),
        "min_improvement must be finite"
    );

    // F24: evaluate the operator-selected objective on BOTH windows (the
    // held-out discipline is preserved — a candidate must improve on the day
    // window AND the untouched window). `oriented_value` makes larger always
    // better, so the same delta comparison works for every objective.
    let obj = input.objective;
    let delta_day =
        obj.oriented_value(&input.child_day_metrics) - obj.oriented_value(&input.parent_day_metrics);
    let delta_untouched = obj.oriented_value(&input.child_untouched_metrics)
        - obj.oriented_value(&input.parent_untouched_metrics);

    if delta_day < input.min_improvement - CMP_EPS {
        return GateVerdict::Fail {
            reason: format!(
                "today's score ({}) improved by {delta_day:.6} \
                 but minimum-improvement threshold is {:.6}",
                obj.label(),
                input.min_improvement
            ),
        };
    }

    if delta_untouched < input.min_improvement - CMP_EPS {
        return GateVerdict::Fail {
            reason: format!(
                "baseline-untouched-score ({}) improved by {delta_untouched:.6} \
                 but minimum-improvement threshold is {:.6}",
                obj.label(),
                input.min_improvement
            ),
        };
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
            return GateVerdict::Fail {
                reason: format!(
                    "max drawdown deteriorated: candidate worst {child_worst:.4}% exceeds \
                     {DRAWDOWN_DETERIORATION_FACTOR}× parent worst {parent_worst:.4}%"
                ),
            };
        }
    }

    GateVerdict::Pass
}
