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
