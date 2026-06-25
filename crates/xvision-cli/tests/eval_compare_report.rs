//! Integration tests for `xvn eval compare` with --json / --markdown / --sort.
//!
//! Seeds two completed runs (with known decisions) via the engine's `RunStore`,
//! then invokes the CLI binary and asserts the output shapes.
//!
//! Test inventory:
//!   1. happy_path_json_has_action_distribution_and_behavior_fields
//!   2. happy_path_markdown_contains_header_and_row
//!   3. runs_flag_comma_separated_works
//!   4. sort_by_sharpe_changes_row_order
//!   5. single_id_via_runs_flag_returns_usage_error
//!   6. default_text_output_backward_compat

use std::process::Command;

use chrono::Utc;
use tempfile::tempdir;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::run::{MetricsSummary, Run, RunMode};
use xvision_engine::eval::scenario_store;
use xvision_engine::eval::store::{DecisionRow, RunStore};

// ---- helpers ----------------------------------------------------------------

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

fn count_unescaped_pipes(line: &str) -> usize {
    let mut escaped = false;
    let mut count = 0;
    for ch in line.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
        } else if ch == '|' {
            count += 1;
        }
    }
    count
}

/// Seed two completed runs with distinct metrics and known decisions.
/// Returns `(run_id_a, run_id_b)` where A has higher return, B has higher sharpe.
async fn seed_two_runs(home: &std::path::Path) -> (String, String) {
    let ctx = ApiContext::open(
        home,
        Actor::Cli {
            user: "eval-compare-report-test".into(),
        },
    )
    .await
    .expect("open ApiContext");
    let store = RunStore::new(ctx.db.clone());

    // --- Run A: higher total_return, lower sharpe ---
    let run_a = Run::new_queued(
        "agent-compare-test".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    let id_a = run_a.id.clone();
    store.create(&run_a).await.expect("seed run A");

    // Seed decisions: mostly flat (over_flat failure mode), a few trades.
    let now = Utc::now();
    let decisions_a: Vec<DecisionRow> = vec![
        DecisionRow {
            run_id: id_a.clone(),
            decision_index: 0,
            timestamp: now,
            asset: "BTC".into(),
            action: "long_open".into(),
            conviction: Some(0.8),
            justification: None,
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: None,
            delayed: Some(false),
        },
        DecisionRow {
            run_id: id_a.clone(),
            decision_index: 1,
            timestamp: now,
            asset: "BTC".into(),
            action: "hold".into(),
            conviction: None,
            justification: None,
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: None,
            delayed: Some(false),
        },
        DecisionRow {
            run_id: id_a.clone(),
            decision_index: 2,
            timestamp: now,
            asset: "BTC".into(),
            action: "flat".into(),
            conviction: None,
            justification: None,
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: Some(5.0),
            delayed: Some(false),
        },
        // Remaining decisions are flat/hold to push flat_rate high.
        DecisionRow {
            run_id: id_a.clone(),
            decision_index: 3,
            timestamp: now,
            asset: "BTC".into(),
            action: "flat".into(),
            conviction: None,
            justification: None,
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: None,
            delayed: Some(false),
        },
        DecisionRow {
            run_id: id_a.clone(),
            decision_index: 4,
            timestamp: now,
            asset: "BTC".into(),
            action: "hold".into(),
            conviction: None,
            justification: None,
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: None,
            delayed: Some(false),
        },
    ];
    for d in &decisions_a {
        store.record_decision(d).await.expect("record decision A");
    }

    store
        .finalize(
            &id_a,
            &MetricsSummary {
                total_return_pct: 8.5,
                sharpe: 0.5,
                max_drawdown_pct: 5.0,
                win_rate: 0.6,
                n_trades: 1,
                n_decisions: decisions_a.len() as u32,
                baselines: None,
                ..Default::default()
            },
        )
        .await
        .expect("finalize run A");

    // --- Run B: lower total_return, higher sharpe ---
    let run_b = Run::new_queued(
        "agent-compare-test".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    let id_b = run_b.id.clone();
    store.create(&run_b).await.expect("seed run B");

    let decisions_b: Vec<DecisionRow> = vec![
        DecisionRow {
            run_id: id_b.clone(),
            decision_index: 0,
            timestamp: now,
            asset: "ETH".into(),
            action: "short_open".into(),
            conviction: Some(0.7),
            justification: None,
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: None,
            delayed: Some(false),
        },
        DecisionRow {
            run_id: id_b.clone(),
            decision_index: 1,
            timestamp: now,
            asset: "ETH".into(),
            action: "flat".into(),
            conviction: None,
            justification: None,
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: Some(2.0),
            delayed: Some(false),
        },
        DecisionRow {
            run_id: id_b.clone(),
            decision_index: 2,
            timestamp: now,
            asset: "ETH".into(),
            action: "long_open".into(),
            conviction: Some(0.6),
            justification: None,
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: None,
            delayed: Some(false),
        },
        DecisionRow {
            run_id: id_b.clone(),
            decision_index: 3,
            timestamp: now,
            asset: "ETH".into(),
            action: "flat".into(),
            conviction: None,
            justification: None,
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: Some(1.5),
            delayed: Some(false),
        },
    ];
    for d in &decisions_b {
        store.record_decision(d).await.expect("record decision B");
    }

    store
        .finalize(
            &id_b,
            &MetricsSummary {
                total_return_pct: 3.5,
                sharpe: 2.1,
                max_drawdown_pct: 1.5,
                win_rate: 0.8,
                n_trades: 2,
                n_decisions: decisions_b.len() as u32,
                baselines: None,
                ..Default::default()
            },
        )
        .await
        .expect("finalize run B");

    (id_a, id_b)
}

async fn seed_extra_completed_run(home: &std::path::Path) -> String {
    let ctx = ApiContext::open(
        home,
        Actor::Cli {
            user: "eval-compare-report-test".into(),
        },
    )
    .await
    .expect("open ApiContext");
    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued(
        "agent-compare-test-extra".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    let id = run.id.clone();
    store.create(&run).await.expect("seed extra run");
    store
        .finalize(
            &id,
            &MetricsSummary {
                total_return_pct: 42.0,
                sharpe: 4.2,
                max_drawdown_pct: 0.5,
                win_rate: 1.0,
                n_trades: 0,
                n_decisions: 0,
                baselines: None,
                ..Default::default()
            },
        )
        .await
        .expect("finalize extra run");
    id
}

fn json_compare_run_ids(body: &serde_json::Value) -> Vec<String> {
    body["runs"]
        .as_array()
        .expect("runs is array")
        .iter()
        .map(|row| row["run_id"].as_str().expect("run row has run_id").to_string())
        .collect()
}

// ---- test 1: JSON output has action_distribution + behavior fields ----------

#[test]
fn happy_path_json_has_action_distribution_and_behavior_fields() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (id_a, id_b) = rt.block_on(async { seed_two_runs(dir.path()).await });

    let out = xvn(
        &["eval", "compare", "--json", "--runs", &format!("{id_a},{id_b}")],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be valid JSON");

    // Top-level keys.
    for key in ["runs", "equity_curves", "findings"] {
        assert!(body.get(key).is_some(), "missing top-level key `{key}`");
    }

    let runs = body["runs"].as_array().expect("runs is array");
    assert_eq!(runs.len(), 2, "expected 2 run rows");

    // Each row must have action_distribution and behavior fields.
    for row in runs {
        for field in [
            "run_id",
            "scenario_id",
            "scenario_name",
            "strategy_id",
            "status",
            "decisions",
            "trades_opened",
            "action_distribution",
            "avg_bars_held",
            "primary_failure_mode",
        ] {
            assert!(
                row.get(field).is_some(),
                "run row missing field `{field}`; row={row}"
            );
        }
        // action_distribution must be an object.
        assert!(
            row["action_distribution"].is_object(),
            "action_distribution must be an object"
        );
    }

    // Run A has a long_open so trades_opened >= 1.
    let row_a = runs
        .iter()
        .find(|r| r["run_id"].as_str() == Some(&id_a))
        .expect("row A not found");
    assert!(
        row_a["trades_opened"].as_u64().unwrap_or(0) >= 1,
        "run A must have at least 1 trade opened"
    );

    // Run B has short_open + long_open → trades_opened = 2.
    let row_b = runs
        .iter()
        .find(|r| r["run_id"].as_str() == Some(&id_b))
        .expect("row B not found");
    assert_eq!(
        row_b["trades_opened"].as_u64().unwrap_or(0),
        2,
        "run B must have 2 trades opened"
    );

    // Run B's action_distribution must contain short_open and long_open.
    let dist_b = &row_b["action_distribution"];
    assert!(
        dist_b["short_open"].as_u64().unwrap_or(0) >= 1,
        "run B action_distribution must have short_open"
    );
    assert!(
        dist_b["long_open"].as_u64().unwrap_or(0) >= 1,
        "run B action_distribution must have long_open"
    );
}

// ---- test 2: default sort by return → run A (8.5%) before run B (3.5%) -----

#[test]
fn default_sort_by_return_puts_higher_return_first() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (id_a, id_b) = rt.block_on(async { seed_two_runs(dir.path()).await });

    let out = xvn(
        &["eval", "compare", "--json", "--runs", &format!("{id_b},{id_a}")],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let runs = body["runs"].as_array().unwrap();

    assert_eq!(
        runs[0]["run_id"].as_str().unwrap(),
        id_a,
        "run A (return=8.5%) must be first when sorted by return"
    );
    assert_eq!(
        runs[1]["run_id"].as_str().unwrap(),
        id_b,
        "run B (return=3.5%) must be second"
    );
}

// ---- test 3: sort by sharpe → run B (2.1) before run A (0.5) ---------------

#[test]
fn sort_by_sharpe_changes_row_order() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (id_a, id_b) = rt.block_on(async { seed_two_runs(dir.path()).await });

    let out = xvn(
        &[
            "eval",
            "compare",
            "--json",
            "--runs",
            &format!("{id_a},{id_b}"),
            "--sort",
            "sharpe",
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let runs = body["runs"].as_array().unwrap();

    assert_eq!(
        runs[0]["run_id"].as_str().unwrap(),
        id_b,
        "run B (sharpe=2.1) must be first when sorted by sharpe"
    );
    assert_eq!(
        runs[1]["run_id"].as_str().unwrap(),
        id_a,
        "run A (sharpe=0.5) must be second"
    );
}

// ---- test 4: markdown output contains header and per-run row ----------------

#[test]
fn markdown_render_contains_header_and_per_run_row() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (id_a, id_b) = rt.block_on(async { seed_two_runs(dir.path()).await });

    let out = xvn(
        &[
            "eval",
            "compare",
            "--markdown",
            "--runs",
            &format!("{id_a},{id_b}"),
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let md = String::from_utf8_lossy(&out.stdout);

    // H1 header.
    assert!(
        md.contains("# Eval comparison (2 runs)"),
        "markdown must contain H1 header; got:\n{md}"
    );

    // Table header row.
    assert!(
        md.contains("| Run | Strategy | Scenario | Return % | Sharpe |"),
        "markdown must contain table header; got:\n{md}"
    );

    // Separator row.
    assert!(
        md.contains("|---|---|---|"),
        "markdown must contain table separator; got:\n{md}"
    );

    // Both runs must appear as table rows (check run id prefix).
    let prefix_a: String = id_a.chars().take(8).collect();
    let prefix_b: String = id_b.chars().take(8).collect();
    assert!(
        md.contains(&prefix_a),
        "markdown must contain run A prefix {prefix_a}; got:\n{md}"
    );
    assert!(
        md.contains(&prefix_b),
        "markdown must contain run B prefix {prefix_b}; got:\n{md}"
    );
}

// ---- test 5: markdown pipe-escaping in scenario names ----------------------

#[test]
fn markdown_escapes_pipe_in_scenario_name() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (id_a, id_b) = rt.block_on(async { seed_two_runs(dir.path()).await });
    rt.block_on(async {
        let ctx = ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "eval-compare-report-test".into(),
            },
        )
        .await
        .expect("open ApiContext");
        let mut scenario = xvision_engine::api::scenario::get(&ctx, "crypto-bull-q1-2025")
            .await
            .expect("load canonical scenario");
        scenario.id = "sol-pipe-scenario".into();
        scenario.display_name = "SOL|Pipe Scenario".into();
        scenario_store::insert_scenario(&ctx, &scenario)
            .await
            .expect("insert pipe scenario");
        sqlx::query("UPDATE eval_runs SET scenario_id = ? WHERE id = ?")
            .bind(&scenario.id)
            .bind(&id_a)
            .execute(&ctx.db)
            .await
            .expect("update scenario id");
    });

    let out = xvn(
        &[
            "eval",
            "compare",
            "--markdown",
            "--runs",
            &format!("{id_a},{id_b}"),
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let md = String::from_utf8_lossy(&out.stdout);
    assert!(
        md.contains("SOL\\|Pipe Scenario"),
        "markdown must escape pipe characters inside scenario cells; got:\n{md}"
    );

    // Every data row must have the same number of | separators as the header.
    let header_line = md
        .lines()
        .find(|l| l.starts_with("| Run |"))
        .expect("header row not found");
    let header_cols = count_unescaped_pipes(header_line);

    // Only check rows that belong to the main run table (before any `###` heading).
    // The per-asset rollup section has its own column shape.
    let mut in_per_asset_section = false;
    for line in md.lines() {
        if line.starts_with("###") {
            in_per_asset_section = true;
        }
        if in_per_asset_section {
            continue;
        }
        if line.starts_with("| ") && !line.starts_with("| Run |") && !line.starts_with("|---|") {
            let cols = count_unescaped_pipes(line);
            assert_eq!(
                cols, header_cols,
                "data row has {cols} pipes but header has {header_cols}:\n  {line}"
            );
        }
    }
}

// ---- test 6: --runs single id → usage error (exit 2) ----------------------

#[test]
fn runs_flag_single_id_returns_usage_error() {
    let dir = tempdir().unwrap();
    // We don't need seeded data — the validation happens before the engine call.
    let out = xvn(
        &["eval", "compare", "--json", "--runs", "only-one-id"],
        dir.path(),
    );
    let c = code(&out);
    assert_eq!(
        c,
        2,
        "expected XvnExit::Usage=2 for single --runs id, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ---- test 7: --runs comma-separated works as alternative to positional ------

#[test]
fn runs_flag_comma_separated_works_like_positional() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (id_a, id_b) = rt.block_on(async { seed_two_runs(dir.path()).await });
    let id_c = rt.block_on(async { seed_extra_completed_run(dir.path()).await });

    // --runs id1,id2 (comma-separated).
    let out_runs_flag = xvn(
        &["eval", "compare", "--json", "--runs", &format!("{id_a},{id_b}")],
        dir.path(),
    );
    assert!(
        out_runs_flag.status.success(),
        "--runs flag failed: {}",
        String::from_utf8_lossy(&out_runs_flag.stderr)
    );

    // Positional ids.
    let out_positional = xvn(&["eval", "compare", "--json", &id_a, &id_b], dir.path());
    assert!(
        out_positional.status.success(),
        "positional args failed: {}",
        String::from_utf8_lossy(&out_positional.stderr)
    );

    // Both should produce valid JSON for exactly the requested ids. The
    // unrequested third run proves the command is not returning every
    // completed run when parsing `--runs`.
    let body_flag: serde_json::Value = serde_json::from_slice(&out_runs_flag.stdout).unwrap();
    let body_pos: serde_json::Value = serde_json::from_slice(&out_positional.stdout).unwrap();
    let expected = vec![id_a.clone(), id_b.clone()];
    let flag_ids = json_compare_run_ids(&body_flag);
    let pos_ids = json_compare_run_ids(&body_pos);
    assert_eq!(
        flag_ids, expected,
        "--runs comma list must return exactly the requested run ids in compare order"
    );
    assert_eq!(
        pos_ids, expected,
        "positional run ids must return exactly the requested run ids in compare order"
    );
    assert_eq!(
        flag_ids, pos_ids,
        "--runs comma list and positional ids must resolve to the same rows"
    );
    assert!(
        !flag_ids.contains(&id_c) && !pos_ids.contains(&id_c),
        "unrequested run {id_c} must be excluded from both compare outputs"
    );
}

// ---- test 9: per-asset rollup section in --markdown output ------------------

/// Run A trades BTC only; Run B trades ETH only.
/// The markdown output must contain a `### Per-asset` section with rows for
/// both BTC and ETH so an operator can see the per-asset breakdown.
#[test]
fn compare_report_has_per_asset_rollup() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (id_a, id_b) = rt.block_on(async { seed_two_runs(dir.path()).await });

    let out = xvn(
        &[
            "eval",
            "compare",
            "--markdown",
            "--runs",
            &format!("{id_a},{id_b}"),
        ],
        dir.path(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let md = String::from_utf8_lossy(&out.stdout);

    // Must have a per-asset section heading.
    assert!(
        md.contains("### Per-asset"),
        "markdown must contain '### Per-asset' section; got:\n{md}"
    );

    // Run A seeds BTC decisions, Run B seeds ETH decisions — both assets
    // must appear as rows in the per-asset table.
    assert!(
        md.contains("| BTC |"),
        "per-asset table must contain a BTC row; got:\n{md}"
    );
    assert!(
        md.contains("| ETH |"),
        "per-asset table must contain an ETH row; got:\n{md}"
    );
}

// ---- test 8: default text output (no --json/--markdown) is backward-compat -

#[test]
fn default_text_output_backward_compat() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (id_a, id_b) = rt.block_on(async { seed_two_runs(dir.path()).await });

    let out = xvn(&["eval", "compare", &id_a, &id_b], dir.path());
    assert!(
        out.status.success(),
        "default text failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let text = String::from_utf8_lossy(&out.stdout);
    // Header line remains tab-separated; Strategy ID is included separately
    // from the display label.
    assert!(
        text.contains("RUN_ID\tSTRATEGY\tSTRATEGY_ID\tSCENARIO"),
        "default text output must contain tab-separated header; got:\n{text}"
    );
    // Both run ids must appear in the output.
    assert!(text.contains(&id_a), "default output must contain run A id");
    assert!(text.contains(&id_b), "default output must contain run B id");
}
