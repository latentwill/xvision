//! V2E eval-net-of-inference-cost-metric — unit and integration tests.
//!
//! Test groups:
//! - `inference_cost_metric_net_return_pct_math`: pure math for
//!   `compute_net_return_pct` (zero cost, normal case, None when no cost).
//! - `inference_cost_metric_dominance_threshold`: threshold logic for
//!   `inference_cost_dominates`.
//! - `inference_cost_metric_backward_compat`: old `MetricsSummary` JSON (without
//!   the new optional fields) deserializes cleanly to `None` values.
//! - `inference_cost_metric_compare_net_column`: `ComparisonRunSummary` carries
//!   `net_return_pct` after `compare_runs`.
//! - `inference_cost_metric_patch_metrics`: `RunStore::patch_metrics` persists
//!   the new fields and round-trips them correctly.

use chrono::{Duration, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use xvision_engine::eval::compare::compare_runs;
use xvision_engine::eval::metrics::{
    compute_net_return_pct, inference_cost_dominates, INFERENCE_COST_DOMINANCE_THRESHOLD,
};
use xvision_engine::eval::run::{MetricsSummary, RunMode};
use xvision_engine::eval::{DecisionRow, Run, RunStore};

// ─── helpers ────────────────────────────────────────────────────────────────

async fn in_memory_store() -> RunStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/016_eval_reviews.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/017_eval_findings_review_columns.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // V2E trace-surface foundation.
    sqlx::query(include_str!("../migrations/026_trace_surface_foundation.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // V2E candle integrity + manifest (bars_content_hash, etc.).
    sqlx::query(include_str!("../migrations/027_run_bars_manifest.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/037_review_annotations_and_autofire.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(include_str!("../migrations/038_eval_runs_live_config.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/065_eval_run_source_and_unrealized_pnl.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    RunStore::new(pool)
}

fn base_metrics() -> MetricsSummary {
    MetricsSummary {
        total_return_pct: 5.0,
        sharpe: 1.2,
        max_drawdown_pct: 3.0,
        win_rate: 0.60,
        n_trades: 8,
        n_decisions: 10,
        baselines: None,
        inference_cost_quote_total: None,
        net_return_pct: None,
        ..Default::default()
    }
}

async fn seed_run(store: &RunStore, metrics: MetricsSummary) -> Run {
    let run = Run::new_queued("agt".into(), "scen".into(), RunMode::Backtest);
    store.create(&run).await.unwrap();

    let t0 = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    for i in 0..3usize {
        let ts = t0 + Duration::hours(i as i64);
        store
            .record_equity(&run.id, ts, 10_000.0 + i as f64 * 100.0)
            .await
            .unwrap();
    }
    store
        .record_decision(&DecisionRow {
            run_id: run.id.clone(),
            decision_index: 0,
            timestamp: t0,
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
            delayed: None,
        })
        .await
        .unwrap();
    store.finalize(&run.id, &metrics).await.unwrap();
    run
}

// ─── net_return_pct math ─────────────────────────────────────────────────────

#[test]
fn inference_cost_metric_net_return_pct_math_zero_cost() {
    // Zero cost → net equals gross (cost doesn't reduce return).
    // Note: aggregate_eval_run_inference_cost filters out <= 0 from the DB,
    // but compute_net_return_pct itself accepts any Some value.
    // When cost is explicitly Some(0.0), net = gross - 0 = gross.
    let net = compute_net_return_pct(5.0, Some(0.0), 10_000.0);
    assert_eq!(net, Some(5.0));
}

#[test]
fn inference_cost_metric_net_return_pct_math_normal_case() {
    // gross = 5.0%, capital = 10_000, cost = 50 USD
    // net = 5.0 - (50 / 10_000 * 100) = 5.0 - 0.5 = 4.5%
    let net = compute_net_return_pct(5.0, Some(50.0), 10_000.0);
    let expected = 5.0 - (50.0 / 10_000.0 * 100.0);
    assert!(
        (net.unwrap() - expected).abs() < 1e-10,
        "expected {expected:.6} got {:?}",
        net
    );
}

#[test]
fn inference_cost_metric_net_return_pct_math_no_cost() {
    // No cost data → None.
    let net = compute_net_return_pct(5.0, None, 10_000.0);
    assert_eq!(net, None);
}

#[test]
fn inference_cost_metric_net_return_pct_math_zero_capital() {
    // Zero capital → None (avoid division by zero).
    let net = compute_net_return_pct(5.0, Some(10.0), 0.0);
    assert_eq!(net, None);
}

#[test]
fn inference_cost_metric_net_return_pct_math_negative_gross() {
    // Negative gross return with cost worsens net further.
    // gross = -2.0%, capital = 10_000, cost = 20 USD
    // net = -2.0 - (20 / 10_000 * 100) = -2.0 - 0.2 = -2.2%
    let net = compute_net_return_pct(-2.0, Some(20.0), 10_000.0);
    let expected = -2.0 - (20.0 / 10_000.0 * 100.0);
    assert!((net.unwrap() - expected).abs() < 1e-10);
}

// ─── dominance threshold ─────────────────────────────────────────────────────

#[test]
fn inference_cost_metric_dominance_threshold_not_exceeded() {
    // cost = 20, gross_quote = 100. ratio = 0.2 < 0.5 threshold.
    assert!(!inference_cost_dominates(
        100.0,
        20.0,
        INFERENCE_COST_DOMINANCE_THRESHOLD
    ));
}

#[test]
fn inference_cost_metric_dominance_threshold_exceeded() {
    // cost = 60, gross_quote = 100. ratio = 0.6 > 0.5 threshold.
    assert!(inference_cost_dominates(
        100.0,
        60.0,
        INFERENCE_COST_DOMINANCE_THRESHOLD
    ));
}

#[test]
fn inference_cost_metric_dominance_threshold_exactly_at_boundary() {
    // cost = 50, gross_quote = 100. ratio = 0.5, not EXCEEDED (not strictly >).
    assert!(!inference_cost_dominates(
        100.0,
        50.0,
        INFERENCE_COST_DOMINANCE_THRESHOLD
    ));
}

#[test]
fn inference_cost_metric_dominance_threshold_zero_gross() {
    // zero gross return → any positive cost exceeds threshold.
    assert!(inference_cost_dominates(
        0.0,
        1.0,
        INFERENCE_COST_DOMINANCE_THRESHOLD
    ));
    // zero gross, zero cost → false.
    assert!(!inference_cost_dominates(
        0.0,
        0.0,
        INFERENCE_COST_DOMINANCE_THRESHOLD
    ));
}

#[test]
fn inference_cost_metric_dominance_threshold_negative_gross() {
    // Works with absolute values: |gross| = 100, |cost| = 60 > 0.5 * 100.
    assert!(inference_cost_dominates(
        -100.0,
        60.0,
        INFERENCE_COST_DOMINANCE_THRESHOLD
    ));
    assert!(!inference_cost_dominates(
        -100.0,
        20.0,
        INFERENCE_COST_DOMINANCE_THRESHOLD
    ));
}

// ─── backward compat ─────────────────────────────────────────────────────────

#[test]
fn inference_cost_metric_backward_compat_old_json_deserializes_to_none() {
    // Old stored MetricsSummary JSON without the new optional fields.
    let old_json = r#"{
        "total_return_pct": 3.125,
        "sharpe": 1.5,
        "max_drawdown_pct": 7.2,
        "win_rate": 0.58,
        "n_trades": 12,
        "n_decisions": 15
    }"#;
    let m: MetricsSummary = serde_json::from_str(old_json).unwrap();
    assert_eq!(m.total_return_pct, 3.125);
    assert_eq!(m.inference_cost_quote_total, None);
    assert_eq!(m.net_return_pct, None);
}

#[test]
fn inference_cost_metric_backward_compat_gross_return_pct_alias() {
    // `gross_return_pct` serde alias deserializes as `total_return_pct`.
    let alias_json = r#"{
        "gross_return_pct": 4.56,
        "sharpe": 0.9,
        "max_drawdown_pct": 2.0,
        "win_rate": 0.5,
        "n_trades": 5,
        "n_decisions": 6
    }"#;
    let m: MetricsSummary = serde_json::from_str(alias_json).unwrap();
    assert_eq!(m.total_return_pct, 4.56);
    assert_eq!(m.gross_return_pct(), 4.56);
    assert_eq!(m.inference_cost_quote_total, None);
    assert_eq!(m.net_return_pct, None);
}

#[test]
fn inference_cost_metric_backward_compat_new_fields_round_trip() {
    let m = MetricsSummary {
        total_return_pct: 2.0,
        sharpe: 0.7,
        max_drawdown_pct: 1.0,
        win_rate: 0.5,
        n_trades: 3,
        n_decisions: 4,
        baselines: None,
        inference_cost_quote_total: Some(15.50),
        net_return_pct: Some(1.845),
        ..Default::default()
    };
    let json = serde_json::to_string(&m).unwrap();
    let m2: MetricsSummary = serde_json::from_str(&json).unwrap();
    assert_eq!(m2.inference_cost_quote_total, Some(15.50));
    assert!((m2.net_return_pct.unwrap() - 1.845).abs() < 1e-10);
}

// ─── patch_metrics round-trip ─────────────────────────────────────────────────

#[tokio::test]
async fn inference_cost_metric_patch_metrics_persists_and_reads_back() {
    let store = in_memory_store().await;

    let mut m = base_metrics();
    let run = seed_run(&store, m.clone()).await;

    // Enrich with cost.
    m.inference_cost_quote_total = Some(25.0);
    m.net_return_pct = Some(4.75);

    let patched = store.patch_metrics(&run.id, &m).await.unwrap();
    assert!(patched, "patch should report a row was affected");

    let reloaded = store.get(&run.id).await.unwrap();
    let metrics = reloaded.metrics.unwrap();
    assert_eq!(metrics.inference_cost_quote_total, Some(25.0));
    assert!((metrics.net_return_pct.unwrap() - 4.75).abs() < 1e-10);
}

// ─── compare net_return_pct column ───────────────────────────────────────────

#[tokio::test]
async fn inference_cost_metric_compare_net_column_populated() {
    let store = in_memory_store().await;

    let mut m_a = base_metrics();
    m_a.inference_cost_quote_total = Some(50.0);
    m_a.net_return_pct = Some(4.5);
    let run_a = seed_run(&store, m_a).await;

    let m_b = base_metrics(); // no cost data
    let run_b = seed_run(&store, m_b).await;

    let report = compare_runs(
        &[run_a.id.clone(), run_b.id.clone()],
        &store,
        &xvision_engine::eval::compare::CompareOptions::default(),
    )
    .await
    .unwrap();

    let summary_a = report.runs.iter().find(|r| r.id == run_a.id).unwrap();
    let summary_b = report.runs.iter().find(|r| r.id == run_b.id).unwrap();

    // Run A: net_return_pct hoisted from metrics.
    assert_eq!(summary_a.net_return_pct, Some(4.5));
    // Run B: no cost data → None.
    assert_eq!(summary_b.net_return_pct, None);
}

#[tokio::test]
async fn inference_cost_metric_compare_net_column_ordering_matches_run_ids() {
    let store = in_memory_store().await;

    let mut m = base_metrics();
    m.net_return_pct = Some(3.5);
    let run_a = seed_run(&store, m.clone()).await;
    m.net_return_pct = Some(7.1);
    let run_b = seed_run(&store, m).await;

    let report = compare_runs(
        &[run_a.id.clone(), run_b.id.clone()],
        &store,
        &xvision_engine::eval::compare::CompareOptions::default(),
    )
    .await
    .unwrap();

    // Output ordering must mirror input ordering.
    assert_eq!(report.runs[0].id, run_a.id);
    assert_eq!(report.runs[1].id, run_b.id);
    assert_eq!(report.runs[0].net_return_pct, Some(3.5));
    assert_eq!(report.runs[1].net_return_pct, Some(7.1));
}
