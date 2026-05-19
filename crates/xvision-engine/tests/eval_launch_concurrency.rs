//! Integration tests for the per-(agent_id) concurrency cap at `eval.start`.
//!
//! These tests verify:
//! 1. When the DB has N in-flight (queued or running) runs for `agent_id`,
//!    the (N+1)th `start_run` call is rejected with `ApiError::Conflict`.
//! 2. A fresh `start_run` with a *different* `agent_id` succeeds even when
//!    the original slot is saturated.
//!
//! We use the low-level `enforce_concurrency_cap` helper + `RunStore` directly
//! to avoid spinning up the full executor stack (no fixture bars needed).

use sqlx::sqlite::SqlitePoolOptions;
use xvision_engine::api::ApiError;
use xvision_engine::eval::concurrency::{enforce_concurrency_cap, DEFAULT_PROVIDER_MODEL_CONCURRENCY};
use xvision_engine::eval::run::{Run, RunMode, RunStatus};
use xvision_engine::eval::store::RunStore;

async fn pool_with_eval_tables() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

/// Helper: insert N runs for the given agent_id with the given status.
async fn insert_runs(store: &RunStore, agent_id: &str, n: usize, status: RunStatus) {
    for _ in 0..n {
        let run = Run::new_queued(agent_id.to_string(), "scenario-a".to_string(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        if status != RunStatus::Queued {
            store.update_status(&run.id, status, None).await.unwrap();
        }
    }
}

// ── test 1 ──────────────────────────────────────────────────────────────────

/// Filling the slot with `DEFAULT_PROVIDER_MODEL_CONCURRENCY` queued runs
/// causes the next cap check to return Conflict.
#[tokio::test]
async fn cap_reached_returns_conflict_when_slot_full_with_queued() {
    let pool = pool_with_eval_tables().await;
    let store = RunStore::new(pool.clone());
    let agent_id = "agent-cap-test-queued";

    insert_runs(&store, agent_id, DEFAULT_PROVIDER_MODEL_CONCURRENCY, RunStatus::Queued).await;

    let result = enforce_concurrency_cap(&pool, agent_id, "openrouter", "google/gemini-3.1-flash-lite").await;
    assert!(
        matches!(result, Err(ApiError::Conflict(_))),
        "expected Conflict when slot is at cap; got: {result:?}"
    );
}

// ── test 2 ──────────────────────────────────────────────────────────────────

/// Same slot saturated with Running (not Queued) runs also trips the cap.
#[tokio::test]
async fn cap_reached_returns_conflict_when_slot_full_with_running() {
    let pool = pool_with_eval_tables().await;
    let store = RunStore::new(pool.clone());
    let agent_id = "agent-cap-test-running";

    insert_runs(&store, agent_id, DEFAULT_PROVIDER_MODEL_CONCURRENCY, RunStatus::Running).await;

    let result = enforce_concurrency_cap(&pool, agent_id, "openrouter", "google/gemini-3.1-flash-lite").await;
    assert!(
        matches!(result, Err(ApiError::Conflict(_))),
        "expected Conflict when slot is at cap with running runs; got: {result:?}"
    );
}

// ── test 3 ──────────────────────────────────────────────────────────────────

/// A slot with N-1 runs still has capacity — cap check returns Ok.
#[tokio::test]
async fn cap_not_reached_when_one_below_limit() {
    let pool = pool_with_eval_tables().await;
    let store = RunStore::new(pool.clone());
    let agent_id = "agent-cap-test-below";

    insert_runs(
        &store,
        agent_id,
        DEFAULT_PROVIDER_MODEL_CONCURRENCY - 1,
        RunStatus::Queued,
    )
    .await;

    let result = enforce_concurrency_cap(&pool, agent_id, "openrouter", "google/gemini-3.1-flash-lite").await;
    assert!(result.is_ok(), "expected Ok when one slot below cap; got: {result:?}");
}

// ── test 4 ──────────────────────────────────────────────────────────────────

/// A different `agent_id` is independent: saturating agent-A does NOT block
/// a launch for agent-B, even if they nominally share the same provider/model.
#[tokio::test]
async fn different_agent_id_not_blocked_by_saturated_sibling() {
    let pool = pool_with_eval_tables().await;
    let store = RunStore::new(pool.clone());
    let saturated_agent = "agent-saturated";
    let other_agent = "agent-free";

    // Saturate `saturated_agent`.
    insert_runs(&store, saturated_agent, DEFAULT_PROVIDER_MODEL_CONCURRENCY, RunStatus::Running).await;

    // `other_agent` has no in-flight runs — should be allowed.
    let result = enforce_concurrency_cap(&pool, other_agent, "openrouter", "google/gemini-3.1-flash-lite").await;
    assert!(
        result.is_ok(),
        "expected Ok for a different agent_id even when sibling is saturated; got: {result:?}"
    );
}

// ── test 5 ──────────────────────────────────────────────────────────────────

/// Terminal runs (completed / failed / cancelled) do NOT count toward the cap.
#[tokio::test]
async fn terminal_runs_do_not_count_toward_cap() {
    let pool = pool_with_eval_tables().await;
    let store = RunStore::new(pool.clone());
    let agent_id = "agent-cap-test-terminal";

    // Insert more than cap-many terminal runs — they should not count.
    insert_runs(&store, agent_id, DEFAULT_PROVIDER_MODEL_CONCURRENCY + 2, RunStatus::Completed).await;
    insert_runs(&store, agent_id, DEFAULT_PROVIDER_MODEL_CONCURRENCY + 2, RunStatus::Failed).await;
    insert_runs(&store, agent_id, DEFAULT_PROVIDER_MODEL_CONCURRENCY + 2, RunStatus::Cancelled).await;

    // Slot is empty for in-flight purposes — cap check must pass.
    let result = enforce_concurrency_cap(&pool, agent_id, "openrouter", "google/gemini-3.1-flash-lite").await;
    assert!(
        result.is_ok(),
        "expected Ok when all existing runs are terminal; got: {result:?}"
    );
}
