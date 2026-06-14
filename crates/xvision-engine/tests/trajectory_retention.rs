//! Trajectory retention tests (item 9).
//!
//! Exercises: TTL on begin_recording, purge_expired (deletes recordings +
//! cascades frames + GCs blobs), and compact (nulls payload_refs for
//! non-complete recordings while keeping hashes).

use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use uuid::Uuid;
use xvision_observability::trajectory::frame::TrajectoryFrame;
use xvision_observability::trajectory::key::{TrajectoryKey, TRAJECTORY_SCHEMA_VERSION};
use xvision_observability::trajectory::store::TrajectoryStore;
use xvision_observability::{BlobStore, RetentionMode};

async fn open_store_with_ttl(tmp: &TempDir, mode: RetentionMode, ttl_secs: u64) -> TrajectoryStore {
    let db_path = tmp.path().join("test_retention.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("open sqlite");

    apply_schema(&pool).await;

    let blob_root = tmp.path().join("blobs");
    let blob = BlobStore::new(blob_root);
    TrajectoryStore::new(pool, blob, mode).with_ttl(ttl_secs)
}

async fn open_store(tmp: &TempDir, mode: RetentionMode) -> TrajectoryStore {
    let db_path = tmp.path().join("test_retention2.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("open sqlite");

    apply_schema(&pool).await;

    let blob_root = tmp.path().join("blobs2");
    let blob = BlobStore::new(blob_root);
    TrajectoryStore::new(pool, blob, mode)
}

async fn apply_schema(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS trajectory_recordings (
          recording_id       TEXT PRIMARY KEY,
          schema_version     INTEGER NOT NULL,
          status             TEXT NOT NULL DEFAULT 'open',
          key_fingerprint    TEXT NOT NULL UNIQUE,
          cycle_id           TEXT NOT NULL,
          slot_role          TEXT NOT NULL,
          arm_scope          TEXT,
          simulation_id      TEXT,
          provider           TEXT NOT NULL,
          model              TEXT NOT NULL,
          model_version      TEXT,
          system_prompt_hash TEXT NOT NULL,
          recovery_reason    TEXT,
          created_at         INTEGER NOT NULL,
          completed_at       INTEGER,
          expires_at         INTEGER
        )",
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS trajectory_frames (
          recording_id  TEXT    NOT NULL REFERENCES trajectory_recordings(recording_id) ON DELETE CASCADE,
          slot_role     TEXT    NOT NULL,
          step_index    INTEGER NOT NULL,
          frame_index   INTEGER NOT NULL,
          frame_kind    TEXT    NOT NULL,
          ts_ms         INTEGER NOT NULL,
          payload_hash  TEXT    NOT NULL,
          payload_ref   TEXT,
          PRIMARY KEY (recording_id, slot_role, step_index, frame_index)
        )",
    )
    .execute(pool)
    .await
    .unwrap();
}

fn make_key(slot: &str) -> TrajectoryKey {
    TrajectoryKey::builder()
        .cycle_id(Uuid::new_v4())
        .slot_role(slot)
        .arm_scope(None::<String>)
        .simulation_id(None::<String>)
        .provider("anthropic")
        .model("claude-opus-4-7")
        .model_version("2026-05")
        .schema_version(TRAJECTORY_SCHEMA_VERSION)
        .system_prompt_hash("sys")
        .user_prompt_hash("usr")
        .build()
}

#[tokio::test]
async fn begin_recording_sets_expires_at() {
    let tmp = TempDir::new().unwrap();
    let store = open_store_with_ttl(&tmp, RetentionMode::FullDebug, 3600).await;

    let key = make_key("trader");
    let before_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let rid = store.begin_recording(&key).await.unwrap();

    let info = store.get_recording(rid.as_str()).await.unwrap();
    let expires_at = info
        .expires_at
        .expect("expires_at must be set when TTL is configured");

    // expires_at should be roughly created_at + 3600 * 1000ms
    let expected_min = before_ms + 3600 * 1000 - 100; // allow 100ms slack
    assert!(
        expires_at >= expected_min,
        "expires_at {expires_at} must be >= {expected_min}"
    );
}

#[tokio::test]
async fn purge_expired_deletes_past_ttl() {
    let tmp = TempDir::new().unwrap();
    let store = open_store_with_ttl(&tmp, RetentionMode::FullDebug, 1).await;

    let key = make_key("trader");
    let rid = store.begin_recording(&key).await.unwrap();
    let f = TrajectoryFrame::TextDelta {
        ts_ms: 1,
        text: "test".into(),
    };
    store.append_frame(&rid, "trader", 0, 0, &f).await.unwrap();
    store.complete_recording(&rid).await.unwrap();

    let info = store.get_recording(rid.as_str()).await.unwrap();
    // The recording was just created; it should have an expires_at.
    assert!(info.expires_at.is_some());

    // Purge at a time well past the TTL (expires_at + 1 hour).
    let far_future = info.expires_at.unwrap() + 3_600_000;
    let deleted = store.purge_expired(far_future).await.unwrap();
    assert_eq!(deleted, 1, "one recording should be deleted");

    // Should not be findable anymore.
    let result = store.get_recording(rid.as_str()).await;
    assert!(result.is_err(), "recording must be gone after purge");
}

#[tokio::test]
async fn purge_expired_does_not_delete_fresh_recordings() {
    let tmp = TempDir::new().unwrap();
    let store = open_store_with_ttl(&tmp, RetentionMode::FullDebug, 86400).await; // 1 day TTL

    let key = make_key("regime");
    let rid = store.begin_recording(&key).await.unwrap();
    store.complete_recording(&rid).await.unwrap();

    // Purge with "now" = just now (should not touch recordings that expire tomorrow).
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let deleted = store.purge_expired(now_ms).await.unwrap();
    assert_eq!(deleted, 0, "fresh recording must not be purged");

    // Still findable.
    store.get_recording(rid.as_str()).await.unwrap();
}

#[tokio::test]
async fn compact_nulls_payload_refs_for_noncomplete() {
    let tmp = TempDir::new().unwrap();
    let store = open_store(&tmp, RetentionMode::FullDebug).await;

    let key = make_key("risk");
    let rid = store.begin_recording(&key).await.unwrap();
    let f = TrajectoryFrame::Usage {
        ts_ms: 1,
        input_tokens: 10,
        output_tokens: 5,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        total_cost: 0.001,
    };
    store.append_frame(&rid, "risk", 0, 0, &f).await.unwrap();
    // Deliberately do NOT complete — leave as 'open'.

    let nulled = store.compact().await.unwrap();
    assert_eq!(nulled, 1, "one frame's payload_ref should be nulled out");
}
