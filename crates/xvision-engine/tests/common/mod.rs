use tempfile::TempDir;
use xvision_engine::api::{Actor, ApiContext};

#[allow(dead_code)]
async fn execute_migration(pool: &sqlx::SqlitePool, sql: &str) {
    sqlx::query(sql).execute(pool).await.expect("apply migration");
}

#[allow(dead_code)]
pub async fn open_api_context() -> (ApiContext, TempDir) {
    let dir = tempfile::tempdir().expect("create test xvn_home");
    let ctx = ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "operator".into(),
        },
    )
    .await
    .expect("open migrated ApiContext");
    (ctx, dir)
}

#[allow(dead_code)]
pub async fn seeded_scenario_id(ctx: &ApiContext) -> String {
    let (id,): (String,) = sqlx::query_as("SELECT id FROM scenarios ORDER BY id LIMIT 1")
        .fetch_one(&ctx.db)
        .await
        .expect("seeded scenario id");
    id
}

#[allow(dead_code)]
pub async fn open_legacy_eval_run_context() -> (ApiContext, TempDir) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open in-memory sqlite");
    execute_migration(&pool, include_str!("../../migrations/001_api_audit.sql")).await;
    execute_migration(&pool, include_str!("../../migrations/002_eval.sql")).await;
    execute_migration(&pool, include_str!("../../migrations/014_eval_agent_id.sql")).await;
    execute_migration(
        &pool,
        include_str!("../../migrations/022_eval_runs_agents_agent_id.sql"),
    )
    .await;
    execute_migration(&pool, include_str!("../../migrations/027_run_bars_manifest.sql")).await;
    execute_migration(
        &pool,
        include_str!("../../migrations/015_eval_decisions_reasoning.sql"),
    )
    .await;

    let dir = tempfile::tempdir().expect("create test xvn_home");
    std::fs::create_dir_all(dir.path().join("strategies")).expect("create strategies dir");
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    (ctx, dir)
}

#[allow(dead_code)]
pub async fn open_legacy_eval_run_context_with_agent_tables() -> (ApiContext, TempDir) {
    let (ctx, dir) = open_legacy_eval_run_context().await;
    execute_migration(&ctx.db, include_str!("../../migrations/005_agents.sql")).await;
    execute_migration(
        &ctx.db,
        include_str!("../../migrations/019_agent_slot_prompt_version.sql"),
    )
    .await;
    execute_migration(
        &ctx.db,
        include_str!("../../migrations/020_agent_slot_inputs_policy.sql"),
    )
    .await;
    execute_migration(
        &ctx.db,
        include_str!("../../migrations/025_agent_slot_cache_and_window.sql"),
    )
    .await;
    (ctx, dir)
}
