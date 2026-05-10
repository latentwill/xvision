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

#[tokio::test]
async fn audit_records_ok_outcome() {
    let pool = pool_with_migration().await;
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext {
        db: pool.clone(),
        actor: Actor::Cli {
            user: "operator".into(),
        },
        xvn_home: dir.path().to_path_buf(),
    };
    record(&ctx, "strategy", "list", None, None, Outcome::Ok, 12)
        .await
        .unwrap();

    let row: (String, String, String, String, String) = sqlx::query_as(
        "SELECT actor, actor_id, domain, operation, outcome FROM api_audit",
    )
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
    let ctx = ApiContext {
        db: pool.clone(),
        actor: Actor::Mcp {
            session_id: "sess-1".into(),
        },
        xvn_home: dir.path().to_path_buf(),
    };
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

    let (outcome, error): (String, Option<String>) =
        sqlx::query_as("SELECT outcome, error FROM api_audit")
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
    let ctx = ApiContext {
        db: pool.clone(),
        actor: Actor::AgentRunner {
            run_id: "run-42".into(),
        },
        xvn_home: dir.path().to_path_buf(),
    };
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
    ) = sqlx::query_as(
        "SELECT target, args_json, duration_ms, actor, actor_id FROM api_audit",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(target.as_deref(), Some("run-42"));
    assert_eq!(args_json.as_deref(), Some(r#"{"scenario":"btc-2024"}"#));
    assert_eq!(duration_ms, 100);
    assert_eq!(actor, "agent_runner");
    assert_eq!(actor_id.as_deref(), Some("run-42"));
}
