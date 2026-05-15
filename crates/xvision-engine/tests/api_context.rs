use sqlx::SqlitePool;
use xvision_engine::api::{Actor, ApiContext};

#[tokio::test]
async fn api_context_constructs_with_actor() {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    assert!(matches!(ctx.actor, Actor::Cli { .. }));
}

#[test]
fn actor_enum_covers_all_callers() {
    use Actor::*;
    let _ = [
        Cli { user: "u".into() },
        Mcp {
            session_id: "s".into(),
        },
        AgentRunner { run_id: "r".into() },
        Scheduler {
            schedule_id: "sch".into(),
        },
    ];
}

#[test]
fn actor_kind_returns_caller_type() {
    assert_eq!(Actor::Cli { user: "u".into() }.kind(), "cli");
    assert_eq!(
        Actor::Mcp {
            session_id: "s".into()
        }
        .kind(),
        "mcp"
    );
    assert_eq!(Actor::AgentRunner { run_id: "r".into() }.kind(), "agent_runner");
    assert_eq!(
        Actor::Scheduler {
            schedule_id: "sch".into()
        }
        .kind(),
        "scheduler"
    );
}

#[test]
fn actor_id_returns_inner_string() {
    assert_eq!(
        Actor::Cli {
            user: "operator".into()
        }
        .id(),
        "operator"
    );
    assert_eq!(
        Actor::Mcp {
            session_id: "session-42".into()
        }
        .id(),
        "session-42"
    );
}

// ── ApiContext::open helper ─────────────────────────────────────────────

#[tokio::test]
async fn api_context_open_creates_db_and_runs_migrations() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "operator".into(),
        },
    )
    .await
    .expect("open must succeed against a fresh xvn_home");
    assert_eq!(ctx.xvn_home, dir.path());

    // Migrations 001 (api_audit) and 002 (eval_runs) must both have run —
    // querying both tables must not error.
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM api_audit")
        .fetch_one(&ctx.db)
        .await
        .expect("api_audit must exist (migration 001)");
    assert_eq!(count.0, 0);
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM eval_runs")
        .fetch_one(&ctx.db)
        .await
        .expect("eval_runs must exist (migration 002)");
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn api_context_open_creates_xvn_db_file_under_xvn_home() {
    let dir = tempfile::tempdir().unwrap();
    let _ctx = ApiContext::open(dir.path(), Actor::Cli { user: "u".into() })
        .await
        .unwrap();
    let db_path = dir.path().join("xvn.db");
    assert!(db_path.exists(), "xvn.db should exist under xvn_home after open");
}

#[tokio::test]
async fn api_context_open_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    // First open creates + migrates.
    let _a = ApiContext::open(dir.path(), Actor::Cli { user: "u".into() })
        .await
        .unwrap();
    // Second open against the same xvn_home must not error (migrations are
    // already applied; sqlx::migrate is idempotent on already-applied steps).
    let _b = ApiContext::open(dir.path(), Actor::Cli { user: "u".into() })
        .await
        .expect("second open against the same xvn_home must succeed");
}

#[tokio::test]
async fn api_context_open_accepts_already_renamed_eval_agent_schema() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("xvn.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePool::connect(&url).await.unwrap();
    sqlx::query(
        "CREATE TABLE eval_runs (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            scenario_id TEXT NOT NULL,
            mode TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at TEXT,
            completed_at TEXT,
            metrics_json TEXT,
            error TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE eval_attestations (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            scenario_id TEXT NOT NULL,
            signed_at TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            signature_hex TEXT NOT NULL,
            public_key_hex TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "u".into() })
        .await
        .expect("open must not try to rename missing strategy_bundle_hash");

    let columns: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(eval_runs)")
            .fetch_all(&ctx.db)
            .await
            .unwrap();
    assert!(columns.iter().any(|(_, name, _, _, _, _)| name == "agent_id"));
    assert!(!columns
        .iter()
        .any(|(_, name, _, _, _, _)| name == "strategy_bundle_hash"));
}

#[tokio::test]
async fn api_context_open_creates_xvn_home_dir_if_missing() {
    let parent = tempfile::tempdir().unwrap();
    let nested = parent.path().join("nested/.xvn");
    let _ctx = ApiContext::open(&nested, Actor::Cli { user: "u".into() })
        .await
        .expect("open must create xvn_home if it doesn't exist");
    assert!(nested.exists());
    assert!(nested.join("xvn.db").exists());
}
