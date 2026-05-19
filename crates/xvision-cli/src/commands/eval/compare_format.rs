//! Markdown + human-readable formatting for `xvn eval compare`.
//!
//! `render_markdown` converts a `ComparisonReport` to GitHub-flavoured
//! Markdown suitable for drop-in to a PR description or chat reply.
//! `render_human` is the existing tab-separated table output, refactored
//! here so `mod.rs` stays lean.

use xvision_engine::eval::compare::{ComparisonReport, ComparisonRunSummary};

// ---------------------------------------------------------------------------
// Markdown rendering
// ---------------------------------------------------------------------------

/// Render a `ComparisonReport` as GitHub-flavoured Markdown.
///
/// Output structure:
/// ```text
/// ## eval compare — <strategy-name>
///
/// | Scenario | Return | Sharpe | Max DD | Decisions | Long | Short | Flat | Hold | Avg hold (bars) | Flips |
/// | --- | ---: | ...
/// | ...                                                                   |
///
/// ### Notes
/// Worst run: … (return)
/// Best run:  … (return)
/// ```
///
/// `strategy_label` is used in the heading; pass the strategy id if the
/// name is not available.
pub fn render_markdown(report: &ComparisonReport, strategy_label: &str) -> String {
    let mut out = String::new();

    out.push_str(&format!("## eval compare — {strategy_label}\n\n"));

    // Table header
    out.push_str("| Scenario | Return | Sharpe | Max DD | Decisions | Long | Short | Flat | Hold | Avg hold (bars) | Flips |\n");
    out.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");

    for run in &report.runs {
        let row = markdown_row(run);
        out.push_str(&row);
        out.push('\n');
    }

    out.push('\n');

    // Notes section
    out.push_str("### Notes\n\n");

    let completed: Vec<&ComparisonRunSummary> = report
        .runs
        .iter()
        .filter(|r| r.metrics.is_some())
        .collect();

    if completed.is_empty() {
        out.push_str("No completed runs with metrics available.\n");
    } else {
        let best = completed
            .iter()
            .max_by(|a, b| {
                let ra = a.metrics.as_ref().map(|m| m.total_return_pct).unwrap_or(f64::NEG_INFINITY);
                let rb = b.metrics.as_ref().map(|m| m.total_return_pct).unwrap_or(f64::NEG_INFINITY);
                ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();
        let worst = completed
            .iter()
            .min_by(|a, b| {
                let ra = a.metrics.as_ref().map(|m| m.total_return_pct).unwrap_or(f64::INFINITY);
                let rb = b.metrics.as_ref().map(|m| m.total_return_pct).unwrap_or(f64::INFINITY);
                ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

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

fn markdown_row(run: &ComparisonRunSummary) -> String {
    let (ret_s, sharpe_s, dd_s, dec_s) = match &run.metrics {
        Some(m) => (
            format!("{:+.2}%", m.total_return_pct),
            format!("{:.3}", m.sharpe),
            format!("{:.2}%", m.max_drawdown_pct),
            m.n_decisions.to_string(),
        ),
        None => ("-".into(), "-".into(), "-".into(), "-".into()),
    };

    let (long_s, short_s, flat_s, hold_s, avg_hold_s, flips_s) = match &run.behavior {
        Some(b) => {
            let long = b.action_counts.get("long_open").copied().unwrap_or(0);
            let short = b.action_counts.get("short_open").copied().unwrap_or(0);
            let flat = b.action_counts.get("flat").copied().unwrap_or(0);
            let hold = b.action_counts.get("hold").copied().unwrap_or(0);
            let avg_hold = b
                .avg_bars_held
                .map(|v| format!("{:.1}", v))
                .unwrap_or_else(|| "-".into());
            (
                long.to_string(),
                short.to_string(),
                flat.to_string(),
                hold.to_string(),
                avg_hold,
                b.direct_flips.to_string(),
            )
        }
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
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        run.scenario_id,
        ret_s,
        sharpe_s,
        dd_s,
        dec_s,
        long_s,
        short_s,
        flat_s,
        hold_s,
        avg_hold_s,
        flips_s,
    )
}

// ---------------------------------------------------------------------------
// Tests — snapshot discipline
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::BTreeMap;
    use xvision_engine::eval::behavior::BehaviorSummary;
    use xvision_engine::eval::compare::{
        ComparisonEquityCurve, ComparisonReport, ComparisonRunSummary,
    };
    use xvision_engine::eval::findings::Finding;
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
            }),
            error: None,
            behavior,
        }
    }

    fn make_behavior(
        long: u64,
        short: u64,
        flat: u64,
        hold: u64,
        flips: u64,
        avg_held: Option<f64>,
    ) -> BehaviorSummary {
        let mut counts = BTreeMap::new();
        counts.insert("long_open".into(), long);
        counts.insert("short_open".into(), short);
        counts.insert("flat".into(), flat);
        counts.insert("hold".into(), hold);
        BehaviorSummary {
            action_counts: counts,
            trades_opened: long + short,
            flat_rate: Some((flat + hold) as f64 / (long + short + flat + hold) as f64),
            direct_flips: flips,
            avg_bars_held: avg_held,
            worst_trade_pct: Some(-5.0),
            best_trade_pct: Some(10.0),
        }
    }

    fn make_report(runs: Vec<ComparisonRunSummary>) -> ComparisonReport {
        ComparisonReport {
            equity_curves: runs.iter().map(|r| ComparisonEquityCurve { run_id: r.id.clone(), samples: vec![] }).collect(),
            findings: vec![],
            runs,
        }
    }

    // -----------------------------------------------------------------------
    // Header line
    // -----------------------------------------------------------------------

    #[test]
    fn markdown_contains_strategy_heading() {
        let report = make_report(vec![make_run(
            "eth_7d",
            -8.85,
            -33.54,
            8.85,
            49,
            Some(make_behavior(1, 6, 35, 7, 0, Some(3.2))),
        )]);
        let md = render_markdown(&report, "my-strategy");
        assert!(md.starts_with("## eval compare — my-strategy\n"),
            "heading missing; got: {md}");
    }

    // -----------------------------------------------------------------------
    // Table header
    // -----------------------------------------------------------------------

    #[test]
    fn markdown_table_header_present() {
        let report = make_report(vec![]);
        let md = render_markdown(&report, "strat");
        assert!(md.contains("| Scenario | Return | Sharpe | Max DD | Decisions | Long | Short | Flat | Hold | Avg hold (bars) | Flips |"),
            "table header missing; got:\n{md}");
    }

    // -----------------------------------------------------------------------
    // Data row
    // -----------------------------------------------------------------------

    #[test]
    fn markdown_row_contains_all_metrics() {
        let report = make_report(vec![make_run(
            "eth_7d",
            -8.85,
            -33.54,
            8.85,
            49,
            Some(make_behavior(1, 6, 35, 7, 0, Some(3.2))),
        )]);
        let md = render_markdown(&report, "s");
        // Check return, sharpe, drawdown, decisions
        assert!(md.contains("-8.85%"), "return missing: {md}");
        assert!(md.contains("-33.540"), "sharpe missing: {md}");
        assert!(md.contains("8.85%"), "drawdown missing: {md}");
        assert!(md.contains("49"), "decisions missing: {md}");
        // Check action distribution
        assert!(md.contains("| 1 |"), "long_open missing: {md}");
        assert!(md.contains("| 6 |"), "short_open missing: {md}");
        assert!(md.contains("| 35 |"), "flat missing: {md}");
        assert!(md.contains("| 7 |"), "hold missing: {md}");
        assert!(md.contains("3.2"), "avg hold missing: {md}");
        assert!(md.contains("| 0 |"), "flips missing: {md}");
    }

    // -----------------------------------------------------------------------
    // No-metrics rows use dashes
    // -----------------------------------------------------------------------

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
        };
        let report = make_report(vec![run]);
        let md = render_markdown(&report, "s");
        // Each dash-padded cell appears at least once
        assert!(md.contains("| - |"), "missing dashes in row: {md}");
    }

    // -----------------------------------------------------------------------
    // Notes section
    // -----------------------------------------------------------------------

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
        // sol_8d is the best (-2.32), btc_crash is worst (-9.56)
        assert!(md.contains("sol_8d"), "best run scenario missing: {md}");
        assert!(md.contains("btc_crash"), "worst run scenario missing: {md}");
    }

    // -----------------------------------------------------------------------
    // markdown_row helper
    // -----------------------------------------------------------------------

    #[test]
    fn markdown_row_no_behavior_uses_dashes() {
        let run = make_run("eth_7d", -8.85, -33.54, 8.85, 49, None);
        let row = markdown_row(&run);
        // Should have 11 cells (12 pipes)
        let pipes = row.chars().filter(|&c| c == '|').count();
        assert_eq!(pipes, 12, "unexpected pipe count in row: {row}");
    }

    #[test]
    fn markdown_row_avg_hold_dash_when_none() {
        let beh = BehaviorSummary {
            action_counts: BTreeMap::new(),
            trades_opened: 0,
            flat_rate: None,
            direct_flips: 0,
            avg_bars_held: None,
            worst_trade_pct: None,
            best_trade_pct: None,
        };
        let run = make_run("eth_7d", -8.85, -33.54, 8.85, 49, Some(beh));
        let row = markdown_row(&run);
        // avg hold column should be "-"
        assert!(row.contains("| - |"), "avg hold dash missing: {row}");
    }

    // -----------------------------------------------------------------------
    // Full snapshot test — exercises every column
    // -----------------------------------------------------------------------

    #[test]
    fn markdown_snapshot_four_runs() {
        let report = make_report(vec![
            make_run("eth_7d", -8.85, -33.54, 8.85, 49, Some(make_behavior(1, 6, 35, 7, 0, Some(3.2)))),
            make_run("btc_bull", -6.17, -23.95, 6.17, 49, Some(make_behavior(0, 4, 38, 7, 1, Some(2.8)))),
            make_run("btc_crash", -9.56, -8.75, 14.68, 49, Some(make_behavior(0, 2, 40, 7, 0, Some(4.0)))),
            make_run("sol_8d", -2.32, -2.64, 10.74, 49, Some(make_behavior(2, 3, 37, 7, 0, Some(3.5)))),
        ]);
        let md = render_markdown(&report, "compression-sniper-v2");

        // Heading
        assert!(md.starts_with("## eval compare — compression-sniper-v2\n"));
        // All four scenarios appear
        assert!(md.contains("eth_7d"));
        assert!(md.contains("btc_bull"));
        assert!(md.contains("btc_crash"));
        assert!(md.contains("sol_8d"));
        // Notes
        assert!(md.contains("Best run:"));
        assert!(md.contains("Worst run:"));
        // best = sol_8d (-2.32), worst = btc_crash (-9.56)
        assert!(md.contains("sol_8d"));
        assert!(md.contains("btc_crash"));
    }
}
