//! Phase 8.4 — anti-overfitting gate.
//!
//! ## Epistemic role (preserved in v1)
//! The gate truthfully signals whether Δ-Sharpe confidence intervals include
//! zero across all market regimes. A positive signal (all CIs above zero)
//! provides evidence against overfitting to a particular regime. The gate's
//! *scheduling* role — blocking forward paper-trading deployment — is relaxed
//! in v1: the verdict is reportable but not blocking.
//!
//! ## Re-tightening trigger
//! The scheduling block should be re-enabled when any v2 self-improvement loop
//! begins iterating over vector configurations (at that point, a regime-only
//! pass could be the result of in-sample optimisation, and the gate becomes the
//! primary circuit-breaker).

use serde::{Deserialize, Serialize};
use xianvec_core::trading::Regime;

use crate::metrics::PreCommittedMetrics;

// ---------------------------------------------------------------------------
// Verdict
// ---------------------------------------------------------------------------

/// Anti-overfit gate output.
///
/// The variant names are historical from the original 2-regime design; the
/// gate generalises cleanly to >2 regimes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GateVerdict {
    /// All sampled regimes show positive Δ-Sharpe with CI entirely above zero.
    /// Strong evidence that the arm difference is not regime-specific.
    PassesBothRegimes,
    /// Some regimes pass, some fail. `winning_regime` is the stratum with the
    /// highest `point_estimate`; `losing_regime` is the stratum with the
    /// lowest (most negative or least positive) `ci_low`.
    SingleRegimeEvidence {
        winning_regime: Regime,
        losing_regime: Regime,
    },
    /// All sampled regimes fail the positive-CI criterion.
    Fails { regimes: Vec<Regime> },
}

// ---------------------------------------------------------------------------
// Core function
// ---------------------------------------------------------------------------

/// Classify the metrics into one of the three gate verdicts.
///
/// "Positive evidence" for a regime: `point_estimate > 0 AND ci_low > 0`.
/// "Failing" otherwise.
///
/// If `regime_stratified` is empty (no regimes were encountered), the function
/// returns `PassesBothRegimes` — vacuously true, consistent with the gate being
/// evidence-based rather than requirement-based in v1.
pub fn anti_overfit_verdict(metrics: &PreCommittedMetrics) -> GateVerdict {
    if metrics.regime_stratified.is_empty() {
        return GateVerdict::PassesBothRegimes;
    }

    let mut passing: Vec<(Regime, f32)> = Vec::new();
    let mut failing: Vec<(Regime, f32)> = Vec::new();

    for (&regime, rm) in &metrics.regime_stratified {
        let bs = &rm.delta_sharpe;
        if bs.point_estimate > 0.0 && bs.ci_low > 0.0 {
            passing.push((regime, bs.point_estimate));
        } else {
            failing.push((regime, bs.ci_low));
        }
    }

    if failing.is_empty() {
        // All regimes pass
        return GateVerdict::PassesBothRegimes;
    }

    if passing.is_empty() {
        // All regimes fail
        let regimes: Vec<Regime> = failing.into_iter().map(|(r, _)| r).collect();
        return GateVerdict::Fails { regimes };
    }

    // Mixed: find the strongest passer and the weakest failer
    let winning_regime = passing
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|&(r, _)| r)
        .expect("passing is non-empty");

    let losing_regime = failing
        .iter()
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|&(r, _)| r)
        .expect("failing is non-empty");

    GateVerdict::SingleRegimeEvidence {
        winning_regime,
        losing_regime,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use xianvec_core::trading::Regime;

    use crate::bootstrap::BootstrapResult;
    use crate::metrics::{PreCommittedMetrics, RegimeMetrics};

    fn make_bootstrap(pe: f32, ci_low: f32, ci_high: f32) -> BootstrapResult {
        BootstrapResult {
            point_estimate: pe,
            ci_low,
            ci_high,
            n_resamples: 100,
            block_size: None,
        }
    }

    fn make_regime_metrics(pe: f32, ci_low: f32, ci_high: f32) -> RegimeMetrics {
        RegimeMetrics {
            n_setups: 10,
            delta_sharpe: make_bootstrap(pe, ci_low, ci_high),
            winner: Some("arm_a".into()),
        }
    }

    fn empty_metrics(regime_stratified: HashMap<Regime, RegimeMetrics>) -> PreCommittedMetrics {
        PreCommittedMetrics {
            delta_sharpe: make_bootstrap(0.5, 0.1, 0.9),
            max_drawdown_pct: std::collections::BTreeMap::new(),
            profit_factor: std::collections::BTreeMap::new(),
            win_rate: std::collections::BTreeMap::new(),
            decision_divergence_rate: 0.0,
            regime_stratified,
        }
    }

    // -----------------------------------------------------------------------
    // PassesBothRegimes: all regimes have pe > 0 and ci_low > 0
    // -----------------------------------------------------------------------

    #[test]
    fn passes_both_regimes_when_all_positive() {
        let mut regimes = HashMap::new();
        regimes.insert(Regime::Bull, make_regime_metrics(0.8, 0.2, 1.4));
        regimes.insert(Regime::Bear, make_regime_metrics(0.5, 0.1, 0.9));
        let metrics = empty_metrics(regimes);
        assert!(matches!(
            anti_overfit_verdict(&metrics),
            GateVerdict::PassesBothRegimes
        ));
    }

    #[test]
    fn passes_when_regime_stratified_is_empty() {
        let metrics = empty_metrics(HashMap::new());
        assert!(matches!(
            anti_overfit_verdict(&metrics),
            GateVerdict::PassesBothRegimes
        ));
    }

    // -----------------------------------------------------------------------
    // Fails: all regimes failing
    // -----------------------------------------------------------------------

    #[test]
    fn fails_when_all_regimes_negative_pe() {
        let mut regimes = HashMap::new();
        regimes.insert(Regime::Bull, make_regime_metrics(-0.3, -0.8, 0.1));
        regimes.insert(Regime::Bear, make_regime_metrics(-0.5, -1.0, 0.0));
        let metrics = empty_metrics(regimes);
        match anti_overfit_verdict(&metrics) {
            GateVerdict::Fails { regimes } => {
                assert_eq!(regimes.len(), 2, "both regimes should fail");
            }
            other => panic!("expected Fails, got {other:?}"),
        }
    }

    #[test]
    fn fails_when_ci_low_zero_even_if_pe_positive() {
        // pe > 0 but ci_low == 0 → not "positive evidence"
        let mut regimes = HashMap::new();
        regimes.insert(Regime::Bull, make_regime_metrics(0.5, 0.0, 1.0));
        let metrics = empty_metrics(regimes);
        assert!(matches!(
            anti_overfit_verdict(&metrics),
            GateVerdict::Fails { .. }
        ));
    }

    #[test]
    fn fails_when_pe_zero() {
        let mut regimes = HashMap::new();
        regimes.insert(Regime::Chop, make_regime_metrics(0.0, -0.1, 0.5));
        let metrics = empty_metrics(regimes);
        assert!(matches!(
            anti_overfit_verdict(&metrics),
            GateVerdict::Fails { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // SingleRegimeEvidence: mixed pass/fail
    // -----------------------------------------------------------------------

    #[test]
    fn single_regime_evidence_on_mixed() {
        let mut regimes = HashMap::new();
        // Bull passes: pe=0.8, ci_low=0.2
        regimes.insert(Regime::Bull, make_regime_metrics(0.8, 0.2, 1.4));
        // Bear fails: pe=-0.3, ci_low=-0.9
        regimes.insert(Regime::Bear, make_regime_metrics(-0.3, -0.9, 0.2));
        let metrics = empty_metrics(regimes);
        match anti_overfit_verdict(&metrics) {
            GateVerdict::SingleRegimeEvidence {
                winning_regime,
                losing_regime,
            } => {
                assert_eq!(winning_regime, Regime::Bull, "Bull should be the winner");
                assert_eq!(losing_regime, Regime::Bear, "Bear should be the loser");
            }
            other => panic!("expected SingleRegimeEvidence, got {other:?}"),
        }
    }

    #[test]
    fn single_regime_winner_is_highest_pe() {
        let mut regimes = HashMap::new();
        // Two passing: HighVol has higher pe
        regimes.insert(Regime::LowVol, make_regime_metrics(0.3, 0.05, 0.6));
        regimes.insert(Regime::HighVol, make_regime_metrics(1.2, 0.4, 2.0));
        // One failing
        regimes.insert(Regime::Chop, make_regime_metrics(-0.1, -0.5, 0.3));
        let metrics = empty_metrics(regimes);
        match anti_overfit_verdict(&metrics) {
            GateVerdict::SingleRegimeEvidence { winning_regime, .. } => {
                assert_eq!(winning_regime, Regime::HighVol, "HighVol has highest pe");
            }
            other => panic!("expected SingleRegimeEvidence, got {other:?}"),
        }
    }
}
