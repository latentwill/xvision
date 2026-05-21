use sqlx::SqlitePool;

/// Apply every migration that touches eval-review state. Mirrors the
/// prefix `ApiContext::open` walks at startup so eval-review integration
/// tests do not each curate their own schema list.
pub async fn eval_review_pool_with_migrations() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    for sql in [
        include_str!("../../migrations/002_eval.sql"),
        include_str!("../../migrations/014_eval_agent_id.sql"),
        include_str!("../../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../../migrations/016_eval_reviews.sql"),
        include_str!("../../migrations/017_eval_findings_review_columns.sql"),
        include_str!("../../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../../migrations/026_trace_surface_foundation.sql"),
        include_str!("../../migrations/027_run_bars_manifest.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}
