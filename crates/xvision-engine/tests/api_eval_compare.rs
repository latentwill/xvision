//! Phase 3.D Task 10 — `api::eval::compare` tests. Builds two completed
//! runs in the eval store (with metrics, equity curves, and findings) then
//! asserts the compare api returns a `ComparisonReport` that pairs runs ↔
//! curves ↔ findings correctly.

use chrono::{Duration, TimeZone, Utc};
mod common;

use common::{open_api_context as ctx_with_eval_tables, seeded_scenario_id};
use xvision_engine::api::eval::{self, CompareRunsRequest};
use xvision_engine::api::strategy as api_strategy;
use xvision_engine::api::ApiError;
use xvision_engine::authoring::CreateStrategyReq;
use xvision_engine::eval::findings::{Finding, Severity};
use xvision_engine::eval::run::{MetricsSummary, RunMode};
use xvision_engine::eval::{DecisionRow, Run, RunStore};

async fn seed_completed_run(
    store: &RunStore,
    agent_id: &str,
    scenario_id: &str,
    metrics: MetricsSummary,
    n_equity_samples: usize,
) -> Run {
    let run = Run::new_queued(agent_id.into(), scenario_id.into(), RunMode::Backtest);
    store.create(&run).await.unwrap();

    // Equity samples — synthetic monotonically-increasing curve so we can
    // assert ordering after re-read.
    let t0 = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    for i in 0..n_equity_samples {
        let ts = t0 + Duration::hours(i as i64);
        let equity = 10_000.0 + (i as f64) * 100.0;
        store.record_equity(&run.id, ts, equity).await.unwrap();
    }

    // One decision row so the findings have something to point at.
    store
        .record_decision(&DecisionRow {
            run_id: run.id.clone(),
            decision_index: 0,
            timestamp: t0,
            asset: "BTC".into(),
            action: "long_open".into(),
            conviction: Some(0.75),
            justification: Some("seeded".into()),
            reasoning: Some("seeded".into()),
            order_size: Some(0.1),
            fill_price: Some(40_000.0),
            fill_size: Some(0.1),
            fee: Some(1.0),
            pnl_realized: None,
            delayed: None,
        })
        .await
        .unwrap();

    store.finalize(&run.id, &metrics).await.unwrap();
    run
}

async fn seed_finding(store: &RunStore, run_id: &str, summary: &str) -> Finding {
    let f = Finding {
        id: ulid::Ulid::new().to_string(),
        run_id: run_id.into(),
        kind: "regime_dependence".into(),
        severity: Severity::Warning,
        summary: summary.into(),
        evidence: serde_json::json!({"foo": "bar"}),
        extracted_at: Utc::now(),
        schema_version: "v1".into(),
        evidence_cycle_ids: None,
        produced_by_check: None,
        eval_review_id: None,
        review_type: None,
        confidence: None,
        title: None,
        description: None,
        recommendation: None,
        created_at: None,
    };
    store.record_finding(&f).await.unwrap();
    f
}

fn metrics(total_return_pct: f64, sharpe: f64) -> MetricsSummary {
    MetricsSummary {
        total_return_pct,
        sharpe,
        max_drawdown_pct: 5.0,
        win_rate: 0.55,
        n_trades: 10,
        n_decisions: 12,
        baselines: None,
        ..Default::default()
    }
}

#[tokio::test]
async fn compare_returns_two_runs_with_curves_and_findings() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;

    let run_a = seed_completed_run(&store, "h-A", &scenario_id, metrics(15.0, 1.2), 5).await;
    let run_b = seed_completed_run(&store, "h-B", &scenario_id, metrics(8.5, 0.7), 7).await;
    let _f_a = seed_finding(&store, &run_a.id, "A regime quirk").await;
    let _f_b = seed_finding(&store, &run_b.id, "B regime quirk").await;

    let report = eval::compare(
        &ctx,
        CompareRunsRequest {
            run_ids: vec![run_a.id.clone(), run_b.id.clone()],
            allow_manifest_mismatch: false,
        },
    )
    .await
    .unwrap();

    assert_eq!(report.runs.len(), 2);
    assert_eq!(report.equity_curves.len(), 2);
    assert_eq!(report.findings.len(), 2);

    // Order is preserved — runs[0] is run_a, runs[1] is run_b.
    assert_eq!(report.runs[0].id, run_a.id);
    assert_eq!(report.runs[1].id, run_b.id);
    assert_eq!(report.runs[0].agent_id, "h-A");
    assert_eq!(report.runs[1].agent_id, "h-B");

    // Metrics carry through.
    assert_eq!(report.runs[0].metrics.as_ref().unwrap().total_return_pct, 15.0);
    assert_eq!(report.runs[1].metrics.as_ref().unwrap().sharpe, 0.7);

    // Equity curves are paired with the right run_id and sized correctly.
    assert_eq!(report.equity_curves[0].run_id, run_a.id);
    assert_eq!(report.equity_curves[0].samples.len(), 5);
    assert_eq!(report.equity_curves[1].run_id, run_b.id);
    assert_eq!(report.equity_curves[1].samples.len(), 7);

    // Equity curves come back in timestamp-ascending order.
    let samples_a = &report.equity_curves[0].samples;
    for w in samples_a.windows(2) {
        assert!(w[0].timestamp < w[1].timestamp);
    }
}

#[tokio::test]
async fn compare_populates_strategy_name_from_manifest() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;

    let strategy_a = api_strategy::create_strategy(
        &ctx,
        CreateStrategyReq {
            name: "Readable Alpha".into(),
            creator: Some("@tester".into()),
        },
    )
    .await
    .unwrap();
    let strategy_b = api_strategy::create_strategy(
        &ctx,
        CreateStrategyReq {
            name: "Readable Beta".into(),
            creator: Some("@tester".into()),
        },
    )
    .await
    .unwrap();

    let run_a = seed_completed_run(&store, &strategy_a.id, &scenario_id, metrics(15.0, 1.2), 5).await;
    let run_b = seed_completed_run(&store, &strategy_b.id, &scenario_id, metrics(8.5, 0.7), 5).await;

    let report = eval::compare(
        &ctx,
        CompareRunsRequest {
            run_ids: vec![run_a.id, run_b.id],
            allow_manifest_mismatch: false,
        },
    )
    .await
    .unwrap();

    assert_eq!(report.runs[0].strategy_name.as_deref(), Some("Readable Alpha"));
    assert_eq!(report.runs[1].strategy_name.as_deref(), Some("Readable Beta"));
}

#[tokio::test]
async fn compare_returns_not_found_for_unknown_run() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;
    let real = seed_completed_run(&store, "h", &scenario_id, metrics(1.0, 0.1), 1).await;

    let err = eval::compare(
        &ctx,
        CompareRunsRequest {
            run_ids: vec![real.id, "does-not-exist".into()],
            allow_manifest_mismatch: false,
        },
    )
    .await
    .unwrap_err();

    match err {
        ApiError::NotFound(msg) => assert!(
            msg.contains("does-not-exist"),
            "NotFound message should name the missing run id, got: {msg}"
        ),
        other => panic!("expected ApiError::NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn compare_rejects_empty_run_ids() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let err = eval::compare(
        &ctx,
        CompareRunsRequest {
            run_ids: vec![],
            allow_manifest_mismatch: false,
        },
    )
    .await
    .unwrap_err();
    match err {
        ApiError::Validation(msg) => assert!(
            msg.to_lowercase().contains("at least one"),
            "Validation message should explain min run count, got: {msg}"
        ),
        other => panic!("expected ApiError::Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn compare_rejects_single_run_id() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;
    let real = seed_completed_run(&store, "h", &scenario_id, metrics(1.0, 0.1), 1).await;

    let err = eval::compare(
        &ctx,
        CompareRunsRequest {
            run_ids: vec![real.id],
            allow_manifest_mismatch: false,
        },
    )
    .await
    .unwrap_err();
    match err {
        ApiError::Validation(msg) => assert!(
            msg.to_lowercase().contains("at least two"),
            "Validation message should explain min run count, got: {msg}"
        ),
        other => panic!("expected ApiError::Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn compare_handles_run_with_no_findings() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;

    let run_a = seed_completed_run(&store, "h-A", &scenario_id, metrics(2.0, 0.5), 3).await;
    let run_b = seed_completed_run(&store, "h-B", &scenario_id, metrics(3.0, 0.6), 3).await;
    // Only run_a gets a finding; run_b is finding-free.
    let _f = seed_finding(&store, &run_a.id, "A only").await;

    let report = eval::compare(
        &ctx,
        CompareRunsRequest {
            run_ids: vec![run_a.id.clone(), run_b.id.clone()],
            allow_manifest_mismatch: false,
        },
    )
    .await
    .unwrap();

    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].run_id, run_a.id);
}

#[tokio::test]
async fn compare_writes_audit_row() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;
    let run_a = seed_completed_run(&store, "h-A", &scenario_id, metrics(2.0, 0.5), 3).await;
    let run_b = seed_completed_run(&store, "h-B", &scenario_id, metrics(3.0, 0.6), 3).await;

    let _ = eval::compare(
        &ctx,
        CompareRunsRequest {
            run_ids: vec![run_a.id.clone(), run_b.id.clone()],
            allow_manifest_mismatch: false,
        },
    )
    .await
    .unwrap();

    let row = sqlx::query(
        "SELECT domain, operation, target, outcome FROM api_audit \
         WHERE operation = 'compare' ORDER BY occurred_at DESC LIMIT 1",
    )
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    let domain: String = sqlx::Row::try_get(&row, "domain").unwrap();
    let operation: String = sqlx::Row::try_get(&row, "operation").unwrap();
    let target: Option<String> = sqlx::Row::try_get(&row, "target").unwrap();
    let outcome: String = sqlx::Row::try_get(&row, "outcome").unwrap();
    assert_eq!(domain, "eval");
    assert_eq!(operation, "compare");
    assert_eq!(outcome, "ok");
    // The audit `target` is the comma-joined run_ids so operators can grep.
    let target = target.expect("compare audit row should have target=run_ids");
    assert!(target.contains(&run_a.id));
    assert!(target.contains(&run_b.id));
}
