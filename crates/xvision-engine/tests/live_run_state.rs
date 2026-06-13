mod support;

#[tokio::test]
async fn migration_creates_live_run_state_table() {
    let ctx = support::api_context_fresh().await; // production migration path → includes 065
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='live_run_state'",
    )
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn create_persists_venue_label_from_live_config() {
    let ctx = support::api_context_fresh().await;
    let store = xvision_engine::eval::store::RunStore::new(ctx.db.clone());
    let run = support::live_run_with_venue(xvision_engine::safety::venue::VenueLabel::Testnet);
    store.create(&run).await.unwrap();
    let venue: String = sqlx::query_scalar("SELECT venue_label FROM eval_runs WHERE id = ?")
        .bind(&run.id).fetch_one(&ctx.db).await.unwrap();
    assert_eq!(venue, "testnet");
}

#[tokio::test]
async fn list_filter_mode_selects_only_live_runs() {
    use xvision_engine::eval::run::RunMode;
    use xvision_engine::eval::store::{ListFilter, RunStore};
    use xvision_engine::safety::venue::VenueLabel;
    let ctx = support::api_context_fresh().await;
    let store = RunStore::new(ctx.db.clone());
    store.create(&support::backtest_run()).await.unwrap();
    store
        .create(&support::live_run_with_venue(VenueLabel::Paper))
        .await
        .unwrap();
    let live = store
        .list(ListFilter {
            mode: Some(RunMode::Live),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].mode, RunMode::Live);
}

#[tokio::test]
async fn live_state_upsert_inserts_then_updates_in_place() {
    use xvision_engine::eval::live_run_state::{LiveRunState, LiveStateStore};
    use xvision_engine::eval::store::RunStore;
    use xvision_engine::safety::venue::VenueLabel;
    let ctx = support::api_context_fresh().await;
    let store = RunStore::new(ctx.db.clone());
    let run = support::live_run_with_venue(VenueLabel::Paper);
    store.create(&run).await.unwrap();

    let lss = LiveStateStore::new(ctx.db.clone());
    let mut snap = LiveRunState {
        run_id: run.id.clone(), strategy_id: Some("strat-1".into()),
        strategy_name: Some("Trend v2".into()), deployed_capital_usd: 10_000.0,
        equity_usd: Some(10_050.0), unrealized_pnl_usd: Some(50.0), realized_pnl_usd: Some(0.0),
        realized_today_usd: Some(0.0), daily_loss_remaining_usd: Some(500.0), drawdown_pct: Some(0.0),
        peak_equity_usd: Some(10_050.0), risk_veto_count: 0,
        last_decision_at: Some("2026-06-13T12:00:00Z".into()), updated_at: "2026-06-13T12:00:00Z".into(),
    };
    lss.upsert(&snap).await.unwrap();
    snap.equity_usd = Some(9_800.0); snap.risk_veto_count = 2;
    lss.upsert(&snap).await.unwrap();

    let got = lss.get(&run.id).await.unwrap().expect("row present");
    assert_eq!(got.equity_usd, Some(9_800.0));
    assert_eq!(got.risk_veto_count, 2);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM live_run_state WHERE run_id = ?")
        .bind(&run.id).fetch_one(&ctx.db).await.unwrap();
    assert_eq!(n, 1);
}
