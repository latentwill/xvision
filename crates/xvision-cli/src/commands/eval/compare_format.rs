//! Markdown formatting for `xvn eval compare`.
//!
//! `render_markdown` converts a `ComparisonReport` to GitHub-flavoured
//! Markdown suitable for drop-in to a PR description or chat reply.
//!
//! Columns include the action-distribution rollup, token totals, wall
//! clock, and cost estimate now that `ComparisonRunSummary` carries the
//! aggregated fields (see `eval::report` and the
//! `cli-report-actions-and-tokens` contract, 2026-05-22). The compact
//! action cell uses a `12L/3S/1F/207H/2LC/0SC` shorthand so a 4-arm
//! table still fits in a 120-char terminal.

use xvision_engine::eval::behavior::ActionCounts;
use xvision_engine::eval::compare::{ComparisonReport, ComparisonRunSummary};

pub fn render_markdown(report: &ComparisonReport, strategy_label: &str) -> String {
    let mut out = String::new();

    out.push_str(&format!("## eval compare — {strategy_label}\n\n"));

    out.push_str(
        "| Scenario | Return | Baseline (buy_hold) | Sharpe | Max DD | Decisions | Trades | Actions | Flips | Repeats | Avg hold (bars) | Flat rate | Reentries | Tokens (in/out) | Wall clock | Cost | Failure mode |\n",
    );
    out.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n");

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

/// Render the per-action counts as a compact `12L/3S/1F/207H/2LC/0SC`
/// shorthand. Suffixes:
///   L = long_open, S = short_open, F = flat, H = hold,
///   LC = long_close, SC = short_close.
/// Cells where every count is zero collapse to `—` so an all-zero arm
/// doesn't dominate the column width.
fn compact_action_cell(counts: &ActionCounts) -> String {
    if counts.long_open == 0
        && counts.short_open == 0
        && counts.flat == 0
        && counts.hold == 0
        && counts.long_close == 0
        && counts.short_close == 0
    {
        return "—".into();
    }
    format!(
        "{}L/{}S/{}F/{}H/{}LC/{}SC",
        counts.long_open,
        counts.short_open,
        counts.flat,
        counts.hold,
        counts.long_close,
        counts.short_close,
    )
}

/// Format a wall-clock duration in ms as a compact human-readable string.
fn fmt_wall_clock(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        let secs = ms / 1_000;
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}m{s:02}s")
    }
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

    let (trades_s, actions_s, flips_s, repeats_s, avg_hold_s, flat_rate_s, reentries_s, failure_s) =
        match &run.behavior {
            Some(b) => (
                b.action_counts.trades().to_string(),
                compact_action_cell(&b.action_counts),
                b.direct_flips.to_string(),
                b.repeated_opens.to_string(),
                b.avg_bars_held
                    .map(|v| format!("{:.1}", v))
                    .unwrap_or_else(|| "—".into()),
                format!("{:.2}", b.flat_rate),
                b.reentries_after_loss.to_string(),
                b.primary_failure_mode.clone(),
            ),
            None => (
                "—".into(),
                "—".into(),
                "—".into(),
                "—".into(),
                "—".into(),
                "—".into(),
                "—".into(),
                "—".into(),
            ),
        };

    let tokens_s = match (run.input_tokens, run.output_tokens) {
        (Some(i), Some(o)) => format!("{i}/{o}"),
        (Some(i), None) => format!("{i}/—"),
        (None, Some(o)) => format!("—/{o}"),
        (None, None) => "—".into(),
    };

    let wall_s = run.wall_clock_ms.map(fmt_wall_clock).unwrap_or_else(|| "—".into());

    let cost_s = match run.cost_usd_estimate {
        Some(c) => {
            let lb = if run.cost_estimate_complete { "" } else { "*" };
            format!("${c:.4}{lb}")
        }
        None => "—".into(),
    };

    format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        run.scenario_id,
        ret_s,
        baseline_buy_hold_s,
        sharpe_s,
        dd_s,
        dec_s,
        trades_s,
        actions_s,
        flips_s,
        repeats_s,
        avg_hold_s,
        flat_rate_s,
        reentries_s,
        tokens_s,
        wall_s,
        cost_s,
        failure_s,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use xvision_engine::eval::behavior::{ActionCounts, BehaviorSummary};
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
        let started = Utc::now();
        ComparisonRunSummary {
            id: format!("run_{scenario_id}"),
            agent_id: "strat_abc".into(),
            scenario_id: scenario_id.into(),
            mode: RunMode::Backtest,
            status: RunStatus::Completed,
            started_at: started,
            completed_at: Some(started + Duration::milliseconds(12_345)),
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
            bars_content_hash: None,
            manifest_canonical: None,
            net_return_pct: None,
            input_tokens: Some(2_100),
            output_tokens: Some(1_400),
            cost_usd_estimate: Some(0.0420),
            cost_estimate_complete: true,
            wall_clock_ms: Some(12_345),
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
            action_counts: ActionCounts {
                long_open: trades.saturating_sub(1),
                short_open: 1,
                flat: 1,
                hold: 40,
                long_close: 0,
                short_close: 0,
            },
            repeated_opens: 0,
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
            md.contains("Scenario")
                && md.contains("Tokens (in/out)")
                && md.contains("Wall clock")
                && md.contains("Cost")
                && md.contains("Actions"),
            "table header missing token/cost/actions columns; got:\n{md}"
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
        assert!(md.contains("3.2"), "avg hold missing: {md}");
        assert!(md.contains("0.71"), "flat_rate missing: {md}");
        assert!(md.contains("late_entries"), "failure mode missing: {md}");
        // New columns:
        assert!(md.contains("2100/1400"), "tokens cell missing: {md}");
        assert!(md.contains("$0.0420"), "cost cell missing: {md}");
        assert!(md.contains("12.3s"), "wall-clock cell missing: {md}");
        // Compact action cell: `<long_open>L/<short_open>S/<flat>F/<hold>H/<lc>LC/<sc>SC`
        assert!(md.contains("L/") && md.contains("F/") && md.contains("H/"), "action cell missing: {md}");
    }

    #[test]
    fn markdown_row_dashes_when_no_metrics_or_tokens() {
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
            bars_content_hash: None,
            manifest_canonical: None,
            net_return_pct: None,
            input_tokens: None,
            output_tokens: None,
            cost_usd_estimate: None,
            cost_estimate_complete: true,
            wall_clock_ms: None,
        };
        let report = make_report(vec![run]);
        let md = render_markdown(&report, "s");
        // Use em-dash for null aggregates per contract acceptance.
        assert!(md.contains("| — |"), "missing em-dash for null fields: {md}");
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
    fn cost_cell_marks_incomplete_with_asterisk() {
        let mut run = make_run(
            "eth_7d",
            -1.0,
            0.0,
            0.0,
            10,
            Some(make_behavior(1, 0, Some(2.0), 0.5, 0, "none_obvious")),
        );
        run.cost_estimate_complete = false;
        let report = make_report(vec![run]);
        let md = render_markdown(&report, "s");
        assert!(md.contains("$0.0420*"), "incomplete-cost marker missing: {md}");
    }

    #[test]
    fn compact_action_cell_em_dash_when_all_zero() {
        let counts = ActionCounts::default();
        assert_eq!(compact_action_cell(&counts), "—");
    }

    #[test]
    fn compact_action_cell_renders_all_six() {
        let counts = ActionCounts {
            long_open: 12,
            short_open: 3,
            flat: 1,
            hold: 207,
            long_close: 2,
            short_close: 0,
        };
        assert_eq!(compact_action_cell(&counts), "12L/3S/1F/207H/2LC/0SC");
    }

    #[test]
    fn fmt_wall_clock_humanises() {
        assert_eq!(fmt_wall_clock(500), "500ms");
        assert_eq!(fmt_wall_clock(12_345), "12.3s");
        assert_eq!(fmt_wall_clock(125_000), "2m05s");
    }
}
