use tempfile::TempDir;
use xvision_engine::api::{Actor, ApiContext};

/// Verifies that migration 069 was applied and all three nanochat tables exist.
#[tokio::test]
async fn nanochat_tables_exist_after_migration() {
    let tmp = TempDir::new().unwrap();
    let ctx = ApiContext::open(tmp.path(), Actor::Cli { user: "test".into() })
        .await
        .expect("open ApiContext must apply all migrations including 069");

    for table in &[
        "trained_models",
        "autoresearch_runs",
        "autoresearch_experiments",
        "xvn_config",
    ] {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?")
                .bind(*table)
                .fetch_one(&ctx.db)
                .await
                .unwrap();
        assert_eq!(count, 1, "table '{table}' must exist after migration 069");
    }

    // Partial unique index must be present.
    let idx: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_autoresearch_single_running'",
    )
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    assert_eq!(
        idx, 1,
        "partial unique index idx_autoresearch_single_running must exist"
    );
}
