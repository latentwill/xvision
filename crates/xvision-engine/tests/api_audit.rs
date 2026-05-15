use sqlx::SqlitePool;
use xvision_engine::api::{
    audit::{record, Outcome},
    Actor, ApiContext,
};

async fn pool_with_migration() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

fn ctx_with(pool: SqlitePool, dir: &std::path::Path) -> ApiContext {
    ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.to_path_buf(),
    )
}

#[tokio::test]
async fn audit_records_ok_outcome() {
    let pool = pool_with_migration().await;
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::new(
        pool.clone(),
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    record(&ctx, "strategy", "list", None, None, Outcome::Ok, 12)
        .await
        .unwrap();

    let row: (String, String, String, String, String) =
        sqlx::query_as("SELECT actor, actor_id, domain, operation, outcome FROM api_audit")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, "cli");
    assert_eq!(row.1, "operator");
    assert_eq!(row.2, "strategy");
    assert_eq!(row.3, "list");
    assert_eq!(row.4, "ok");
}

#[tokio::test]
async fn audit_records_error_outcome() {
    let pool = pool_with_migration().await;
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::new(
        pool.clone(),
        Actor::Mcp {
            session_id: "sess-1".into(),
        },
        dir.path().to_path_buf(),
    );
    record(
        &ctx,
        "strategy",
        "create",
        Some("agent-x"),
        Some(r#"{"name":"x"}"#),
        Outcome::Error("validation failed".into()),
        7,
    )
    .await
    .unwrap();

    let (outcome, error): (String, Option<String>) = sqlx::query_as("SELECT outcome, error FROM api_audit")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(outcome, "error");
    assert_eq!(error.as_deref(), Some("validation failed"));
}

#[tokio::test]
async fn audit_records_target_and_args_json() {
    let pool = pool_with_migration().await;
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::new(
        pool.clone(),
        Actor::AgentRunner {
            run_id: "run-42".into(),
        },
        dir.path().to_path_buf(),
    );
    record(
        &ctx,
        "eval",
        "start",
        Some("run-42"),
        Some(r#"{"scenario":"btc-2024"}"#),
        Outcome::Ok,
        100,
    )
    .await
    .unwrap();

    let (target, args_json, duration_ms, actor, actor_id): (
        Option<String>,
        Option<String>,
        i64,
        String,
        Option<String>,
    ) = sqlx::query_as("SELECT target, args_json, duration_ms, actor, actor_id FROM api_audit")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(target.as_deref(), Some("run-42"));
    assert_eq!(args_json.as_deref(), Some(r#"{"scenario":"btc-2024"}"#));
    assert_eq!(duration_ms, 100);
    assert_eq!(actor, "agent_runner");
    assert_eq!(actor_id.as_deref(), Some("run-42"));
}

/// Spec G.1 (v1 gaps Track G): when both target and args_json are passed
/// as None, they must round-trip as SQL NULL — not as the literal string
/// "None" or an empty string. This is load-bearing because consumers query
/// `WHERE target IS NULL` to filter call-site-less records.
#[tokio::test]
async fn audit_records_null_target_and_args() {
    let pool = pool_with_migration().await;
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx_with(pool.clone(), dir.path());
    record(&ctx, "health", "check", None, None, Outcome::Ok, 3)
        .await
        .unwrap();

    let (target, args_json): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT target, args_json FROM api_audit")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(target.is_none(), "target should be NULL, got {target:?}");
    assert!(args_json.is_none(), "args_json should be NULL, got {args_json:?}");
}

/// Spec G.1: 10 concurrent record calls against the same pool must produce
/// 10 distinct rows with 10 distinct ULIDs. Catches a regression where
/// `record` could share state across calls (e.g. a global `Ulid::new()`
/// mutex with an off-by-one) that wouldn't surface in serial tests.
#[tokio::test]
async fn audit_records_concurrent_writes_yield_distinct_ulids() {
    let pool = pool_with_migration().await;
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx_with(pool.clone(), dir.path());

    let mut handles = Vec::with_capacity(10);
    for i in 0..10 {
        let ctx = ctx.clone();
        handles.push(tokio::spawn(async move {
            record(
                &ctx,
                "concurrent",
                "write",
                Some(&format!("target-{i}")),
                None,
                Outcome::Ok,
                i,
            )
            .await
            .unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM api_audit")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 10, "expected 10 rows, got {count}");

    let (distinct_ids,): (i64,) = sqlx::query_as("SELECT COUNT(DISTINCT id) FROM api_audit")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(distinct_ids, 10, "expected 10 distinct ULIDs, got {distinct_ids}");
}
