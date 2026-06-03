#![allow(dead_code, deprecated)]

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::{canonical_scenarios, scenario_store};

pub async fn api_eval_run_context() -> (ApiContext, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("strategies")).unwrap();
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "operator".into(),
        },
    )
    .await
    .unwrap();
    seed_flash_scenario(&ctx).await;
    (ctx, dir)
}

pub async fn api_eval_run_context_with_agents() -> (ApiContext, tempfile::TempDir) {
    api_eval_run_context().await
}

async fn seed_flash_scenario(ctx: &ApiContext) {
    let scenario = canonical_scenarios()
        .into_iter()
        .find(|s| s.id == "flash-crash-2024-08")
        .expect("flash-crash canonical scenario must exist");
    scenario_store::insert_scenario(ctx, &scenario).await.unwrap();
}

/// Apply every migration that touches eval-review state. Mirrors the
/// prefix `ApiContext::open` walks at startup so eval-review integration
/// tests do not each curate their own schema list.
#[allow(dead_code)]
pub async fn eval_review_pool_with_migrations() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    for sql in [
        include_str!("../../migrations/002_eval.sql"),
        include_str!("../../migrations/013_cli_jobs.sql"),
        include_str!("../../migrations/014_eval_agent_id.sql"),
        include_str!("../../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../../migrations/016_eval_reviews.sql"),
        include_str!("../../migrations/017_eval_findings_review_columns.sql"),
        include_str!("../../migrations/018_agent_run_observability.sql"),
        include_str!("../../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../../migrations/026_trace_surface_foundation.sql"),
        include_str!("../../migrations/027_run_bars_manifest.sql"),
        include_str!("../../migrations/037_review_annotations_and_autofire.sql"),
        include_str!("../../migrations/038_eval_runs_live_config.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}

/// Apply the safety-state schema for safety integration tests.
#[allow(dead_code)]
pub async fn safety_pool_with_migrations() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../../migrations/030_safety_state_and_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}
