use crate::eval::MetricsSummary;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Factor by which the child's worst drawdown may exceed the parent's before rejection.
const DRAWDOWN_DETERIORATION_FACTOR: f64 = 1.5;

/// Tolerance applied to boundary comparisons.
/// Prevents identical inputs from flipping at the threshold due to FP rounding.
const CMP_EPS: f64 = 1e-9;

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
    debug_assert!(input.min_improvement.is_finite(), "min_improvement must be finite");

    let delta_day = input.child_day_metrics.sharpe - input.parent_day_metrics.sharpe;
    let delta_untouched =
        input.child_untouched_metrics.sharpe - input.parent_untouched_metrics.sharpe;

    if delta_day < input.min_improvement - CMP_EPS {
        return GateVerdict::Fail {
            reason: format!(
                "today's score improved by {delta_day:.6} \
                 but minimum-improvement threshold is {:.6}",
                input.min_improvement
            ),
        };
    }

    if delta_untouched < input.min_improvement - CMP_EPS {
        return GateVerdict::Fail {
            reason: format!(
                "baseline-untouched-score improved by {delta_untouched:.6} \
                 but minimum-improvement threshold is {:.6}",
                input.min_improvement
            ),
        };
    }

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

    GateVerdict::Pass
}
