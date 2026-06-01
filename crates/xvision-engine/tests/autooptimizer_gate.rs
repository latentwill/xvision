use xvision_engine::autooptimizer::gate::{evaluate, GateInput, GateVerdict};
use xvision_engine::eval::MetricsSummary;

fn metrics(sharpe: f64, max_drawdown_pct: f64) -> MetricsSummary {
    MetricsSummary {
        sharpe,
        max_drawdown_pct,
        ..MetricsSummary::default()
    }
}

fn make_input(
    parent_day_sharpe: f64,
    child_day_sharpe: f64,
    parent_untouched_sharpe: f64,
    child_untouched_sharpe: f64,
    parent_day_dd: f64,
    child_day_dd: f64,
    parent_untouched_dd: f64,
    child_untouched_dd: f64,
    min_improvement: f64,
) -> GateInput {
    GateInput {
        parent_day_metrics: metrics(parent_day_sharpe, parent_day_dd),
        child_day_metrics: metrics(child_day_sharpe, child_day_dd),
        parent_untouched_metrics: metrics(parent_untouched_sharpe, parent_untouched_dd),
        child_untouched_metrics: metrics(child_untouched_sharpe, child_untouched_dd),
        min_improvement,
    }
}

/// Both Sharpe deltas clear the threshold; drawdown is stable → Pass.
#[test]
fn pass_case() {
    let i = make_input(1.0, 1.2, 0.8, 1.0, -10.0, -11.0, -8.0, -9.0, 0.1);
    assert!(matches!(evaluate(&i), GateVerdict::Pass));
}

/// Day delta = 0.05 < min_improvement 0.1 → Fail on today's score.
#[test]
fn fail_day_case() {
    let i = make_input(1.0, 1.05, 0.8, 1.0, -10.0, -10.0, -8.0, -8.0, 0.1);
    let GateVerdict::Fail { reason } = evaluate(&i) else {
        panic!("expected Fail, got Pass");
    };
    assert!(
        reason.contains("today's score"),
        "expected 'today's score' in reason, got: {reason}"
    );
}

/// Day delta passes (0.2); untouched delta = 0.05 < 0.1 → Fail on baseline-untouched-score.
/// This is the primary gate: untouched-period regression blocks promotion.
#[test]
fn fail_untouched_case() {
    let i = make_input(1.0, 1.2, 0.8, 0.85, -10.0, -10.0, -8.0, -8.0, 0.1);
    let GateVerdict::Fail { reason } = evaluate(&i) else {
        panic!("expected Fail, got Pass");
    };
    assert!(
        reason.contains("baseline-untouched-score"),
        "expected 'baseline-untouched-score' in reason, got: {reason}"
    );
}

/// Both Sharpe deltas pass; child day drawdown -16.5% > 1.5 × |-10%| = 15% → Fail on drawdown.
#[test]
fn fail_drawdown_case() {
    let i = make_input(1.0, 1.2, 0.8, 1.0, -10.0, -16.5, -8.0, -8.0, 0.1);
    let GateVerdict::Fail { reason } = evaluate(&i) else {
        panic!("expected Fail, got Pass");
    };
    assert!(
        reason.contains("drawdown"),
        "expected 'drawdown' in reason, got: {reason}"
    );
}

/// Sharpe delta exactly at min_improvement must pass (CMP_EPS tolerance protects the boundary).
#[test]
fn pass_at_exact_threshold() {
    // delta_day = 0.1 exactly; delta_untouched = 0.1 exactly
    let i = make_input(1.0, 1.1, 0.8, 0.9, -10.0, -10.0, -8.0, -8.0, 0.1);
    assert!(
        matches!(evaluate(&i), GateVerdict::Pass),
        "delta exactly at threshold should pass"
    );
}

/// Child drawdown exactly at 1.5× parent must pass (CMP_EPS guards the upper boundary).
#[test]
fn pass_drawdown_at_exact_limit() {
    // parent_worst = 10.0; child_worst = 15.0 (exactly 1.5×) → must Pass
    let i = make_input(1.0, 1.2, 0.8, 1.0, -10.0, -15.0, -8.0, -8.0, 0.1);
    assert!(
        matches!(evaluate(&i), GateVerdict::Pass),
        "child drawdown exactly at 1.5× parent should pass"
    );
}

/// Ten consecutive calls with identical inputs must return the exact same verdict.
#[test]
fn determinism() {
    let i = make_input(1.0, 1.2, 0.8, 1.0, -10.0, -11.0, -8.0, -9.0, 0.1);
    let first = evaluate(&i);
    for _ in 0..9_usize {
        match (evaluate(&i), &first) {
            (GateVerdict::Pass, GateVerdict::Pass) => {}
            (GateVerdict::Fail { reason }, GateVerdict::Fail { reason: first_reason }) => {
                assert_eq!(reason, *first_reason, "reason string changed across calls");
            }
            _ => panic!("verdict changed between calls"),
        }
    }
}
