use crate::eval::MetricsSummary;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use xvision_eval::bootstrap::paired_bootstrap_sharpe_delta;

/// Factor by which the child's worst drawdown may exceed the parent's before rejection.
const DRAWDOWN_DETERIORATION_FACTOR: f64 = 1.5;

/// Tolerance applied to boundary comparisons.
/// Prevents identical inputs from flipping at the threshold due to FP rounding.
const CMP_EPS: f64 = 1e-9;

pub const DEFAULT_MIN_TRADES_PER_WINDOW: u32 = 10;
pub const DEFAULT_GATE_BOOTSTRAP_RESAMPLES: usize = 500;
pub const DEFAULT_GATE_BOOTSTRAP_PERIODS_PER_YEAR: f32 = 365.0 * 24.0;
pub const DEFAULT_GATE_BOOTSTRAP_SEED: u64 = 0xA076_1D64_78BD_642F;

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
    #[serde(default)]
    pub min_trades_per_window: u32,
    #[serde(default)]
    pub edge_gate_enabled: bool,
    #[serde(default)]
    pub require_return_series: bool,
    #[serde(default)]
    pub bootstrap: GateBootstrapConfig,
    #[serde(default)]
    pub day_returns: Option<PairedReturnSeries>,
    #[serde(default)]
    pub untouched_returns: Option<PairedReturnSeries>,
    #[serde(default)]
    pub edge_returns: Option<PairedReturnSeries>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedReturnSeries {
    pub candidate: Vec<f32>,
    pub baseline: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateBootstrapConfig {
    pub n_resamples: usize,
    pub block_size: Option<usize>,
    pub periods_per_year: f32,
    pub seed: u64,
}

impl Default for GateBootstrapConfig {
    fn default() -> Self {
        Self {
            n_resamples: DEFAULT_GATE_BOOTSTRAP_RESAMPLES,
            block_size: None,
            periods_per_year: DEFAULT_GATE_BOOTSTRAP_PERIODS_PER_YEAR,
            seed: DEFAULT_GATE_BOOTSTRAP_SEED,
        }
    }
}

impl GateInput {
    pub fn aggregate_only(
        parent_day_metrics: MetricsSummary,
        child_day_metrics: MetricsSummary,
        parent_untouched_metrics: MetricsSummary,
        child_untouched_metrics: MetricsSummary,
        min_improvement: f64,
        objective: Objective,
    ) -> Self {
        Self {
            parent_day_metrics,
            child_day_metrics,
            parent_untouched_metrics,
            child_untouched_metrics,
            min_improvement,
            objective,
            min_trades_per_window: 0,
            edge_gate_enabled: false,
            require_return_series: false,
            bootstrap: GateBootstrapConfig::default(),
            day_returns: None,
            untouched_returns: None,
            edge_returns: None,
        }
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

    // B16: evaluate EVERY condition before returning so the rejection reason
    // surfaces all failing checks, not just the first. A near-miss on the
    // untouched (holdout) window must stay visible even when the day window is
    // the one that tripped the gate. Order is deterministic: day, untouched,
    // drawdown — so the reason string is byte-stable for identical inputs.
    let mut failures: Vec<String> = Vec::new();

    if input.min_trades_per_window > 0 {
        let day_trades = input.child_day_metrics.n_trades;
        let untouched_trades = input.child_untouched_metrics.n_trades;
        if day_trades < input.min_trades_per_window {
            failures.push(format!(
                "today's trade count ({day_trades}) is below min-trades-per-window {}",
                input.min_trades_per_window
            ));
        }
        if untouched_trades < input.min_trades_per_window {
            failures.push(format!(
                "baseline-untouched trade count ({untouched_trades}) is below min-trades-per-window {}",
                input.min_trades_per_window
            ));
        }
    }

    let day_failed = delta_day < input.min_improvement - CMP_EPS;
    if day_failed {
        failures.push(format!(
            "today's score ({}) improved by {delta_day:.6} \
             but minimum-improvement threshold is {:.6}",
            obj.label(),
            input.min_improvement
        ));
    }

    let untouched_failed = delta_untouched < input.min_improvement - CMP_EPS;
    if untouched_failed {
        failures.push(format!(
            "baseline-untouched-score ({}) improved by {delta_untouched:.6} \
             but minimum-improvement threshold is {:.6}",
            obj.label(),
            input.min_improvement
        ));
    } else if day_failed {
        // The day check failed but the holdout passed: surface the holdout delta
        // anyway so a holdout near-miss (or near-pass) is never invisible.
        failures.push(format!(
            "baseline-untouched-score ({}) improved by {delta_untouched:.6} \
             (cleared the {:.6} minimum)",
            obj.label(),
            input.min_improvement
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

    append_ci_failure(
        "today's paired-return CI-low",
        &input.day_returns,
        input.require_return_series,
        &input.bootstrap,
        input.bootstrap.seed,
        &mut failures,
    );
    append_ci_failure(
        "baseline-untouched paired-return CI-low",
        &input.untouched_returns,
        input.require_return_series,
        &input.bootstrap,
        input.bootstrap.seed ^ 0x9E37_79B9_7F4A_7C15,
        &mut failures,
    );
    if input.edge_gate_enabled {
        append_ci_failure(
            "edge-over-random CI-low",
            &input.edge_returns,
            input.require_return_series,
            &input.bootstrap,
            input.bootstrap.seed ^ 0xC2B2_AE3D_27D4_EB4F,
            &mut failures,
        );
    }

    if failures.is_empty() {
        GateVerdict::Pass
    } else {
        GateVerdict::Fail {
            reason: failures.join("; "),
        }
    }
}

fn append_ci_failure(
    label: &str,
    pair: &Option<PairedReturnSeries>,
    require_return_series: bool,
    cfg: &GateBootstrapConfig,
    seed: u64,
    failures: &mut Vec<String>,
) {
    let Some(pair) = pair else {
        if require_return_series {
            failures.push(format!("{label} missing paired return series"));
        }
        return;
    };
    match paired_bootstrap_sharpe_delta(
        &pair.candidate,
        &pair.baseline,
        cfg.n_resamples,
        cfg.block_size,
        cfg.periods_per_year,
        seed,
    ) {
        Ok(result) if result.ci_low > 0.0 => {}
        Ok(result) => failures.push(format!(
            "{label} must be > 0.000000 (got {:.6}, point {:.6})",
            result.ci_low, result.point_estimate
        )),
        Err(e) => failures.push(format!("{label} could not be computed: {e}")),
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
