//! Tests for the soft-delete + undo-forget semantics added in
//! `memory-forget-undo-snapshot`. The contract specifies five
//! behaviors:
//!
//! 1. `forget()` is a soft-delete (UPDATE, not DELETE) under the
//!    default grace window.
//! 2. `query()` (and the public read paths) skip rows whose
//!    `forgotten_at` is non-null.
//! 3. `undo_forget(namespace, since)` restores rows whose
//!    `forgotten_at` falls inside the window; rows outside the window
//!    are not restored.
//! 4. `hard_delete_expired(grace_days)` hard-deletes rows whose
//!    `forgotten_at` is older than `grace_days`.
//! 5. `XVN_MEMORY_FORGET_GRACE_DAYS=0` collapses `forget()` to an
//!    immediate hard-delete (opt-out semantics matching V2D's prior
//!    destructive behavior).

use chrono::{Duration, Utc};
use tokio::sync::Mutex;
use xvision_memory::store::{MemoryStore, FORGET_GRACE_ENV};
use xvision_memory::types::{MemoryItem, Tier};

/// Tests in this file mutate `XVN_MEMORY_FORGET_GRACE_DAYS` to exercise
/// the opt-out path. Env mutations are process-wide, so we serialize
/// every test under a single async mutex — running them in parallel
/// would race the `set_var`/`remove_var` calls across the test threads.
/// `cargo test` defaults to a multi-thread runner, so this guard is
/// load-bearing. `tokio::sync::Mutex` (rather than `std::sync::Mutex`)
/// so clippy's `await_holding_lock` lint is satisfied for the
/// guard held across the `.await`s in each test body.
static ENV_GUARD: Mutex<()> = Mutex::const_new(());

fn pattern(id: &str, ns: &str, text: &str, emb: Vec<f32>) -> MemoryItem {
    MemoryItem {
        id: id.into(),
        namespace: ns.into(),
        tier: Tier::Pattern,
        text: text.into(),
        embedding: emb,
        created_at: Utc::now(),
        run_id: None,
        scenario_id: None,
        cycle_idx: None,
        source_window_start: None,
        source_window_end: None,
        training_window_end: None,
        promotion_state: Some("active".into()),
        attestation_id: Some("attest-test".into()),
        forgotten_at: None,
    }
}

/// Case 1 — `forget()` soft-deletes by default (rows remain in the
/// table with `forgotten_at` set) and `query()` no longer sees them.
#[tokio::test]
async fn forget_default_is_soft_delete_and_query_skips() {
    let _guard = ENV_GUARD.lock().await;
    std::env::remove_var(FORGET_GRACE_ENV);

    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .upsert_pattern(&pattern("p1", "agent:A", "alpha", vec![1.0, 0.0]), "test")
        .await
        .unwrap();
    store
        .upsert_pattern(&pattern("p2", "agent:A", "beta", vec![0.0, 1.0]), "test")
        .await
        .unwrap();

    // Sanity: rows are visible before forget.
    let hits = store.query("agent:A", &[1.0, 0.0], 5, None).await.unwrap();
    assert_eq!(hits.len(), 2);

    let affected = store.forget("agent:A").await.unwrap();
    assert_eq!(affected, 2, "soft-delete must touch both rows");

    // Rows still in the table — count_forgotten sees them.
    let forgotten = store.count_forgotten("agent:A").await.unwrap();
    assert_eq!(forgotten, 2, "rows persist after soft-delete");

    // But query skips them.
    let hits = store.query("agent:A", &[1.0, 0.0], 5, None).await.unwrap();
    assert!(hits.is_empty(), "query must skip rows with non-null forgotten_at");
}

/// Case 2 — `undo_forget(ns, since)` restores rows whose
/// `forgotten_at >= since`; rows outside the window stay forgotten.
#[tokio::test]
async fn undo_forget_restores_within_window_only() {
    let _guard = ENV_GUARD.lock().await;
    std::env::remove_var(FORGET_GRACE_ENV);

    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .upsert_pattern(&pattern("p1", "agent:A", "alpha", vec![1.0, 0.0]), "test")
        .await
        .unwrap();
    store
        .upsert_pattern(&pattern("p2", "agent:A", "beta", vec![0.0, 1.0]), "test")
        .await
        .unwrap();

    // Forget at a fixed, deterministic timestamp.
    let now = Utc::now();
    let forgotten_count = store.forget_at("agent:A", now).await.unwrap();
    assert_eq!(forgotten_count, 2);

    // since strictly newer than now → no rows are inside the window.
    let restored = store
        .undo_forget("agent:A", now + Duration::seconds(1))
        .await
        .unwrap();
    assert_eq!(
        restored, 0,
        "rows whose forgotten_at < since must not be restored"
    );

    // since equal to the forget timestamp → both rows restored.
    let restored = store.undo_forget("agent:A", now).await.unwrap();
    assert_eq!(restored, 2, "rows at the lower bound must restore");

    // Rows are visible again.
    let hits = store.query("agent:A", &[1.0, 0.0], 5, None).await.unwrap();
    assert_eq!(hits.len(), 2);
    let still_forgotten = store.count_forgotten("agent:A").await.unwrap();
    assert_eq!(still_forgotten, 0);
}

/// Case 3 — `hard_delete_expired` removes only rows whose
/// `forgotten_at` is older than the grace window.
#[tokio::test]
async fn hard_delete_expired_only_clears_aged_rows() {
    let _guard = ENV_GUARD.lock().await;
    std::env::remove_var(FORGET_GRACE_ENV);

    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .upsert_pattern(&pattern("old", "agent:A", "old", vec![1.0, 0.0]), "test")
        .await
        .unwrap();
    store
        .upsert_pattern(&pattern("new", "agent:A", "new", vec![0.0, 1.0]), "test")
        .await
        .unwrap();

    let now = Utc::now();
    // "old" forgotten 30 days ago; "new" forgotten just now.
    store
        .forget_at("agent:A", now - Duration::days(30))
        .await
        .unwrap();
    // The above marked BOTH rows because they were live. Re-stamp only
    // the "new" id back to live, then forget it at `now` to get the
    // mixed ages we want.
    sqlx::query("UPDATE memory_items SET forgotten_at = NULL WHERE id = 'new'")
        .execute(store.pool())
        .await
        .unwrap();
    store.forget_at("agent:A", now).await.unwrap();

    // Janitor with 14-day grace at the same `now` → "old" goes, "new" stays.
    let deleted = store.hard_delete_expired_at(14, now).await.unwrap();
    assert_eq!(deleted, 1, "only rows outside the grace window are deleted");

    // "new" still soft-deleted (one row left in the namespace).
    let forgotten = store.count_forgotten("agent:A").await.unwrap();
    assert_eq!(forgotten, 1);

    // Restoring with a window that covers `now` revives "new".
    let restored = store
        .undo_forget("agent:A", now - Duration::days(1))
        .await
        .unwrap();
    assert_eq!(restored, 1);
    let hits = store.query("agent:A", &[0.0, 1.0], 5, None).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "new");
}

/// Case 4 — soft-delete is scoped by namespace. `forget("agent:A")` must
/// not touch rows in `global`, and `undo_forget` must restore only the
/// matching namespace.
#[tokio::test]
async fn forget_and_undo_are_namespace_scoped() {
    let _guard = ENV_GUARD.lock().await;
    std::env::remove_var(FORGET_GRACE_ENV);

    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .upsert_pattern(&pattern("a1", "agent:A", "alpha", vec![1.0, 0.0]), "test")
        .await
        .unwrap();
    store
        .upsert_pattern(&pattern("g1", "global", "gamma", vec![1.0, 0.0]), "test")
        .await
        .unwrap();

    let now = Utc::now();
    let affected = store.forget_at("agent:A", now).await.unwrap();
    assert_eq!(affected, 1);

    // global is untouched.
    let g_forgotten = store.count_forgotten("global").await.unwrap();
    assert_eq!(g_forgotten, 0);
    let g_hits = store.query("global", &[1.0, 0.0], 5, None).await.unwrap();
    assert_eq!(g_hits.len(), 1);

    // undo_forget on global is a no-op.
    let restored = store
        .undo_forget("global", now - Duration::days(1))
        .await
        .unwrap();
    assert_eq!(restored, 0);

    // undo_forget on agent:A restores its row.
    let restored = store
        .undo_forget("agent:A", now - Duration::days(1))
        .await
        .unwrap();
    assert_eq!(restored, 1);
}

/// Case 5 — `XVN_MEMORY_FORGET_GRACE_DAYS=0` collapses `forget()` to
/// immediate hard-delete (opt-out matching V2D's prior destructive
/// behavior). Rows leave the table — `undo_forget` cannot bring them
/// back.
///
/// **Concurrency note:** this test mutates a process-wide env var.
/// Other tests in this file `remove_var` it on entry so a concurrent
/// run cannot leak `0` into them, but if tests are run on a single
/// thread (`--test-threads=1`) we restore the var on exit too.
#[tokio::test]
async fn forget_grace_zero_is_immediate_hard_delete() {
    let _guard = ENV_GUARD.lock().await;
    // SAFETY: env mutation is process-wide. Set, run, unset.
    std::env::set_var(FORGET_GRACE_ENV, "0");

    let result = async {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .upsert_pattern(&pattern("p1", "agent:A", "alpha", vec![1.0, 0.0]), "test")
            .await
            .unwrap();
        let affected = store.forget("agent:A").await.unwrap();
        assert_eq!(affected, 1);

        // No row left — undo cannot recover.
        let forgotten = store.count_forgotten("agent:A").await.unwrap();
        assert_eq!(forgotten, 0, "GRACE_DAYS=0 must hard-delete");
        let restored = store
            .undo_forget("agent:A", Utc::now() - Duration::days(365))
            .await
            .unwrap();
        assert_eq!(restored, 0);
    }
    .await;

    std::env::remove_var(FORGET_GRACE_ENV);
    result
}
