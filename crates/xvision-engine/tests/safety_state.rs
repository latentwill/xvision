//! Safety state tests — pause/resume toggles, in-memory consistency.
//! All tests use in-memory SQLite with the safety migration applied.

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_engine::safety::{AuthContext, SafetyManager};

async fn safety_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/030_safety_state_and_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

fn system_auth() -> AuthContext {
    AuthContext::system()
}

fn anon() -> AuthContext {
    AuthContext::api_anonymous()
}

#[tokio::test]
async fn safety_bootstrap_seeds_unpaused_on_fresh_paper_install() {
    let pool = safety_pool().await;
    let manager = SafetyManager::new(pool);
    manager.bootstrap(false).await.unwrap();

    let state = manager.current().await;
    assert!(!state.paused, "fresh paper install must start unpaused");
}

#[tokio::test]
async fn safety_bootstrap_seeds_paused_on_fresh_live_install() {
    let pool = safety_pool().await;
    let manager = SafetyManager::new(pool);
    manager.bootstrap(true).await.unwrap();

    let state = manager.current().await;
    assert!(state.paused, "fresh live install must start paused");
    assert_eq!(state.paused_by.as_deref(), Some("system"));
}

#[tokio::test]
async fn safety_bootstrap_idempotent_on_second_call() {
    let pool = safety_pool().await;
    let manager = SafetyManager::new(pool);
    // First bootstrap with live → paused.
    manager.bootstrap(true).await.unwrap();
    // Second bootstrap with paper → must NOT override the existing state
    // (row already exists → read path, not seed path).
    manager.bootstrap(false).await.unwrap();

    let state = manager.current().await;
    assert!(state.paused, "second bootstrap must not overwrite existing state");
}

#[tokio::test]
async fn pause_then_resume_toggles_state() {
    let pool = safety_pool().await;
    let manager = SafetyManager::new(pool);
    manager.bootstrap(false).await.unwrap();
    let auth = anon();

    // Initially unpaused.
    assert!(!manager.is_paused().await);

    // Pause.
    let paused = manager.pause(Some("test pause".into()), &auth).await.unwrap();
    assert!(paused.paused);
    assert_eq!(paused.reason.as_deref(), Some("test pause"));
    assert!(manager.is_paused().await);

    // Resume.
    let resumed = manager.resume(None, &auth).await.unwrap();
    assert!(!resumed.paused);
    assert!(!manager.is_paused().await);
}

#[tokio::test]
async fn pause_state_persists_to_db() {
    // Use a shared pool to simulate persistence across manager instances.
    let pool = safety_pool().await;

    // Pause via first manager.
    let mgr1 = SafetyManager::new(pool.clone());
    mgr1.bootstrap(false).await.unwrap();
    mgr1.pause(Some("persist test".into()), &system_auth())
        .await
        .unwrap();

    // Build a new manager over the same pool — must load persisted state.
    let mgr2 = SafetyManager::new(pool.clone());
    mgr2.bootstrap(false).await.unwrap();

    let state = mgr2.current().await;
    assert!(state.paused, "paused state must persist across manager instances");
    assert_eq!(state.reason.as_deref(), Some("persist test"));
}

#[tokio::test]
async fn audit_row_written_on_toggle() {
    let pool = safety_pool().await;
    let manager = SafetyManager::new(pool);
    manager.bootstrap(false).await.unwrap();
    let auth = anon();

    manager.pause(Some("audit test".into()), &auth).await.unwrap();

    let rows = manager.audit_writer().list(10).await.unwrap();
    assert!(!rows.is_empty(), "audit row must be written on pause");
    let last = &rows[0]; // newest first
    assert_eq!(last.action_kind, "pause_toggle");
    assert_eq!(last.result, "allowed");
    assert!(last.pause_state_at_time, "pause state must be true after pause");
}
