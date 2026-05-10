use sqlx::SqlitePool;
use xvision_engine::api::{Actor, ApiContext};

#[tokio::test]
async fn api_context_constructs_with_actor() {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext {
        db: pool,
        actor: Actor::Cli {
            user: "operator".into(),
        },
        xvn_home: dir.path().to_path_buf(),
    };
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
        AgentRunner {
            run_id: "r".into(),
        },
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
    assert_eq!(
        Actor::AgentRunner {
            run_id: "r".into()
        }
        .kind(),
        "agent_runner"
    );
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
