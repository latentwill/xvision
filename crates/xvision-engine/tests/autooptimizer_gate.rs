use xvision_engine::autooptimizer::gate::{evaluate, GateInput, GateVerdict, Objective};
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
        holdout_min_improvement: min_improvement,
        objective: Objective::default(),
    }
}

#[test]
fn holdout_threshold_is_independent_from_day_threshold() {
    let input = GateInput {
        parent_day_metrics: metrics(1.0, -10.0),
        child_day_metrics: metrics(1.12, -10.0),
        parent_untouched_metrics: metrics(0.8, -8.0),
        child_untouched_metrics: metrics(0.806, -8.0),
        min_improvement: 0.10,
        holdout_min_improvement: 0.005,
        objective: Objective::default(),
    };

    assert!(matches!(evaluate(&input), GateVerdict::Pass));
}

#[test]
fn legacy_gate_input_json_defaults_holdout_threshold_to_day_threshold() {
    let json = serde_json::json!({
        "parent_day_metrics": metrics(1.0, -10.0),
        "child_day_metrics": metrics(1.2, -10.0),
        "parent_untouched_metrics": metrics(0.8, -8.0),
        "child_untouched_metrics": metrics(1.0, -8.0),
        "min_improvement": 0.1,
        "objective": "sharpe"
    });

    let input: GateInput = serde_json::from_value(json).expect("legacy gate input should deserialize");

    assert_eq!(input.holdout_min_improvement, input.min_improvement);
}

#[test]
fn holdout_failure_message_uses_holdout_threshold() {
    let input = GateInput {
        parent_day_metrics: metrics(1.0, -10.0),
        child_day_metrics: metrics(1.12, -10.0),
        parent_untouched_metrics: metrics(0.8, -8.0),
        child_untouched_metrics: metrics(0.803, -8.0),
        min_improvement: 0.10,
        holdout_min_improvement: 0.005,
        objective: Objective::default(),
    };

    let GateVerdict::Fail { reason } = evaluate(&input) else {
        panic!("expected holdout failure");
    };

    assert!(reason.contains("holdout minimum-improvement threshold is 0.005000"));
}

// ── F24: configurable objective ──────────────────────────────────────────────

fn m_full(sharpe: f64, total_return_pct: f64, win_rate: f64, max_drawdown_pct: f64) -> MetricsSummary {
    MetricsSummary {
        sharpe,
        total_return_pct,
        win_rate,
        max_drawdown_pct,
        ..MetricsSummary::default()
    }
}

/// F24: with the `total_return` objective, a candidate that improves RETURN on
/// both windows passes even though its Sharpe is flat (the old gate would have
/// failed it — it only looked at Sharpe).
#[test]
fn total_return_objective_gates_on_return_not_sharpe() {
    let input = GateInput {
        parent_day_metrics: m_full(1.0, 5.0, 0.5, -10.0),
        child_day_metrics: m_full(1.0, 9.0, 0.5, -10.0),
        parent_untouched_metrics: m_full(1.0, 4.0, 0.5, -8.0),
        child_untouched_metrics: m_full(1.0, 7.0, 0.5, -8.0),
        min_improvement: 1.0,
        holdout_min_improvement: 1.0,
        objective: Objective::TotalReturn,
    };
    assert!(matches!(evaluate(&input), GateVerdict::Pass));
}

/// F24: with the `max_drawdown` objective, a candidate that REDUCES drawdown on
/// both windows passes (improvement = parent - child, the minimize direction),
/// and the non-objective drawdown guard is skipped.
#[test]
fn max_drawdown_objective_rewards_reducing_drawdown() {
    let input = GateInput {
        // child drawdown smaller (closer to 0) on both windows.
        parent_day_metrics: m_full(1.0, 5.0, 0.5, -20.0),
        child_day_metrics: m_full(1.0, 5.0, 0.5, -12.0),
        parent_untouched_metrics: m_full(1.0, 5.0, 0.5, -18.0),
        child_untouched_metrics: m_full(1.0, 5.0, 0.5, -10.0),
        holdout_min_improvement: 1.0,
        min_improvement: 1.0,
        objective: Objective::MaxDrawdown,
    };
    assert!(matches!(evaluate(&input), GateVerdict::Pass));
}

/// F24: the held-out discipline still holds per objective — improving RETURN on
/// the day window but NOT the untouched window is rejected.
#[test]
fn total_return_objective_requires_both_windows() {
    let input = GateInput {
        parent_day_metrics: m_full(1.0, 5.0, 0.5, -10.0),
        child_day_metrics: m_full(1.0, 9.0, 0.5, -10.0),
        parent_untouched_metrics: m_full(1.0, 4.0, 0.5, -8.0),
        holdout_min_improvement: 1.0,
        child_untouched_metrics: m_full(1.0, 4.0, 0.5, -8.0), // no untouched improvement
        min_improvement: 1.0,
        objective: Objective::TotalReturn,
    };
    assert!(matches!(evaluate(&input), GateVerdict::Fail { .. }));
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

/// B16: when the day delta fails, the reason must STILL surface the
/// baseline-untouched-score (holdout) number so a near-miss on holdout is not
/// invisible. Day delta = 0.05 < 0.1 (fail); untouched delta = 0.3 >= 0.1 (pass).
#[test]
fn fail_day_also_reports_holdout() {
    let i = make_input(1.0, 1.05, 0.8, 1.1, -10.0, -10.0, -8.0, -8.0, 0.1);
    let GateVerdict::Fail { reason } = evaluate(&i) else {
        panic!("expected Fail, got Pass");
    };
    assert!(
        reason.contains("today's score"),
        "expected 'today's score' in reason, got: {reason}"
    );
    assert!(
        reason.contains("baseline-untouched-score"),
        "expected 'baseline-untouched-score' in reason even though holdout passed, got: {reason}"
    );
    assert!(
        !reason.contains('\n'),
        "reason must not contain newlines, got: {reason}"
    );
}

/// B16: multiple failing conditions are ALL reported. Day delta fails AND
/// drawdown blows up; reason names both.
#[test]
fn fail_reports_all_failing_conditions() {
    // day delta 0.05 < 0.1 (fail), untouched delta 0.3 (pass),
    // child day drawdown -16.5% > 1.5 × 10% = 15% (drawdown fail).
    let i = make_input(1.0, 1.05, 0.8, 1.1, -10.0, -16.5, -8.0, -8.0, 0.1);
    let GateVerdict::Fail { reason } = evaluate(&i) else {
        panic!("expected Fail, got Pass");
    };
    assert!(
        reason.contains("today's score"),
        "expected 'today's score' in reason, got: {reason}"
    );
    assert!(
        reason.contains("drawdown"),
        "expected 'drawdown' in reason, got: {reason}"
    );
    assert!(
        !reason.contains('\n'),
        "reason must not contain newlines, got: {reason}"
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
