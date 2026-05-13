//! Verify the canonical scenario seed runs on every fresh xvn_home
//! (idempotent — re-opening the same home keeps the count at 4, not 8).

use tempfile::tempdir;
use xvision_engine::api::{Actor, ApiContext};

#[tokio::test]
async fn seed_runs_on_fresh_db() {
    let dir = tempdir().unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "test".into(),
        },
    )
    .await
    .unwrap();

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM scenarios WHERE source = 'canonical'")
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    assert_eq!(count.0, 4, "expected 4 canonical scenarios seeded");

    let legacy_strategy_path = dir
        .path()
        .join("strategies")
        .join(["bun", "dle", "-canonical", "-defaults", ".json"].concat());
    assert!(
        !legacy_strategy_path.exists(),
        "fresh homes must not seed the legacy default strategy at {}",
        legacy_strategy_path.display()
    );
}

#[tokio::test]
async fn seed_is_idempotent_across_reopens() {
    let dir = tempdir().unwrap();
    let _ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "test".into(),
        },
    )
    .await
    .unwrap();
    let ctx2 = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "test".into(),
        },
    )
    .await
    .unwrap();

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM scenarios WHERE source = 'canonical'")
            .fetch_one(&ctx2.db)
            .await
            .unwrap();
    assert_eq!(count.0, 4, "seed must not double-insert on reopen");
}
