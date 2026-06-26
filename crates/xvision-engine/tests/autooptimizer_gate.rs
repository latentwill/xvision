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
        parent_n_trades: 0,
        child_n_trades: 0,
        min_trade_retention_ratio: 0.5,
        min_realized_return_ratio: 0.0,
    }
}

#[test]
fn holdout_threshold_is_independent_from_day_threshold() {
    let input = GateInput { parent_day_metrics: metrics(1.0, -10.0),
    child_day_metrics: metrics(1.12, -10.0),
    parent_untouched_metrics: metrics(0.8, -8.0),
    child_untouched_metrics: metrics(0.806, -8.0),
    min_improvement: 0.10,
    holdout_min_improvement: 0.005, objective: Objective::default(), parent_n_trades: 0, child_n_trades: 0, min_trade_retention_ratio: 0.5, min_realized_return_ratio: 0.0 };

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
    let input = GateInput { parent_day_metrics: metrics(1.0, -10.0),
    child_day_metrics: metrics(1.12, -10.0),
    parent_untouched_metrics: metrics(0.8, -8.0),
    child_untouched_metrics: metrics(0.803, -8.0),
    min_improvement: 0.10,
    holdout_min_improvement: 0.005, objective: Objective::default(), parent_n_trades: 0, child_n_trades: 0, min_trade_retention_ratio: 0.5, min_realized_return_ratio: 0.0 };

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
    let input = GateInput { parent_day_metrics: m_full(1.0, 5.0, 0.5, -10.0),
    child_day_metrics: m_full(1.0, 9.0, 0.5, -10.0),
    parent_untouched_metrics: m_full(1.0, 4.0, 0.5, -8.0),
    child_untouched_metrics: m_full(1.0, 7.0, 0.5, -8.0),
    min_improvement: 1.0,
    holdout_min_improvement: 1.0, objective: Objective::TotalReturn, parent_n_trades: 0, child_n_trades: 0, min_trade_retention_ratio: 0.5, min_realized_return_ratio: 0.0 };
    assert!(matches!(evaluate(&input), GateVerdict::Pass));
}

/// F24: with the `max_drawdown` objective, a candidate that REDUCES drawdown on
/// both windows passes (improvement = parent - child, the minimize direction),
/// and the non-objective drawdown guard is skipped.
#[test]
fn max_drawdown_objective_rewards_reducing_drawdown() {
    let input = GateInput { // child drawdown smaller (closer to 0) on both windows.
    parent_day_metrics: m_full(1.0, 5.0, 0.5, -20.0),
    child_day_metrics: m_full(1.0, 5.0, 0.5, -12.0),
    parent_untouched_metrics: m_full(1.0, 5.0, 0.5, -18.0),
    child_untouched_metrics: m_full(1.0, 5.0, 0.5, -10.0),
    holdout_min_improvement: 1.0,
    min_improvement: 1.0, objective: Objective::MaxDrawdown, parent_n_trades: 0, child_n_trades: 0, min_trade_retention_ratio: 0.5, min_realized_return_ratio: 0.0 };
    assert!(matches!(evaluate(&input), GateVerdict::Pass));
}

/// F24: the held-out discipline still holds per objective — improving RETURN on
/// the day window but NOT the untouched window is rejected.
#[test]
fn total_return_objective_requires_both_windows() {
    let input = GateInput { parent_day_metrics: m_full(1.0, 5.0, 0.5, -10.0),
    child_day_metrics: m_full(1.0, 9.0, 0.5, -10.0),
    parent_untouched_metrics: m_full(1.0, 4.0, 0.5, -8.0),
    holdout_min_improvement: 1.0,
    child_untouched_metrics: m_full(1.0, 4.0, 0.5, -8.0), // no untouched improvement
    min_improvement: 1.0, objective: Objective::TotalReturn, parent_n_trades: 0, child_n_trades: 0, min_trade_retention_ratio: 0.5, min_realized_return_ratio: 0.0 };
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

// ── Min-trades gate ──────────────────────────────────────────────────────

fn metrics_with_trades(sharpe: f64, max_drawdown_pct: f64, n_trades: u32) -> MetricsSummary {
    MetricsSummary {
        sharpe,
        max_drawdown_pct,
        n_trades,
        ..MetricsSummary::default()
    }
}

fn make_input_with_trades(
    parent_sharpe: f64,
    child_sharpe: f64,
    parent_trades: u32,
    child_trades: u32,
) -> GateInput {
    GateInput {
        parent_day_metrics: metrics_with_trades(parent_sharpe, -10.0, parent_trades),
        child_day_metrics: metrics_with_trades(child_sharpe, -10.0, child_trades),
        parent_untouched_metrics: metrics_with_trades(parent_sharpe, -8.0, parent_trades),
        child_untouched_metrics: metrics_with_trades(child_sharpe, -8.0, child_trades),
        min_improvement: 0.1,
        holdout_min_improvement: 0.1,
        objective: Objective::default(),
        parent_n_trades: parent_trades,
        child_n_trades: child_trades,
        min_trade_retention_ratio: 0.5,
        min_realized_return_ratio: 0.0,
    }
}

/// A 0-trade child that beats a negative-Sharpe parent must be rejected.
#[test]
fn reject_zero_trade_child_even_with_sharpe_improvement() {
    // Parent has -2.0 Sharpe (losing strategy), child has 0.0 Sharpe (0 trades).
    // Without the min-trades gate, this would pass (0.0 > -2.0, delta 2.0 > 0.1).
    let input = make_input_with_trades(-2.0, 0.0, 10, 0);
    let GateVerdict::Fail { reason } = evaluate(&input) else {
        panic!("expected rejection of 0-trade child");
    };
    assert!(reason.contains("insufficient trades"), "reason: {reason}");
    assert!(reason.contains("child executed 0"), "reason: {reason}");
    assert!(reason.contains("required 5"), "reason: {reason}");
    assert!(reason.contains("50% of parent's 10"), "reason: {reason}");
}

/// A child retaining enough trades should pass when Sharpe improves.
#[test]
fn accept_child_retaining_enough_parent_trades() {
    let input = make_input_with_trades(-2.0, 1.0, 10, 6);
    // 6 >= max(1, floor(10 * 0.5)) = 5 — passes trade check.
    // Delta Sharpe = 1.0 - (-2.0) = 3.0 > 0.1 — passes improvement check.
    assert!(matches!(evaluate(&input), GateVerdict::Pass));
}

/// Sentinel (0, 0) trade counts must skip the trade check entirely.
#[test]
fn sentinel_zero_trades_skips_check() {
    let input = make_input_with_trades(-2.0, 0.0, 0, 0);
    // trade check skipped; Sharpe delta 2.0 > 0.1 passes; drawdown ok.
    assert!(matches!(evaluate(&input), GateVerdict::Pass));
}

/// Child below parent trade ratio must be rejected.
#[test]
fn reject_child_below_parent_trade_ratio() {
    let input = make_input_with_trades(1.0, 1.5, 10, 3);
    // 3 < max(1, floor(10 * 0.5)) = 5 — fails trade check.
    let GateVerdict::Fail { reason } = evaluate(&input) else {
        panic!("expected rejection of low-trade child");
    };
    assert!(reason.contains("insufficient trades"));
}

/// Combined failure: 0 trades AND blown drawdown → both reasons in output.
#[test]
fn combined_trade_and_drawdown_failure() {
    let input = GateInput {
        parent_day_metrics: MetricsSummary {
            sharpe: -2.0,
            max_drawdown_pct: -5.0,
            n_trades: 10,
            ..MetricsSummary::default()
            min_realized_return_ratio: 0.0,
    },
        child_day_metrics: MetricsSummary {
            sharpe: 0.0,
            max_drawdown_pct: -15.0, // 3× parent worst
            n_trades: 0,
            ..MetricsSummary::default()
        },
        parent_untouched_metrics: MetricsSummary {
            sharpe: -2.0,
            max_drawdown_pct: -4.0,
            n_trades: 10,
            ..MetricsSummary::default()
        },
        child_untouched_metrics: MetricsSummary {
            sharpe: 0.0,
            max_drawdown_pct: -12.0,
            n_trades: 0,
            ..MetricsSummary::default()
        },
        min_improvement: 0.1,
        holdout_min_improvement: 0.1,
        objective: Objective::default(),
        parent_n_trades: 10,
        child_n_trades: 0,
        min_trade_retention_ratio: 0.5,
        min_realized_return_ratio: 0.0,
    };
    let GateVerdict::Fail { reason } = evaluate(&input) else {
        panic!("expected combined failure");
    };
    assert!(reason.contains("insufficient trades"), "reason: {reason}");
    assert!(reason.contains("max drawdown deteriorated"), "reason: {reason}");
}

// ── Realized-return ratio gate ────────────────────────────────────────────

fn rr_input(
    parent_total: f64,
    child_total: f64,
    parent_realized: f64,
    child_realized: f64,
    min_ratio: f64,
) -> GateInput {
    GateInput {
        parent_day_metrics: MetricsSummary {
            sharpe: 1.0,
            total_return_pct: parent_total,
            realized_pnl_pct: parent_realized,
            max_drawdown_pct: -10.0,
            ..MetricsSummary::default()
        },
        child_day_metrics: MetricsSummary {
            sharpe: 1.2,
            total_return_pct: child_total,
            realized_pnl_pct: child_realized,
            max_drawdown_pct: -10.0,
            ..MetricsSummary::default()
        },
        parent_untouched_metrics: MetricsSummary {
            sharpe: 0.8,
            total_return_pct: parent_total,
            realized_pnl_pct: parent_realized,
            max_drawdown_pct: -8.0,
            ..MetricsSummary::default()
        },
        child_untouched_metrics: MetricsSummary {
            sharpe: 1.0,
            total_return_pct: child_total,
            realized_pnl_pct: child_realized,
            max_drawdown_pct: -8.0,
            ..MetricsSummary::default()
        },
        min_improvement: 0.1,
        holdout_min_improvement: 0.1,
        objective: Objective::default(),
        parent_n_trades: 10,
        child_n_trades: 10,
        min_trade_retention_ratio: 0.5,
        min_realized_return_ratio: min_ratio,
    }
}

/// Child with insufficient realized profit must be rejected.
#[test]
fn reject_child_below_realized_ratio() {
    // Total return 10%, only 1% realized → ratio 0.1 < 0.25
    let input = rr_input(5.0, 10.0, 3.0, 1.0, 0.25);
    let GateVerdict::Fail { reason } = evaluate(&input) else {
        panic!("expected realized-ratio rejection");
    };
    assert!(reason.contains("insufficient realized profit"), "reason: {reason}");
}

/// Child with enough realized profit passes.
#[test]
fn accept_child_above_realized_ratio() {
    let input = rr_input(5.0, 10.0, 3.0, 4.0, 0.25);
    assert!(matches!(evaluate(&input), GateVerdict::Pass));
}

/// Negative total return must skip the check.
#[test]
fn skip_realized_check_on_negative_return() {
    let input = rr_input(-5.0, -2.0, -3.0, -1.0, 0.25);
    // Delta Sharpe 1.2 - 1.0 = 0.2 > 0.1 → passes on Sharpe, realized skipped
    assert!(matches!(evaluate(&input), GateVerdict::Pass));
}

/// Zero total return must skip the check.
#[test]
fn skip_realized_check_on_zero_return() {
    let input = rr_input(0.0, 0.0, 0.0, 0.0, 0.25);
    // Delta Sharpe ok, drawdown ok, realized skipped
    assert!(matches!(evaluate(&input), GateVerdict::Pass));
}

/// Ratio 0.0 disables the check.
#[test]
fn disabled_realized_check_passes_zero_realized() {
    let input = rr_input(5.0, 10.0, 3.0, 0.0, 0.0);
    assert!(matches!(evaluate(&input), GateVerdict::Pass));
}

/// Combined failure: realized ratio AND drawdown both fail.
#[test]
fn combined_realized_and_drawdown_failure() {
    let input = GateInput {
        parent_day_metrics: MetricsSummary {
            sharpe: 1.0,
            total_return_pct: 5.0,
            realized_pnl_pct: 3.0,
            max_drawdown_pct: -5.0,
            ..MetricsSummary::default()
        },
        child_day_metrics: MetricsSummary {
            sharpe: 1.2,
            total_return_pct: 10.0,
            realized_pnl_pct: 1.0,
            max_drawdown_pct: -15.0,
            ..MetricsSummary::default()
        },
        parent_untouched_metrics: MetricsSummary {
            sharpe: 0.8,
            total_return_pct: 5.0,
            realized_pnl_pct: 3.0,
            max_drawdown_pct: -4.0,
            ..MetricsSummary::default()
        },
        child_untouched_metrics: MetricsSummary {
            sharpe: 1.0,
            total_return_pct: 10.0,
            realized_pnl_pct: 1.0,
            max_drawdown_pct: -12.0,
            ..MetricsSummary::default()
        },
        min_improvement: 0.1,
        holdout_min_improvement: 0.1,
        objective: Objective::default(),
        parent_n_trades: 10,
        child_n_trades: 10,
        min_trade_retention_ratio: 0.5,
        min_realized_return_ratio: 0.25,
    };
    let GateVerdict::Fail { reason } = evaluate(&input) else {
        panic!("expected combined failure");
    };
    assert!(reason.contains("insufficient realized profit"), "reason: {reason}");
    assert!(reason.contains("max drawdown deteriorated"), "reason: {reason}");
}
