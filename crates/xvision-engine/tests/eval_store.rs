use chrono::{TimeZone, Utc};
mod common;

use tempfile::TempDir;
use xvision_engine::eval::{DecisionRow, ListFilter, MetricsSummary, Run, RunMode, RunStatus, RunStore};

async fn store_with_migration() -> (RunStore, TempDir, String) {
    let (ctx, dir) = common::open_api_context().await;
    let scenario_id = common::seeded_scenario_id(&ctx).await;
    (RunStore::new(ctx.db), dir, scenario_id)
}

fn fresh_run(scenario: &str, mode: RunMode) -> Run {
    Run::new_queued("strategy-hash-x".into(), scenario.into(), mode)
}

#[tokio::test]
async fn list_returns_empty_for_fresh_pool() {
    let (store, _db_dir, _scenario_id) = store_with_migration().await;
    let out = store.list(ListFilter::default()).await.unwrap();
    assert!(out.is_empty());
}

#[tokio::test]
async fn create_then_get_round_trips() {
    let (store, _db_dir, scenario_id) = store_with_migration().await;
    let run = fresh_run(&scenario_id, RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();
    let back = store.get(&id).await.unwrap();
    assert_eq!(back.id, id);
    assert_eq!(back.scenario_id, scenario_id);
    assert_eq!(back.mode, RunMode::Backtest);
    assert_eq!(back.status, RunStatus::Queued);
    assert!(back.metrics.is_none());
    assert!(back.completed_at.is_none());
}

#[tokio::test]
async fn get_unknown_id_errors() {
    let (store, _db_dir, _scenario_id) = store_with_migration().await;
    let r = store.get("missing").await;
    assert!(r.is_err(), "get on unknown id should error");
}

#[tokio::test]
async fn update_status_transitions_queued_to_running_to_completed() {
    let (store, _db_dir, scenario_id) = store_with_migration().await;
    let run = fresh_run(&scenario_id, RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();

    store.update_status(&id, RunStatus::Running, None).await.unwrap();
    assert_eq!(store.get(&id).await.unwrap().status, RunStatus::Running);

    store
        .update_status(&id, RunStatus::Completed, None)
        .await
        .unwrap();
    assert_eq!(store.get(&id).await.unwrap().status, RunStatus::Completed);
}

#[tokio::test]
async fn update_status_failed_persists_error_message() {
    let (store, _db_dir, scenario_id) = store_with_migration().await;
    let run = fresh_run(&scenario_id, RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();
    store
        .update_status(&id, RunStatus::Failed, Some("alpaca timeout"))
        .await
        .unwrap();
    let back = store.get(&id).await.unwrap();
    assert_eq!(back.status, RunStatus::Failed);
    assert_eq!(back.error.as_deref(), Some("alpaca timeout"));
}

#[tokio::test]
async fn cancelled_run_cannot_be_revived_or_finalized() {
    let (store, _db_dir, scenario_id) = store_with_migration().await;
    let run = fresh_run(&scenario_id, RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();

    assert!(store.cancel_active(&id, "stopped by user").await.unwrap());
    assert!(!store.begin_running(&id).await.unwrap());

    let metrics = MetricsSummary {
        total_return_pct: 12.5,
        sharpe: 1.42,
        max_drawdown_pct: -8.3,
        win_rate: 0.58,
        n_trades: 17,
        n_decisions: 42,
        baselines: None,
        ..Default::default()
    };
    let err = store.finalize(&id, &metrics).await.unwrap_err();
    assert!(
        err.to_string().contains("already cancelled"),
        "unexpected finalize error: {err:#}",
    );

    let back = store.get(&id).await.unwrap();
    assert_eq!(back.status, RunStatus::Cancelled);
    assert_eq!(back.error.as_deref(), Some("stopped by user"));
    assert!(back.metrics.is_none());
}

#[tokio::test]
async fn fail_active_does_not_overwrite_cancelled_run() {
    let (store, _db_dir, scenario_id) = store_with_migration().await;
    let run = fresh_run(&scenario_id, RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();

    assert!(store.cancel_active(&id, "stopped by user").await.unwrap());
    assert!(!store.fail_active(&id, "late parser failure").await.unwrap());

    let back = store.get(&id).await.unwrap();
    assert_eq!(back.status, RunStatus::Cancelled);
    assert_eq!(back.error.as_deref(), Some("stopped by user"));
}

#[tokio::test]
async fn update_status_does_not_revive_terminal_run() {
    let (store, _db_dir, scenario_id) = store_with_migration().await;
    let run = fresh_run(&scenario_id, RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();
    store
        .update_status(&id, RunStatus::Failed, Some("parser failure"))
        .await
        .unwrap();

    let err = store
        .update_status(&id, RunStatus::Running, None)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("already failed"),
        "unexpected update_status error: {err:#}",
    );
    assert_eq!(store.get(&id).await.unwrap().status, RunStatus::Failed);
}

#[tokio::test]
async fn finalize_sets_metrics_status_and_completed_at() {
    let (store, _db_dir, scenario_id) = store_with_migration().await;
    let run = fresh_run(&scenario_id, RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();

    let metrics = MetricsSummary {
        total_return_pct: 12.5,
        sharpe: 1.42,
        max_drawdown_pct: -8.3,
        win_rate: 0.58,
        n_trades: 17,
        n_decisions: 42,
        baselines: None,
        ..Default::default()
    };
    store.finalize(&id, &metrics).await.unwrap();
    let back = store.get(&id).await.unwrap();
    assert_eq!(back.status, RunStatus::Completed);
    assert!(back.completed_at.is_some());
    assert_eq!(back.metrics.as_ref(), Some(&metrics));
}

#[tokio::test]
async fn list_with_strategy_filter_only_returns_matching() {
    let (store, _db_dir, scenario_id) = store_with_migration().await;
    let mut a = fresh_run(&scenario_id, RunMode::Backtest);
    a.agent_id = "hash-A".into();
    let mut b = fresh_run(&scenario_id, RunMode::Backtest);
    b.agent_id = "hash-B".into();
    store.create(&a).await.unwrap();
    store.create(&b).await.unwrap();

    let out = store
        .list(ListFilter {
            agent_id: Some("hash-A".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].agent_id, "hash-A");
}

#[tokio::test]
async fn list_with_status_filter_only_returns_matching() {
    let (store, _db_dir, scenario_id) = store_with_migration().await;
    let r1 = fresh_run(&scenario_id, RunMode::Backtest);
    let r2 = fresh_run(&scenario_id, RunMode::Backtest);
    store.create(&r1).await.unwrap();
    store.create(&r2).await.unwrap();
    store
        .update_status(&r1.id, RunStatus::Completed, None)
        .await
        .unwrap();

    let out = store
        .list(ListFilter {
            status: Some(RunStatus::Completed),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id, r1.id);
}

#[tokio::test]
async fn record_decision_and_read_decisions_in_index_order() {
    let (store, _db_dir, scenario_id) = store_with_migration().await;
    let run = fresh_run(&scenario_id, RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();

    // Insert out of order to verify the read orders by decision_index.
    let row = |idx: u32, action: &str, ts_minutes: i64| DecisionRow {
        run_id: id.clone(),
        decision_index: idx,
        timestamp: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap() + chrono::Duration::minutes(ts_minutes),
        asset: "BTC/USD".into(),
        action: action.into(),
        conviction: Some(0.7),
        justification: Some(format!("decision {idx}")),
        reasoning: Some(format!("reasoning {idx}")),
        order_size: Some(0.05),
        fill_price: Some(70_000.0 + (idx as f64) * 100.0),
        fill_size: Some(0.05),
        fee: Some(1.25),
        pnl_realized: None,
    };
    store.record_decision(&row(2, "long_open", 30)).await.unwrap();
    store.record_decision(&row(0, "hold", 0)).await.unwrap();
    store.record_decision(&row(1, "long_open", 15)).await.unwrap();

    let read = store.read_decisions(&id).await.unwrap();
    assert_eq!(read.len(), 3);
    assert_eq!(
        read.iter().map(|d| d.decision_index).collect::<Vec<_>>(),
        vec![0, 1, 2],
        "decisions must come back in decision_index order",
    );
    assert_eq!(read[0].action, "hold");
    assert_eq!(read[2].action, "long_open");
}

#[tokio::test]
async fn record_decision_duplicate_index_errors() {
    let (store, _db_dir, scenario_id) = store_with_migration().await;
    let run = fresh_run(&scenario_id, RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();

    let row = DecisionRow {
        run_id: id.clone(),
        decision_index: 0,
        timestamp: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
        asset: "BTC/USD".into(),
        action: "hold".into(),
        conviction: None,
        justification: None,
        reasoning: None,
        order_size: None,
        fill_price: None,
        fill_size: None,
        fee: None,
        pnl_realized: None,
    };
    store.record_decision(&row).await.unwrap();
    let err = store.record_decision(&row).await;
    assert!(err.is_err(), "duplicate (run_id, decision_index) must error");
}

#[tokio::test]
async fn record_and_read_equity_curve_in_timestamp_order() {
    let (store, _db_dir, scenario_id) = store_with_migration().await;
    let run = fresh_run(&scenario_id, RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();

    let t0 = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    store
        .record_equity(&id, t0 + chrono::Duration::hours(2), 11_000.0)
        .await
        .unwrap();
    store.record_equity(&id, t0, 10_000.0).await.unwrap();
    store
        .record_equity(&id, t0 + chrono::Duration::hours(1), 10_500.0)
        .await
        .unwrap();

    let curve = store.read_equity_curve(&id).await.unwrap();
    assert_eq!(curve.len(), 3);
    assert_eq!(curve[0].1, 10_000.0);
    assert_eq!(curve[1].1, 10_500.0);
    assert_eq!(curve[2].1, 11_000.0);
}
