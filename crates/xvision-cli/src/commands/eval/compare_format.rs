//! Markdown formatting for `xvn eval compare`.
//!
//! `render_markdown` converts a `ComparisonReport` to GitHub-flavoured
//! Markdown suitable for drop-in to a PR description or chat reply.
//!
//! Columns are aligned with the on-demand behavior derivation that ships
//! in `xvision_engine::eval::behavior::BehaviorSummary`. Per-action
//! counts (long/short/flat/hold) are intentionally omitted from the
//! table — that surface lives on `xvn eval show <run> --behavior`, and
//! duplicating it here would require either a parallel derivation pass
//! or extending `BehaviorSummary` with `action_counts`. Track #14
//! follow-up may revisit.

use xvision_engine::eval::compare::{ComparisonReport, ComparisonRunSummary};

pub fn render_markdown(report: &ComparisonReport, strategy_label: &str) -> String {
    let mut out = String::new();

    out.push_str(&format!("## eval compare — {strategy_label}\n\n"));

    out.push_str(
        "| Scenario | Return | Baseline (buy_hold) | Sharpe | Max DD | Decisions | Trades | Flips | Avg hold (bars) | Flat rate | Reentries | Failure mode |\n",
    );
    out.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n");

    for run in &report.runs {
        out.push_str(&markdown_row(run));
        out.push('\n');
    }

    out.push('\n');
    out.push_str("### Notes\n\n");

    let completed: Vec<&ComparisonRunSummary> = report.runs.iter().filter(|r| r.metrics.is_some()).collect();

    if completed.is_empty() {
        out.push_str("No completed runs with metrics available.\n");
    } else {
        let best = completed
            .iter()
            .max_by(|a, b| cmp_return(a, b))
            .expect("non-empty");
        let worst = completed
            .iter()
            .min_by(|a, b| cmp_return(a, b))
            .expect("non-empty");

        let best_ret = best.metrics.as_ref().map(|m| m.total_return_pct).unwrap_or(0.0);
        let worst_ret = worst.metrics.as_ref().map(|m| m.total_return_pct).unwrap_or(0.0);

        out.push_str(&format!(
            "Best run: `{}` ({:+.2}%)\n\n",
            best.scenario_id, best_ret
        ));
        out.push_str(&format!(
            "Worst run: `{}` ({:+.2}%)\n",
            worst.scenario_id, worst_ret
        ));
    }

    out
}

fn cmp_return(a: &&ComparisonRunSummary, b: &&ComparisonRunSummary) -> std::cmp::Ordering {
    let ra = a
        .metrics
        .as_ref()
        .map(|m| m.total_return_pct)
        .unwrap_or(f64::NEG_INFINITY);
    let rb = b
        .metrics
        .as_ref()
        .map(|m| m.total_return_pct)
        .unwrap_or(f64::NEG_INFINITY);
    ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
}

fn markdown_row(run: &ComparisonRunSummary) -> String {
    let (ret_s, baseline_buy_hold_s, sharpe_s, dd_s, dec_s) = match &run.metrics {
        Some(m) => {
            let bh_s = m
                .baselines
                .as_ref()
                .map(|b| format!("{:+.2}%", b.relative_to.buy_hold))
                .unwrap_or_else(|| "-".into());
            (
                format!("{:+.2}%", m.total_return_pct),
                bh_s,
                format!("{:.3}", m.sharpe),
                format!("{:.2}%", m.max_drawdown_pct),
                m.n_decisions.to_string(),
            )
        }
        None => ("-".into(), "-".into(), "-".into(), "-".into(), "-".into()),
    };

    let (trades_s, flips_s, avg_hold_s, flat_rate_s, reentries_s, failure_s) = match &run.behavior {
        Some(b) => (
            b.trades_opened.to_string(),
            b.direct_flips.to_string(),
            b.avg_bars_held
                .map(|v| format!("{:.1}", v))
                .unwrap_or_else(|| "-".into()),
            format!("{:.2}", b.flat_rate),
            b.reentries_after_loss.to_string(),
            b.primary_failure_mode.clone(),
        ),
        None => (
            "-".into(),
            "-".into(),
            "-".into(),
            "-".into(),
            "-".into(),
            "-".into(),
        ),
    };

    format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        run.scenario_id,
        ret_s,
        baseline_buy_hold_s,
        sharpe_s,
        dd_s,
        dec_s,
        trades_s,
        flips_s,
        avg_hold_s,
        flat_rate_s,
        reentries_s,
        failure_s,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use xvision_engine::eval::behavior::BehaviorSummary;
    use xvision_engine::eval::compare::{ComparisonEquityCurve, ComparisonReport, ComparisonRunSummary};
    use xvision_engine::eval::run::{MetricsSummary, RunMode, RunStatus};

    fn make_run(
        scenario_id: &str,
        total_return_pct: f64,
        sharpe: f64,
        max_drawdown_pct: f64,
        n_decisions: u32,
        behavior: Option<BehaviorSummary>,
    ) -> ComparisonRunSummary {
        ComparisonRunSummary {
            id: format!("run_{scenario_id}"),
            agent_id: "strat_abc".into(),
            scenario_id: scenario_id.into(),
            mode: RunMode::Backtest,
            status: RunStatus::Completed,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            metrics: Some(MetricsSummary {
                total_return_pct,
                sharpe,
                max_drawdown_pct,
                win_rate: 0.5,
                n_trades: 3,
                n_decisions,
                baselines: None,
                ..Default::default()
            }),
            error: None,
            behavior,
            net_return_pct: None,
        }
    }

    fn make_behavior(
        trades: u32,
        flips: u32,
        avg_held: Option<f64>,
        flat_rate: f64,
        reentries: u32,
        failure_mode: &str,
    ) -> BehaviorSummary {
        BehaviorSummary {
            flat_rate,
            trades_opened: trades,
            direct_flips: flips,
            avg_bars_held: avg_held,
            reentries_after_loss: reentries,
            exits_on_invalidation: 0,
            primary_failure_mode: failure_mode.into(),
        }
    }

    fn make_report(runs: Vec<ComparisonRunSummary>) -> ComparisonReport {
        ComparisonReport {
            equity_curves: runs
                .iter()
                .map(|r| ComparisonEquityCurve {
                    run_id: r.id.clone(),
                    samples: vec![],
                })
                .collect(),
            findings: vec![],
            runs,
        }
    }

    #[test]
    fn markdown_contains_strategy_heading() {
        let report = make_report(vec![make_run(
            "eth_7d",
            -8.85,
            -33.54,
            8.85,
            49,
            Some(make_behavior(7, 0, Some(3.2), 0.71, 2, "late_entries")),
        )]);
        let md = render_markdown(&report, "my-strategy");
        assert!(
            md.starts_with("## eval compare — my-strategy\n"),
            "heading missing; got: {md}"
        );
    }

    #[test]
    fn markdown_table_header_present() {
        let report = make_report(vec![]);
        let md = render_markdown(&report, "strat");
        assert!(
            md.contains(
                "| Scenario | Return | Baseline (buy_hold) | Sharpe | Max DD | Decisions | Trades | Flips | Avg hold (bars) | Flat rate | Reentries | Failure mode |"
            ),
            "table header missing; got:\n{md}"
        );
    }

    #[test]
    fn markdown_row_contains_all_metrics() {
        let report = make_report(vec![make_run(
            "eth_7d",
            -8.85,
            -33.54,
            8.85,
            49,
            Some(make_behavior(7, 0, Some(3.2), 0.71, 2, "late_entries")),
        )]);
        let md = render_markdown(&report, "s");
        assert!(md.contains("-8.85%"), "return missing: {md}");
        assert!(md.contains("-33.540"), "sharpe missing: {md}");
        assert!(md.contains("8.85%"), "drawdown missing: {md}");
        assert!(md.contains("49"), "decisions missing: {md}");
        assert!(md.contains("| 7 |"), "trades_opened missing: {md}");
        assert!(md.contains("| 0 |"), "flips missing: {md}");
        assert!(md.contains("3.2"), "avg hold missing: {md}");
        assert!(md.contains("0.71"), "flat_rate missing: {md}");
        assert!(md.contains("| 2 |"), "reentries missing: {md}");
        assert!(md.contains("late_entries"), "failure mode missing: {md}");
    }

    #[test]
    fn markdown_row_dashes_when_no_metrics() {
        let run = ComparisonRunSummary {
            id: "run_x".into(),
            agent_id: "strat_abc".into(),
            scenario_id: "sc_no_metrics".into(),
            mode: RunMode::Backtest,
            status: RunStatus::Failed,
            started_at: Utc::now(),
            completed_at: None,
            metrics: None,
            error: Some("timeout".into()),
            behavior: None,
            net_return_pct: None,
        };
        let report = make_report(vec![run]);
        let md = render_markdown(&report, "s");
        assert!(md.contains("| - |"), "missing dashes in row: {md}");
    }

    #[test]
    fn markdown_notes_best_and_worst() {
        let report = make_report(vec![
            make_run("eth_7d", -8.85, -33.54, 8.85, 49, None),
            make_run("sol_8d", -2.32, -2.64, 10.74, 49, None),
            make_run("btc_crash", -9.56, -8.75, 14.68, 49, None),
        ]);
        let md = render_markdown(&report, "strat");
        assert!(md.contains("### Notes"), "notes section missing: {md}");
        assert!(md.contains("Best run:"), "best run note missing: {md}");
        assert!(md.contains("Worst run:"), "worst run note missing: {md}");
        assert!(md.contains("sol_8d"), "best run scenario missing: {md}");
        assert!(md.contains("btc_crash"), "worst run scenario missing: {md}");
    }

    #[test]
    fn markdown_row_no_behavior_uses_dashes() {
        let run = make_run("eth_7d", -8.85, -33.54, 8.85, 49, None);
        let row = markdown_row(&run);
        let pipes = row.chars().filter(|&c| c == '|').count();
        assert_eq!(pipes, 13, "unexpected pipe count in row: {row}");
    }

    #[test]
    fn markdown_row_avg_hold_dash_when_none() {
        let beh = BehaviorSummary {
            flat_rate: 0.0,
            trades_opened: 0,
            direct_flips: 0,
            avg_bars_held: None,
            reentries_after_loss: 0,
            exits_on_invalidation: 0,
            primary_failure_mode: "none_obvious".into(),
        };
        let run = make_run("eth_7d", -8.85, -33.54, 8.85, 49, Some(beh));
        let row = markdown_row(&run);
        assert!(row.contains("| - |"), "avg hold dash missing: {row}");
    }

    #[test]
    fn markdown_snapshot_four_runs() {
        let report = make_report(vec![
            make_run(
                "eth_7d",
                -8.85,
                -33.54,
                8.85,
                49,
                Some(make_behavior(7, 0, Some(3.2), 0.71, 2, "late_entries")),
            ),
            make_run(
                "btc_bull",
                -6.17,
                -23.95,
                6.17,
                49,
                Some(make_behavior(4, 1, Some(2.8), 0.78, 1, "churn")),
            ),
            make_run(
                "btc_crash",
                -9.56,
                -8.75,
                14.68,
                49,
                Some(make_behavior(2, 0, Some(4.0), 0.82, 0, "over_flat")),
            ),
            make_run(
                "sol_8d",
                -2.32,
                -2.64,
                10.74,
                49,
                Some(make_behavior(5, 0, Some(3.5), 0.74, 1, "no_edge")),
            ),
        ]);
        let md = render_markdown(&report, "compression-sniper-v2");

        assert!(md.starts_with("## eval compare — compression-sniper-v2\n"));
        for s in ["eth_7d", "btc_bull", "btc_crash", "sol_8d"] {
            assert!(md.contains(s), "scenario {s} missing: {md}");
        }
        assert!(md.contains("Best run:"));
        assert!(md.contains("Worst run:"));
        // best = sol_8d (-2.32), worst = btc_crash (-9.56)
        assert!(md.contains("sol_8d"));
        assert!(md.contains("btc_crash"));
    }
}
