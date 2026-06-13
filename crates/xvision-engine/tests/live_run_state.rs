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
